/*
 * Duskcue — Self-hosted media streaming server
 * Copyright (C) 2026-2026 Duskcue Contributors
 * Licensed under AGPL-3.0. See LICENSE for details.
 */
import { describe, expect, it, vi } from 'vitest';
import { createCredentialReplacement } from '../../src/lib/household/credential-replacement.js';
import { sessionContext, ownerProof, ownerStatus } from '../../src/lib/household/schema.js';
import { ceremonyOptions, credentialJson } from '../../src/lib/household/ceremony.js';
import { createApiScenario } from '../fixtures/api.js';
import { fixtureSessionContext } from '../fixtures/household.js';

function deferred() { let resolve: (value?: any) => void; let reject: (error: any) => void; const promise = new Promise<any>((a, b) => { resolve = a; reject = b; }); return { promise, resolve, reject }; }
function setup(transport = 'bearer') {
    const scenario = createApiScenario();
    const before = fixtureSessionContext(scenario, transport);
    scenario.household.revision = '1';
    const after = fixtureSessionContext(scenario, transport);
    let current = true;
    const proof = { context: after, expires_at: new Date(Date.now() + 300_000).toISOString(), ...(transport === 'bearer' ? { session_token: 'x'.repeat(48) } : {}) };
    const api = { begin: vi.fn(() => ({ origin: 'https://captured.invalid' })), current: vi.fn(() => current), persist: vi.fn(async () => {}), readContext: vi.fn(async () => after), publish: vi.fn(), failed: vi.fn(async () => {}), announce: vi.fn(), ended: vi.fn() };
    const replacement = createCredentialReplacement(api);
    return { scenario, before, after, proof, api, replacement, supersede: () => { current = false; } };
}

describe('retained credential replacement', () => {
    it('persists the captured-origin bearer before checking and publishing the actual context', async () => {
        const { replacement, before, after, proof, api } = setup();
        await expect(replacement.run(async () => proof, before)).resolves.toEqual(after);
        expect(api.persist).toHaveBeenCalledWith(proof.session_token, 'https://captured.invalid', expect.any(Function));
        expect(api.publish).toHaveBeenCalledWith(after);
        expect(api.persist.mock.invocationCallOrder[0]).toBeLessThan(api.readContext.mock.invocationCallOrder[0]);
        expect(replacement.pending).toBeNull();
    });
    it('uses cookie reconciliation without persisting or exposing a body token', async () => {
        const { replacement, before, proof, api } = setup('cookie');
        await replacement.run(async () => proof, before);
        expect(api.persist).not.toHaveBeenCalled();
        expect(api.publish).toHaveBeenCalledOnce();
    });
    it('holds admission through a pending native write and rejects another proof instead of queueing it', async () => {
        const { replacement, before, proof, api } = setup(); const writing = deferred();
        api.persist.mockImplementationOnce(() => writing.promise);
        const first = replacement.run(async () => proof, before);
        await vi.waitFor(() => expect(api.persist).toHaveBeenCalledOnce());
        await expect(replacement.run(async () => proof, before)).rejects.toThrow('busy');
        expect(replacement.pending).toBe(first);
        expect(api.publish).not.toHaveBeenCalled();
        writing.resolve(); await first;
        expect(replacement.pending).toBeNull();
    });
    it('cannot publish a late native completion into a newer login or selected server', async () => {
        const { replacement, before, proof, api, supersede } = setup(); const writing = deferred();
        api.persist.mockImplementationOnce(() => writing.promise);
        const pending = replacement.run(async () => proof, before);
        const rejected = expect(pending).rejects.toMatchObject({ name: 'AbortError' });
        await vi.waitFor(() => expect(api.persist).toHaveBeenCalledOnce());
        supersede(); writing.resolve(); await rejected;
        expect(api.publish).not.toHaveBeenCalled(); expect(api.failed).not.toHaveBeenCalled();
        expect(api.announce).toHaveBeenCalledWith('https://captured.invalid');
    });
    it('does not adopt a stale HTTP success into a newer session', async () => {
        const { replacement, before, proof, api, supersede } = setup(); const network = deferred();
        const pending = replacement.run(() => network.promise, before); const rejected = expect(pending).rejects.toMatchObject({ name: 'AbortError' });
        supersede(); network.resolve(proof); await rejected;
        expect(api.persist).not.toHaveBeenCalled(); expect(api.failed).not.toHaveBeenCalled();
    });
    it.each(['storage', 'context'])('keeps %s failure explicit and never repeats the mutation', async (kind) => {
        const { replacement, before, proof, api } = setup(); const submit = vi.fn(async () => proof);
        if (kind === 'storage') api.persist.mockRejectedValueOnce(new Error('private OS diagnostic'));
        else api.readContext.mockRejectedValueOnce(new Error('response unavailable'));
        await expect(replacement.run(submit, before)).rejects.toThrow();
        expect(submit).toHaveBeenCalledOnce(); expect(api.failed).toHaveBeenCalledOnce(); expect(api.publish).not.toHaveBeenCalled();
    });
    it('never restores the proof profile after the current session moved to another profile', async () => {
        const { replacement, before, after, proof, api } = setup();
        const changed = { ...after, profile_id: '019a4567-1234-7000-8000-000000000777', profile_context_revision: '3' };
        api.readContext.mockResolvedValueOnce(changed);
        await replacement.run(async () => proof, before);
        expect(api.publish).toHaveBeenCalledWith(changed);
    });
    it('rejects a context from another session after rotation', async () => {
        const { replacement, before, after, proof, api } = setup();
        api.readContext.mockResolvedValueOnce({ ...after, session_id: '019a4567-1234-7000-8000-000000000888' });
        await expect(replacement.run(async () => proof, before)).rejects.toThrow('context');
        expect(api.publish).not.toHaveBeenCalled();
    });
});

