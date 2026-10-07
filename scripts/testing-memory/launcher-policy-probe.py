import ctypes
import errno
import json
import os
import platform
import signal
import sys

import seccomp


class Ruleset(ctypes.Structure):
    _fields_ = [("handled_access_fs", ctypes.c_uint64)]


class PathRule(ctypes.Structure):
    _fields_ = [("allowed_access", ctypes.c_uint64), ("parent_fd", ctypes.c_int32)]


libc = ctypes.CDLL(None, use_errno=True)
libc.syscall.restype = ctypes.c_long
libc.prctl.restype = ctypes.c_int
libc.open.restype = ctypes.c_int
libc.read.restype = ctypes.c_ssize_t
libc.write.restype = ctypes.c_ssize_t
libc.execve.argtypes = [ctypes.c_char_p, ctypes.POINTER(ctypes.c_char_p), ctypes.POINTER(ctypes.c_char_p)]


def landlock(source, cache, effective_abi):
    if effective_abi == 0:
        if libc.prctl(38, 1, 0, 0, 0) != 0:
            raise RuntimeError("No-new-privileges failed for the unsupported filesystem checkpoint.")
        return
    rights = {1: 8191, 2: 16383, 3: 32767}[effective_abi]
    definition = Ruleset(rights)
    fd = libc.syscall(444, ctypes.byref(definition), ctypes.sizeof(definition), 0)
    if fd < 0:
        raise RuntimeError(f"Landlock creation failed: errno {ctypes.get_errno()}")
    try:
        paths = [(b"/usr", 13), (b"/lib", 13), (b"/etc", 13), (b"/dev/dri", 13), (source, 5), (cache, rights), (b"/tmp", rights)]
        for path, access in paths:
            if not os.path.exists(path):
                continue
            opened = os.open(path, os.O_PATH | os.O_CLOEXEC)
            try:
                rule = PathRule(access, opened)
                if libc.syscall(445, fd, 1, ctypes.byref(rule), 0) != 0:
                    raise RuntimeError(f"Landlock rule failed: errno {ctypes.get_errno()}")
            finally:
                os.close(opened)
        if libc.prctl(38, 1, 0, 0, 0) != 0 or libc.syscall(446, fd, 0) != 0:
            raise RuntimeError(f"Landlock enforcement failed: errno {ctypes.get_errno()}")
    finally:
        os.close(fd)


