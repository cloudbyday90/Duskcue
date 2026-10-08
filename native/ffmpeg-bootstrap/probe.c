// Duskcue — Self-hosted media streaming server
// Copyright (C) 2026-2026 Duskcue Contributors
// Licensed under AGPL-3.0. See LICENSE for details.

#define _GNU_SOURCE
#include <errno.h>
#include <fcntl.h>
#include <linux/membarrier.h>
#include <pthread.h>
#include <stdio.h>
#include <string.h>
#include <sys/socket.h>
#include <sys/prctl.h>
#include <sys/resource.h>
#include <sys/syscall.h>
#include <time.h>
#include <unistd.h>

extern char **environ;

static void *finish_owned_thread(void *value) {
    return value;
}

int main(int argc, char **argv) {
    if (argc < 2) return 2;
    if (strcmp(argv[1], "status") == 0) {
        int no_new_privs = prctl(PR_GET_NO_NEW_PRIVS, 0UL, 0UL, 0UL, 0UL);
        int seccomp = prctl(PR_GET_SECCOMP, 0UL, 0UL, 0UL, 0UL);
        char status[64];
        int count = snprintf(status, sizeof(status), "no_new_privs=%d\nseccomp=%d\n", no_new_privs, seccomp);
        if (count <= 0 || (size_t)count >= sizeof(status)) return 4;
        return write(STDOUT_FILENO, status, (size_t)count) == count ? 0 : 5;
    }
    if (strcmp(argv[1], "exec") == 0) {
        char *arguments[] = {"ffmpeg", "-version", NULL};
        syscall(SYS_execve, "/usr/bin/ffmpeg", arguments, environ);
        return 3;
    }
    if (strcmp(argv[1], "execat") == 0) {
        char *arguments[] = {"ffmpeg", "-version", NULL};
        syscall(SYS_execveat, AT_FDCWD, "/usr/bin/ffmpeg", arguments, environ, 0);
        return 3;
    }
    if (strcmp(argv[1], "network") == 0) {
        syscall(SYS_socket, AF_INET, SOCK_STREAM, 0);
        return 3;
    }
    if (strcmp(argv[1], "numa-query") == 0) {
        errno = 0;
        if (syscall(SYS_get_mempolicy, NULL, NULL, 0UL, 0UL, 0UL) != -1 || errno != EPERM) return 4;
        return write(STDOUT_FILENO, "NUMA_QUERY_DENIED\n", 18) == 18 ? 0 : 5;
    }
    if (strcmp(argv[1], "barrier-registration") == 0) {
        const unsigned long ignored_cpu_values[] = {0UL, 1UL, ~0UL};
        for (unsigned int index = 0; index < sizeof(ignored_cpu_values) / sizeof(ignored_cpu_values[0]); index++) {
            errno = 0;
            if (syscall(SYS_membarrier, (long)MEMBARRIER_CMD_REGISTER_PRIVATE_EXPEDITED, 0UL, ignored_cpu_values[index]) != -1 || errno != EPERM) return 4;
        }
        return write(STDOUT_FILENO, "BARRIER_REGISTRATION_DENIED\n", 28) == 28 ? 0 : 5;
    }
    if (strcmp(argv[1], "self-resources") == 0) {
        struct rusage usage = {0};
        if (syscall(SYS_getrusage, (long)RUSAGE_SELF, &usage) != 0 ||
            usage.ru_utime.tv_sec < 0 || usage.ru_stime.tv_sec < 0) return 4;
        return write(STDOUT_FILENO, "SELF_RESOURCE_QUERY_OK\n", 23) == 23 ? 0 : 5;
    }
    if (strcmp(argv[1], "children-resources") == 0) {
        struct rusage usage = {0};
        syscall(SYS_getrusage, (long)RUSAGE_CHILDREN, &usage);
        return 3;
    }
    if (strcmp(argv[1], "thread-exit") == 0) {
        pthread_t worker;
        int marker = 1;
        void *result = NULL;
        if (pthread_create(&worker, NULL, finish_owned_thread, &marker) != 0 ||
            pthread_join(worker, &result) != 0 || result != &marker) return 4;
        return write(STDOUT_FILENO, "OWNED_THREAD_EXIT_OK\n", 21) == 21 ? 0 : 5;
    }
    if ((strcmp(argv[1], "unlink-file") == 0 || strcmp(argv[1], "unlink-legacy") == 0) && argc == 3) {
        int result = strcmp(argv[1], "unlink-legacy") == 0 ? unlink(argv[2]) :
            (int)syscall(SYS_unlinkat, AT_FDCWD, argv[2], 0UL);
        if (result != 0) return 4;
        return write(STDOUT_FILENO, "CACHE_FILE_REMOVED\n", 19) == 19 ? 0 : 5;
    }
    if ((strcmp(argv[1], "unlink-outside") == 0 || strcmp(argv[1], "unlink-legacy-outside") == 0) && argc == 3) {
        errno = 0;
        int result = strcmp(argv[1], "unlink-legacy-outside") == 0 ? unlink(argv[2]) :
            (int)syscall(SYS_unlinkat, AT_FDCWD, argv[2], 0UL);
        if (result != -1 || errno != EACCES) return 4;
        return write(STDOUT_FILENO, "OUTSIDE_REMOVE_DENIED\n", 22) == 22 ? 0 : 5;
    }
    if (strcmp(argv[1], "unlink-directory") == 0 && argc == 3) {
        syscall(SYS_unlinkat, AT_FDCWD, argv[2], (unsigned long)AT_REMOVEDIR);
        return 3;
    }
    if (strcmp(argv[1], "path") == 0 && argc == 3) {
        int input = open(argv[2], O_RDONLY);
        if (input >= 0) { close(input); return 3; }
        if (errno != EACCES) return 4;
        return write(STDOUT_FILENO, "OUTSIDE_DENIED\n", 15) == 15 ? 0 : 5;
    }
    if (strcmp(argv[1], "allowed") == 0 && argc == 4) {
        char byte;
        int input = open(argv[2], O_RDONLY);
        if (input < 0 || read(input, &byte, 1) != 1 || close(input) != 0) return 4;
        int output = open(argv[3], O_CREAT | O_WRONLY | O_TRUNC, 0600);
        if (output < 0 || write(output, &byte, 1) != 1 || close(output) != 0) return 5;
        return write(STDOUT_FILENO, "ALLOWED_SOURCE_CACHE\n", 21) == 21 ? 0 : 6;
    }
    if (strcmp(argv[1], "close-sleep") == 0 && argc == 3) {
        if (write(STDOUT_FILENO, "READY\n", 6) != 6 ||
            close(STDOUT_FILENO) != 0 || close(STDERR_FILENO) != 0) return 5;
        int marker = open(argv[2], O_CREAT | O_WRONLY | O_TRUNC, 0600);
        char pid[32];
        int count = snprintf(pid, sizeof(pid), "%ld\n", (long)getpid());
        if (marker < 0 || count <= 0 || (size_t)count >= sizeof(pid) ||
            write(marker, pid, (size_t)count) != count || close(marker) != 0) return 4;
        struct timespec duration = {.tv_sec = 60};
        while (nanosleep(&duration, &duration) != 0 && errno == EINTR) {}
        return 0;
    }
    if (strcmp(argv[1], "main") == 0) {
        if (argc == 3) {
            int marker = open(argv[2], O_CREAT | O_WRONLY | O_TRUNC, 0600);
            if (marker < 0 || write(marker, "MAIN_REACHED\n", 13) != 13 || close(marker) != 0) return 4;
        }
        return write(STDOUT_FILENO, "MAIN_REACHED\n", 13) == 13 ? 0 : 5;
    }
    return 2;
}
