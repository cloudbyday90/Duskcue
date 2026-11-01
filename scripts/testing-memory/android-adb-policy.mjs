// Duskcue — Self-hosted media streaming server
// Copyright (C) 2026-2026 Duskcue Contributors
// Licensed under AGPL-3.0. See LICENSE for details.

export const androidConsolePorts = Object.freeze(Array.from({ length: 31 }, (_, index) => 5620 + index * 2));

export function androidAdbEnvironment(base, adbPort, consolePort) {
    if (!Number.isInteger(adbPort) || adbPort < 1024 || adbPort > 65535 || !androidConsolePorts.includes(consolePort)
        || [consolePort, consolePort + 1].includes(adbPort)) throw new Error('Invalid isolated Android port authority.');
    return { ...base, ANDROID_ADB_SERVER_PORT: String(adbPort), ADB_SERVER_SOCKET: `tcp:127.0.0.1:${adbPort}`,
        ANDROID_SERIAL: `emulator-${consolePort}`, ADB_LOCAL_TRANSPORT_MAX_PORT: '0' };
}
