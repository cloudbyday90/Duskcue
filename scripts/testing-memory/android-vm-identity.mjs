// Duskcue — Self-hosted media streaming server
// Copyright (C) 2026-2026 Duskcue Contributors
// Licensed under AGPL-3.0. See LICENSE for details.

export const androidPackage = 'com.duskcue.mobile.qualification';
export const androidActivity = `${androidPackage}/com.duskcue.mobile.QualificationActivity`;

export function androidPid(text) {
    if (!/^[1-9][0-9]{0,8}$/.test(text)) throw new Error('Android package PID is missing or ambiguous.');
    return Number(text);
}

export function androidCommandIdentity(text) {
    const prefix = `${androidPackage}\0`;
    if (typeof text !== 'string' || text.length > 4096 || !text.startsWith(prefix)
        || !/^\0*$/.test(text.slice(prefix.length))) throw new Error('Android package command identity changed.');
}

export function androidProcessIdentity(text, pid) {
    const match = text.trim().match(/^([1-9][0-9]*) \((.+)\) ([A-Za-z]) (.+)$/);
    if (!match || Number(match[1]) !== pid || !/^[!-~ ]{1,64}$/.test(match[2]) || /[ZXx]/.test(match[3])) throw new Error('Android process identity is not live and exact.');
    const fields = match[4].split(' ');
    if (fields.length < 19 || !/^[1-9][0-9]*$/.test(fields[18])) throw new Error('Android process start identity is invalid.');
    return { pid, started: fields[18] };
}

export function androidFlutterMessage(line, pid) {
    if (/^-+ beginning of (?:main|system|crash)$/.test(line)) return null;
    const match = line.match(/^([VDIWEF])\/flutter\s*\(\s*([1-9][0-9]*)\): ?(.*)$/);
    if (!match || Number(match[2]) !== pid) throw new Error('Android app log is not attributed to its exact PID.');
    return match[3];
}

export function androidVmUri(message, port) {
    if (!message.includes('Dart VM service is listening on')) return null;
    const match = message.match(/^The Dart VM service is listening on (http:\/\/127\.0\.0\.1:([1-9][0-9]{0,4})\/[A-Za-z0-9_=-]{8,44}\/)$/);
    if (!match || Number(match[2]) !== port || port > 65535) throw new Error('Android VM-service URI does not match the owned loopback forward.');
    return match[1];
}

export function androidPausedIsolate(vm, isolate) {
    if (vm?.type !== 'VM' || !Array.isArray(vm.isolates) || vm.isolates.length > 1) throw new Error('Android VM has an ambiguous application isolate.');
    if (!vm.isolates.length) return null;
    const reference = vm.isolates[0];
    if (reference.type !== '@Isolate' || reference.name !== 'main' || !/^isolates\/[0-9]+$/.test(reference.id)) throw new Error('Android main isolate identity is invalid.');
    if (isolate === undefined) return reference.id;
    if (isolate.type !== 'Isolate' || isolate.id !== reference.id || isolate.name !== 'main') throw new Error('Android main isolate identity changed.');
    if (isolate.pauseEvent?.kind === 'None' && typeof isolate.runnable === 'boolean') return null;
    if (isolate.pauseEvent?.kind !== 'PauseStart' || isolate.runnable !== true) throw new Error('Android main isolate is not paused at startup.');
    return reference.id;
}
