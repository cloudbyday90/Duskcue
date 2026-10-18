// Duskcue — Self-hosted media streaming server
// Copyright (C) 2026-2026 Duskcue Contributors
// Licensed under AGPL-3.0. See LICENSE for details.

import { spawn } from 'node:child_process';
import { createDirectBridgeSession } from './direct-bridge-session.mjs';

export function openDirectBridge(context, runtime, abortSignal) {
    if (!/^[A-Za-z0-9_.-]+$/.test(context) || !/^[a-f0-9]{64}$/.test(runtime)) throw new Error('Owned bridge identity is invalid.');
    const child = spawn('docker.exe', ['--context', context, 'exec', '-i', runtime, 'node', '/qualification/direct-http-bridge.mjs'], { windowsHide: true, stdio: ['pipe', 'pipe', 'pipe'] });
    return createDirectBridgeSession(child, { abortSignal });
}
