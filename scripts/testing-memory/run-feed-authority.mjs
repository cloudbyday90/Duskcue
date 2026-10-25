// Duskcue — Self-hosted media streaming server
// Copyright (C) 2026-2026 Duskcue Contributors
// Licensed under AGPL-3.0. See LICENSE for details.

import net from 'node:net';
import { spawn } from 'node:child_process';

let hits = 0;
const trap = net.createServer((socket) => { hits += 1; socket.destroy(); });
await new Promise((resolve, reject) => { trap.once('error', reject); trap.listen(0, '127.0.0.1', resolve); });
const proxy = `http://127.0.0.1:${trap.address().port}`;
const env = { ...process.env, HTTP_PROXY: proxy, HTTPS_PROXY: proxy, ALL_PROXY: proxy, http_proxy: proxy, https_proxy: proxy, all_proxy: proxy, NO_PROXY: '', no_proxy: '' };
let code;
try {
    const child = spawn('cargo.exe', ['test', '-p', 'duskcue', '--locked', '--offline', '-j', '2', '--features', 'playback-qualification', '--test', 'live_tv_feed_authority', '--', '--ignored', '--nocapture'], { env, stdio: 'inherit', windowsHide: true });
    code = await new Promise((resolve, reject) => { child.once('error', reject); child.once('exit', (status) => resolve(status ?? 1)); });
} finally {
    await new Promise((resolve) => trap.close(resolve));
}
console.log(`IPTV_FEED_PROXY_RESULT ${JSON.stringify({ connections: hits, passed: hits === 0 })}`);
process.exitCode = code || (hits === 0 ? 0 : 1);
