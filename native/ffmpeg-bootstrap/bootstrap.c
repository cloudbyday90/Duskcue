#define _GNU_SOURCE
#include <errno.h>
#include <fcntl.h>
#include <linux/audit.h>
#include <linux/filter.h>
#include <linux/seccomp.h>
#include <stdint.h>
#include <stdlib.h>
#include <string.h>
#include <sys/prctl.h>
#include <sys/stat.h>
#include <sys/syscall.h>
#include <unistd.h>

#if defined(__x86_64__)
#define DUSKCUE_ARCH AUDIT_ARCH_X86_64
#elif defined(__aarch64__)
#define DUSKCUE_ARCH AUDIT_ARCH_AARCH64
#else
#error unsupported_duskcue_bootstrap_architecture
#endif

#if __BYTE_ORDER__ != __ORDER_LITTLE_ENDIAN__
#error unsupported_duskcue_bootstrap_byte_order
#endif

_Static_assert(sizeof(struct sock_filter) == 8, "invalid_duskcue_bpf_instruction_size");

#define DUSKCUE_HEADER_BYTES 40
#define DUSKCUE_MAX_INSTRUCTIONS 4096

static void fail_closed(void) {
    _exit(126);
}

static uint32_t read32(const unsigned char *bytes) {
    return (uint32_t)bytes[0] | ((uint32_t)bytes[1] << 8) |
           ((uint32_t)bytes[2] << 16) | ((uint32_t)bytes[3] << 24);
}

static uint64_t read64(const unsigned char *bytes) {
    return (uint64_t)read32(bytes) | ((uint64_t)read32(bytes + 4) << 32);
}

static int descriptor(const char *name) {
    const char *text = getenv(name);
    unsigned int value = 0;
    if (text == NULL || *text == '\0') fail_closed();
    for (unsigned int index = 0; text[index] != '\0'; index++) {
        if (index >= 9 || text[index] < '0' || text[index] > '9') fail_closed();
        value = value * 10 + (unsigned int)(text[index] - '0');
        if (value > 100000000) fail_closed();
    }
    if (value < 3) fail_closed();
    return (int)value;
}

static void read_exact(int fd, unsigned char *bytes, size_t size, off_t offset) {
    size_t completed = 0;
    unsigned int interruptions = 0;
    while (completed < size) {
        ssize_t count = pread(fd, bytes + completed, size - completed, offset + (off_t)completed);
        if (count < 0 && errno == EINTR && interruptions++ < 8) continue;
        if (count <= 0) fail_closed();
        completed += (size_t)count;
    }
}

static uint64_t checksum(const unsigned char *bytes, size_t size, uint64_t hash) {
    for (size_t index = 0; index < size; index++) {
        hash = (hash ^ bytes[index]) * UINT64_C(0x100000001b3);
    }
    return hash;
}

static void acknowledge(int fd, const unsigned char *header) {
    unsigned char bytes[24] = {'D', 'U', 'S', 'K', 'A', 'C', 'K', '1'};
    memcpy(bytes + 8, header + 24, 8);
    memcpy(bytes + 16, header + 12, 4);
    bytes[20] = 1;
    for (unsigned int attempt = 0; attempt < 8; attempt++) {
        ssize_t count = write(fd, bytes, sizeof(bytes));
        if (count == (ssize_t)sizeof(bytes)) return;
        if (count < 0 && errno == EINTR) continue;
        fail_closed();
    }
    fail_closed();
}

__attribute__((constructor)) static void install_duskcue_policy(void) {
    unsigned char header[DUSKCUE_HEADER_BYTES];
    struct sock_filter instructions[DUSKCUE_MAX_INSTRUCTIONS];
    struct stat policy_status;
    struct stat acknowledgment_status;
    int policy_fd = descriptor("DUSKCUE_FFMPEG_POLICY_FD");
    int acknowledgment_fd = descriptor("DUSKCUE_FFMPEG_ACK_FD");
    if (policy_fd == acknowledgment_fd || unsetenv("DUSKCUE_FFMPEG_POLICY_FD") != 0 ||
        unsetenv("DUSKCUE_FFMPEG_ACK_FD") != 0) fail_closed();
    if (fstat(policy_fd, &policy_status) != 0 || !S_ISREG(policy_status.st_mode) ||
        fstat(acknowledgment_fd, &acknowledgment_status) != 0 || !S_ISFIFO(acknowledgment_status.st_mode)) fail_closed();
    int write_flags = fcntl(acknowledgment_fd, F_GETFL);
    if (write_flags < 0 || (write_flags & O_ACCMODE) != O_WRONLY || (write_flags & O_NONBLOCK) == 0) fail_closed();
    int required_seals = F_SEAL_WRITE | F_SEAL_GROW | F_SEAL_SHRINK | F_SEAL_SEAL;
    int seals = fcntl(policy_fd, F_GET_SEALS);
    if (seals < 0 || (seals & required_seals) != required_seals) fail_closed();
    read_exact(policy_fd, header, sizeof(header), 0);
    uint32_t count = read32(header + 16);
    if (memcmp(header, "DUSKSC01", 8) != 0 || read32(header + 8) != 1 ||
        read32(header + 12) != DUSKCUE_ARCH || read32(header + 20) != 0 ||
        count == 0 || count > DUSKCUE_MAX_INSTRUCTIONS ||
        policy_status.st_size != (off_t)(DUSKCUE_HEADER_BYTES + count * sizeof(struct sock_filter))) fail_closed();
    read_exact(policy_fd, (unsigned char *)instructions, count * sizeof(struct sock_filter), DUSKCUE_HEADER_BYTES);
    uint64_t expected_checksum = read64(header + 32);
    memset(header + 32, 0, 8);
    uint64_t actual_checksum = checksum(header, sizeof(header), UINT64_C(0xcbf29ce484222325));
    actual_checksum = checksum((unsigned char *)instructions, count * sizeof(struct sock_filter), actual_checksum);
    if (actual_checksum != expected_checksum ||
        prctl(PR_GET_NO_NEW_PRIVS, 0UL, 0UL, 0UL, 0UL) != 1) fail_closed();
    if (close(policy_fd) != 0) fail_closed();
    struct sock_fprog program = {.len = (unsigned short)count, .filter = instructions};
    if (syscall(SYS_seccomp, SECCOMP_SET_MODE_FILTER, SECCOMP_FILTER_FLAG_TSYNC, &program) != 0) fail_closed();
    acknowledge(acknowledgment_fd, header);
    if (close(acknowledgment_fd) != 0) fail_closed();
}
