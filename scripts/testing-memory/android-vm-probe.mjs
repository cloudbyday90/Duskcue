// Duskcue — Self-hosted media streaming server
// Copyright (C) 2026-2026 Duskcue Contributors
// Licensed under AGPL-3.0. See LICENSE for details.

import { request } from 'node:http';
import { androidVmUri } from './android-vm-identity.mjs';

export async function androidVmRead(uri, method, parameters, budget) {
    const port = Number(new URL(uri).port);
    if (androidVmUri(`The Dart VM service is listening on ${uri}`, port) !== uri
        || !['getVM', 'getIsolate'].includes(method)) throw new Error('Unapproved Android VM readiness request.');
    const url = new URL(`${uri}${method}`);
    if (method === 'getVM' && Object.keys(parameters).length || method === 'getIsolate' && (Object.keys(parameters).join() !== 'isolateId'
        || !/^isolates\/[0-9]+$/.test(parameters.isolateId))) throw new Error('Unapproved Android VM readiness parameters.');
    url.search = new URLSearchParams(parameters).toString();
    let size = 0, failure, response;
    const chunks = [];
    const req = request(url, { method: 'GET', agent: false, headers: { 'Accept-Encoding': 'identity', Accept: 'application/json' } });
    const cancel = () => req.destroy(new Error('Android VM readiness was cancelled.'));
    const timer = setTimeout(() => req.destroy(new Error('Android VM readiness deadline expired.')), Math.min(5000, budget.remaining(true)));
    budget.signal.addEventListener('abort', cancel, { once: true });
    if (budget.signal.aborted) cancel();
    try {
        await new Promise((resolve) => {
            req.on('error', (error) => { failure ||= error; });
            req.once('close', resolve);
            req.once('response', (value) => {
                response = value;
                if (value.statusCode !== 200 || value.headers['content-encoding'] && value.headers['content-encoding'] !== 'identity'
                    || !/^application\/json(?:;\s*charset=utf-8)?$/i.test(value.headers['content-type'] || '')) {
                    failure ||= new Error('Android VM readiness refused status, coding or type.');
                    req.destroy(failure);
                    return;
                }
                value.on('error', (error) => { failure ||= error; });
                value.on('data', (chunk) => {
                    try {
                        budget.consume(chunk.length); size += chunk.length;
                        if (size > 1048576) throw new Error('Android VM readiness body exceeded its bound.');
                        chunks.push(chunk);
                    } catch (error) { failure ||= error; req.destroy(error); }
                });
            });
            req.end();
        });
        if (response && !response.closed) await new Promise((resolve) => response.once('close', resolve));
        if (failure) throw failure;
        if (!response?.complete) throw new Error('Android VM readiness response is incomplete.');
        const data = JSON.parse(new TextDecoder('utf-8', { fatal: true }).decode(Buffer.concat(chunks)));
        if (data.error || !data.result || data.jsonrpc !== '2.0') throw new Error('Android VM readiness did not return a JSON-RPC result.');
        return data.result;
    } finally {
        clearTimeout(timer);
        budget.signal.removeEventListener('abort', cancel);
        req.destroy();
    }
}
