import { describe, expect, it, vi } from 'vitest';
import { createPlaybackAuthorizationRecovery } from '../../src/lib/playback/authorization.js';

vi.mock('../../src/lib/stores/auth.js', () => ({ auth: {} }));

const context = { serverOrigin: 'http://fixture.test', userId: 'user-a' };

describe('scoped close authorization recovery', () => {
    it.each([0, 403, 503])('does not revalidate an unrelated playback error %s', async (status) => {
        const checkSession = vi.fn();
        const recovery = createPlaybackAuthorizationRecovery({ auth: { checkSession }, readContext: () => context });
        expect(await recovery.recover({ status }, context)).toMatchObject({ status: 'superseded' });
        expect(checkSession).not.toHaveBeenCalled();
    });

    it('recognizes intentional same-origin auth clearing as expiry rather than treating its null user as superseded', async () => {
        let current = { ...context };
        const expired = { status: 'expired', isCurrent: () => true };
        const checkSession = vi.fn(async (_options) => { current = { ...current, userId: null }; return expired; });
        const recovery = createPlaybackAuthorizationRecovery({ auth: { checkSession }, readContext: () => current });
        expect(await recovery.recover({ status: 401 }, context)).toBe(expired);
        expect(checkSession).toHaveBeenCalledExactlyOnceWith({ revalidate: true, expectedContext: context });
    });

    it.each(['valid', 'unavailable'])('retains close recovery when validation is %s', async (status) => {
        const outcome = { status, isCurrent: () => true };
        const checkSession = vi.fn(async (_options) => outcome);
        const recovery = createPlaybackAuthorizationRecovery({ auth: { checkSession }, readContext: () => context });
        expect(await recovery.recover({ status: 401 }, context)).toBe(outcome);
    });

    it('does not apply an expired outcome to a newer account or origin', async () => {
        let current = { ...context };
        const checkSession = vi.fn(async (_options) => { current = { serverOrigin: 'http://other.test', userId: 'user-b' }; return { status: 'expired', isCurrent: () => true }; });
        const recovery = createPlaybackAuthorizationRecovery({ auth: { checkSession }, readContext: () => current });
        expect(await recovery.recover({ status: 401 }, context)).toMatchObject({ status: 'superseded' });
    });

    it('rejects an obsolete scoped validation result even when the visible account still has the same identity', async () => {
        const checkSession = vi.fn(async (_options) => ({ status: 'expired', isCurrent: () => false }));
        const recovery = createPlaybackAuthorizationRecovery({ auth: { checkSession }, readContext: () => context });
        expect(await recovery.recover({ status: 401 }, context)).toMatchObject({ status: 'superseded' });
    });

    it('ignores a pending current-scope validation result after the owning player unmounts', async () => {
        let finish!: (value: object) => void;
        const pending = new Promise((resolve) => { finish = resolve; });
        const checkSession = vi.fn(async (_options) => pending);
        const recovery = createPlaybackAuthorizationRecovery({ auth: { checkSession }, readContext: () => context });
        const checking = recovery.recover({ status: 401 }, context);
        recovery.dispose();
        finish({ status: 'valid', isCurrent: () => true });
        expect(await checking).toMatchObject({ status: 'superseded' });
    });
});
