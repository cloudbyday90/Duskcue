/*
 * Duskcue — Self-hosted media streaming server
 * Copyright (C) 2026-2026 Duskcue Contributors
 * Licensed under AGPL-3.0. See LICENSE for details.
 */

import { ownerProof, sameSession } from './schema.js';

export function createCredentialReplacement({ begin, current, persist, readContext, publish, failed, announce, ended = () => {} }) {
    let pending = null;
    return {
        get pending() { return pending; },
        run(submit, expected) {
            if (pending) return Promise.reject(new Error('Credential replacement is busy'));
            const lease = begin(expected);
            const work = (async () => {
                try {
                    const proof = ownerProof(await submit(), expected);
                    if (!current(lease)) throw new DOMException('Session changed', 'AbortError');
                    if (expected.transport === 'bearer') await persist(proof.session_token, lease.origin, () => current(lease));
                    if (!current(lease)) throw new DOMException('Session changed', 'AbortError');
                    const actual = await readContext();
                    if (!current(lease)) throw new DOMException('Session changed', 'AbortError');
                    if (!sameSession(actual, proof.context) || actual.credential_revision !== proof.context.credential_revision) throw new Error('Session context changed');
                    publish(actual);
                    return actual;
                } catch (error) {
                    if (current(lease)) await failed(error, lease);
                    throw error;
                } finally { announce(lease.origin); }
            })();
            pending = work;
            const release = () => { if (pending === work) { pending = null; ended(); } };
            work.then(release, release);
            return work;
        },
    };
}
