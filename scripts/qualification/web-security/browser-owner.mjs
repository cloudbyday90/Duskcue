// Duskcue — Self-hosted media streaming server
// Copyright (C) 2026-2026 Duskcue Contributors
// Licensed under AGPL-3.0. See LICENSE for details.

import { observe } from './lifecycle.mjs';
import { fixtureEnvironment } from './process.mjs';

function processClosed(child) {
    if (!child || !Number.isInteger(child.pid) || child.pid <= 0) throw new Error('Missing owned browser process.');
    return new Promise((resolve) => {
        const closed = () => { child.removeListener('close', closed); resolve(); };
        child.once('close', closed);
        if ((child.exitCode !== null || child.signalCode !== null) && child.stdio?.every((stream) => !stream || stream.closed === true)) closed();
    });
}

function watched(promise, signal, timeoutMs) {
    let abort;
    const cancelled = new Promise((_, reject) => { abort = () => reject(signal.reason || new Error('Browser cancelled.')); signal.addEventListener('abort', abort, { once: true }); });
    if (signal.aborted) abort();
    return observe(Promise.race([promise, cancelled]), timeoutMs, 'Browser operation remains unconfirmed.')
        .finally(() => signal.removeEventListener('abort', abort));
}

export class BrowserOwner {
    #sdk;
    #signal;
    #output;
    #fail;
    #timeout;
    #executable;
    #launch;
    #connect;
    #context;
    #server;
    #browser;
    #actualContext;
    #childClosed;
    #disposal;
    #closing = false;
    #confirmed = false;
    #failure;
    #cancel;
    constructor(sdk, { signal, output = () => {}, fail = () => {}, timeoutMs = 10000, executablePath }) {
        this.#sdk = sdk; this.#signal = signal; this.#output = output; this.#fail = fail; this.#timeout = timeoutMs;
        this.#executable = executablePath;
        this.#cancel = () => { this.close().catch(() => {}); };
        signal.addEventListener('abort', this.#cancel, { once: true });
    }
    get cleanupConfirmed() { return this.#confirmed; }
    check() { this.#signal.throwIfAborted(); if (this.#failure) throw this.#failure; }
    async open() {
        this.check();
        if (this.#launch || this.#closing) throw new Error('Browser admission is closed or occupied.');
        this.#launch = Promise.resolve().then(() => this.#sdk.launchServer({ headless: true, host: '127.0.0.1', timeout: 10000,
            executablePath: this.#executable, env: fixtureEnvironment() })).then((server) => {
            this.#server = server;
            const child = server.process();
            this.#childClosed = processClosed(child);
            for (const stream of [child.stdout, child.stderr]) stream?.on('data', (chunk) => {
                if (this.#failure) return;
                try { this.#output(chunk); }
                catch (error) { this.#failure = error; this.#fail(error); this.close().catch(() => {}); }
            });
            return server;
        });
        this.#launch.catch(() => {});
        const server = await watched(this.#launch, this.#signal, this.#timeout);
        this.check();
        const endpoint = new URL(server.wsEndpoint());
        if (endpoint.protocol !== 'ws:' || endpoint.hostname !== '127.0.0.1' || !endpoint.port) throw new Error('Invalid owned browser endpoint.');
        this.#connect = Promise.resolve().then(() => this.#sdk.connect(endpoint.href, { timeout: 10000 })).then((browser) => (this.#browser = browser));
        this.#connect.catch(() => {});
        const browser = await watched(this.#connect, this.#signal, this.#timeout);
        this.check();
        this.#context = browser.newContext({ acceptDownloads: false, serviceWorkers: 'block', locale: 'en-US' }).then((context) => (this.#actualContext = context));
        this.#context.catch(() => {});
        const context = await watched(this.#context, this.#signal, this.#timeout);
        this.check();
        return context;
    }
    close() {
        this.#closing = true;
        this.#disposal ||= this.#closeActual();
        return observe(this.#disposal, this.#timeout, 'Browser cleanup is unconfirmed; fixture retained.');
    }
    async #closeActual() {
        const failures = [];
        try {
            if (this.#launch) {
                try { await this.#launch; } catch (error) { failures.push(error); }
                const graceful = (async () => {
                    await this.#connect?.catch(() => {});
                    await this.#context?.catch(() => {});
                    if (this.#actualContext) await this.#actualContext.close();
                    if (this.#browser) await this.#browser.close();
                    if (this.#server) await this.#server.close();
                })();
                try { await observe(graceful, this.#timeout, 'Browser graceful cleanup timed out.'); }
                catch (error) {
                    failures.push(error);
                    if (this.#server) try { await this.#server.kill(); } catch (killError) { failures.push(killError); }
                }
                await this.#childClosed;
                try { await graceful; } catch (error) { if (!failures.includes(error)) failures.push(error); }
            }
            if (this.#failure) failures.push(this.#failure);
            if (failures.length) throw new AggregateError(failures, 'Browser setup/cleanup did not settle successfully.');
            this.#confirmed = true;
        } finally { this.#signal.removeEventListener('abort', this.#cancel); }
    }
}