describe('owner metadata and ceremony boundaries', () => {
    it('preserves opaque decimal revisions and excludes extra response fields', () => {
        const { before } = setup();
        const decoded = sessionContext({ ...before, credential_revision: '9007199254740993', secret: 'not projected' });
        expect(decoded.credential_revision).toBe('9007199254740993'); expect(decoded).not.toHaveProperty('secret');
        expect(() => sessionContext({ ...before, credential_revision: 1 })).toThrow();
        expect(() => sessionContext({ ...before, context_id: 'malformed' })).toThrow();
    });
    it('rejects a non-rotation, wrong account or unexpected cookie body token', () => {
        const { before, proof } = setup('cookie');
        expect(() => ownerProof({ ...proof, context: before }, before)).toThrow();
        expect(() => ownerProof({ ...proof, context: { ...proof.context, user_id: proof.context.session_id } }, before)).toThrow();
        expect(() => ownerProof({ ...proof, session_token: 'leaked' }, before)).toThrow();
    });
    it('rejects grant metadata combined with an unrelated scope', () => {
        const { before, after } = setup();
        const status = { context: after, expires_at: null, protection_enabled: true, password_available: true, trusted_passkeys: 1 };
        expect(() => ownerStatus(status, before)).toThrow();
        expect(ownerStatus(status, after).trusted_passkeys).toBe(1);
    });
    it('converts binary JSON options without modifying the server data or RP', () => {
        const source = { publicKey: { challenge: 'AQID', rpId: 'localhost', allowCredentials: [{ id: 'BAUG', type: 'public-key' }] } };
        const parsed = ceremonyOptions(source, false, {} as typeof PublicKeyCredential);
        expect([...parsed.challenge]).toEqual([1, 2, 3]); expect([...parsed.allowCredentials[0].id]).toEqual([4, 5, 6]);
        expect(source.publicKey.challenge).toBe('AQID'); expect(parsed.rpId).toBe('localhost');
        const created = ceremonyOptions({ challenge: 'AQID', user: { id: 'BAUG' } }, true, {} as typeof PublicKeyCredential);
        expect([...created.user.id]).toEqual([4, 5, 6]);
    });
    it('serializes legacy browser credentials using standard WebAuthn names', () => {
        const credential = { id: 'AQID', rawId: new Uint8Array([1, 2, 3]).buffer, type: 'public-key', getClientExtensionResults: () => ({}), response: { clientDataJSON: new Uint8Array([4]).buffer, authenticatorData: new Uint8Array([5]).buffer, signature: new Uint8Array([6]).buffer, userHandle: null } };
        expect(credentialJson(credential, false)).toMatchObject({ rawId: 'AQID', clientExtensionResults: {}, response: { signature: 'Bg', userHandle: null } });
    });
});