def main():
    if platform.machine() != "x86_64":
        raise RuntimeError("This checkpoint refuses an unknown architecture.")
    with open(sys.argv[1], encoding="utf-8") as file:
        policy = json.load(file)
    if policy["abi"] != 3 or policy["mismatchAction"] != "kill_process" or policy["matchAction"] != "allow":
        raise RuntimeError("The source policy is not the recorded ABI3 allowlist.")
    if "execve" in policy["syscalls"] or "execveat" in policy["syscalls"]:
        raise RuntimeError("The checkpoint source execution rule changed.")
    if policy["landlockCompatibility"] != "best_effort":
        raise RuntimeError("The checkpoint does not match the recorded library compatibility.")
    ctypes.set_errno(0)
    available_abi = int(libc.syscall(444, None, 0, 1))
    available_errno = ctypes.get_errno() if available_abi < 0 else 0
    if available_abi < 1 and available_errno not in [errno.ENOSYS, errno.EOPNOTSUPP]:
        raise RuntimeError(f"Unrecognized Landlock query failure: ABI {available_abi}, errno {available_errno}")
    effective_abi = min(available_abi, 3) if available_abi >= 1 else 0
    native = seccomp.Arch.X86_64
    filtering = seccomp.SyscallFilter(defaction=seccomp.KILL_PROCESS)
    for name in policy["syscalls"]:
        number = seccomp.resolve_syscall(native, name)
        if not isinstance(number, int) or number < 0:
            raise RuntimeError(f"Unknown syscall in exact source policy: {name}")
        filtering.add_rule_exactly(seccomp.ALLOW, number)
    filtering.set_attr(seccomp.Attr.CTL_NNP, 1)
    source = b"/opt/duskcue/launcher-probe-source.mkv"
    cache = b"/opt/duskcue/launcher-probe-cache"
    outside = b"/opt/duskcue/launcher-probe-outside.txt"
    output = b"/opt/duskcue/launcher-probe-cache/allowed.txt"
    with open(source, "wb") as file:
        file.write(b"owned source path fixture")
    os.makedirs(cache, exist_ok=True)
    with open(outside, "wb") as file:
        file.write(b"owned outside fixture")
    baseline = os.open(outside, os.O_RDONLY)
    os.close(baseline)
    source_baseline = libc.open(source, os.O_RDONLY, 0)
    source_byte = ctypes.create_string_buffer(1)
    source_baseline_read = libc.read(source_baseline, source_byte, 1)
    source_baseline_errno = ctypes.get_errno() if source_baseline < 0 or source_baseline_read != 1 else 0
    if source_baseline >= 0:
        libc.close(source_baseline)
    inherited = libc.prctl(21, 0, 0, 0, 0)
    executable = ctypes.c_char_p(b"/usr/bin/ffmpeg")
    arguments = (ctypes.c_char_p * 3)(b"ffmpeg", b"-version", None)
    environment = (ctypes.c_char_p * 3)(b"PATH=/usr/bin:/bin", b"LANG=C.UTF-8", None)
    markers = {
        name: (ctypes.create_string_buffer(value), len(value))
        for name, value in {
            "landlock": b"LANDLOCK_APPLIED\n",
            "none": b"LANDLOCK_NOT_ENFORCED\n",
            "source": b"SOURCE_ALLOWED\n",
            "cache": b"CACHE_WRITE_ALLOWED\n",
            "outside": b"OUTSIDE_PATH_DENIED\n",
            "outside_allowed": b"OUTSIDE_PATH_ALLOWED\n",
            "nnp": b"NO_NEW_PRIVS_1\n",
            "seccomp": b"APP_SECCOMP_APPLIED\n",
            "mode": b"SECCOMP_MODE_2\n",
            "exec": b"BEFORE_EXEC\n",
            "failed": b"SETUP_OR_PATH_CHECK_FAILED\n",
            "source_open": b"SOURCE_OPEN_FAILED\n",
            "source_read": b"SOURCE_READ_FAILED\n",
            "cache_open": b"CACHE_OPEN_FAILED\n",
            "cache_write": b"CACHE_WRITE_FAILED\n",
            "returned": b"EXEC_RETURNED\n",
        }.items()
    }
    errno_markers = {
        value: (ctypes.create_string_buffer(f"ERRNO_{value}\n".encode("ascii")), len(f"ERRNO_{value}\n"))
        for value in range(134)
    }
    payload = ctypes.create_string_buffer(b"owned cache")
    byte = ctypes.create_string_buffer(1)
    report_read, report_write = os.pipe2(os.O_CLOEXEC)
    stdout_read, stdout_write = os.pipe2(os.O_CLOEXEC)
    child = os.fork()
    if child == 0:
        os.close(report_read)
        os.close(stdout_read)
        os.dup2(stdout_write, 1)
        os.dup2(stdout_write, 2)
        os.close(stdout_write)

        def report(name):
            buffer, length = markers[name]
            libc.write(report_write, buffer, length)

        try:
            landlock(source, cache, effective_abi)
            report("landlock" if effective_abi else "none")
            ctypes.set_errno(0)
            opened = libc.open(source, os.O_RDONLY, 0)
            if opened < 0:
                report("source_open")
                raise RuntimeError("Owned source open was denied.")
            if libc.read(opened, byte, 1) != 1:
                report("source_read")
                raise RuntimeError("Owned source read failed.")
            libc.close(opened)
            report("source")
            opened = libc.open(output, os.O_WRONLY | os.O_CREAT | os.O_TRUNC, 0o600)
            if opened < 0:
                report("cache_open")
                raise RuntimeError("Owned cache open was denied.")
            if libc.write(opened, payload, len(payload.raw) - 1) != len(payload.raw) - 1:
                report("cache_write")
                raise RuntimeError("Owned cache write failed.")
            libc.close(opened)
            report("cache")
            opened = libc.open(outside, os.O_RDONLY, 0)
            if opened >= 0:
                libc.close(opened)
                report("outside_allowed")
            elif ctypes.get_errno() == errno.EACCES:
                report("outside")
            else:
                raise RuntimeError("Outside path failed with an unexpected errno.")
            if libc.prctl(39, 0, 0, 0, 0) != 1:
                raise RuntimeError("No-new-privileges was not active.")
            report("nnp")
            filtering.load()
            report("seccomp")
            if libc.prctl(21, 0, 0, 0, 0) != 2:
                os._exit(72)
            report("mode")
            report("exec")
            libc.execve(executable, arguments, environment)
            report("returned")
            os._exit(73)
        except BaseException:
            value = ctypes.get_errno()
            if value in errno_markers:
                buffer, length = errno_markers[value]
                libc.write(report_write, buffer, length)
            report("failed")
            os._exit(71)
    os.close(report_write)
    os.close(stdout_write)
    signal.alarm(20)
    _, status = os.waitpid(child, 0)
    signal.alarm(0)
    report_bytes = os.read(report_read, 16384)
    stdout = os.read(stdout_read, 65536)
    os.close(report_read)
    os.close(stdout_read)
    observed = report_bytes.decode("ascii").splitlines()
    required = ["SOURCE_ALLOWED", "CACHE_WRITE_ALLOWED", "NO_NEW_PRIVS_1", "APP_SECCOMP_APPLIED", "SECCOMP_MODE_2", "BEFORE_EXEC"]
    seccomp_enforced = all(value in observed for value in required)
    result = {
        "kind": "source-policy-kernel-reproduction",
        "actualServerBinaryExecuted": False,
        "actualProductionLauncherProof": False,
        "kernel": platform.release(),
        "inheritedSeccompMode": inherited,
        "sourceControlOpen": source_baseline >= 0,
        "sourceControlRead": source_baseline_read,
        "sourceControlErrno": source_baseline_errno,
        "sourcePolicySha256": policy["sourceSha256"],
        "syscallCount": len(policy["syscalls"]),
        "landlockRequestedAbi": 3,
        "fixtureFilesystem": "container-local-overlay",
        "availableLandlockAbi": available_abi,
        "availableLandlockErrno": available_errno,
        "landlockEffectiveAbi": effective_abi,
        "landlockEffectiveRights": {0: 0, 1: 8191, 2: 16383, 3: 32767}[effective_abi],
        "landlockCompatibility": "best_effort",
        "filesystemEnforcement": "requested_abi3" if effective_abi == 3 else "partial" if effective_abi else "none",
        "outsidePathDenied": "OUTSIDE_PATH_DENIED" in observed,
        "observedMarkers": observed,
        "seccompEnforced": seccomp_enforced,
        "applicationPolicyEnforced": effective_abi == 3 and seccomp_enforced and "OUTSIDE_PATH_DENIED" in observed,
        "degraded": effective_abi < 3 or not seccomp_enforced,
        "execSucceeded": b"ffmpeg version" in stdout,
        "exitCode": os.WEXITSTATUS(status) if os.WIFEXITED(status) else None,
        "signal": os.WTERMSIG(status) if os.WIFSIGNALED(status) else None,
    }
    result["compatibilityConcernObserved"] = inherited == 2 and seccomp_enforced and not result["execSucceeded"] and result["signal"] == signal.SIGSYS
    with open("/fixtures/launcher-policy-report.json", "w", encoding="utf-8") as file:
        json.dump(result, file, indent=2)
        file.write("\n")
    print(json.dumps(result))
    if not result["compatibilityConcernObserved"]:
        raise RuntimeError("The checkpoint did not establish enforced initial-exec denial.")


main()
