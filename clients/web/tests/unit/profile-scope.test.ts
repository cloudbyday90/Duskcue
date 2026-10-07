import { describe, expect, it, vi } from 'vitest';
import { createProfileScopeSync } from '../../src/lib/profiles/scope.js';

const origin = 'https://first.example';
const payload = (revision = 'one', server = origin, userId = 'account-1') => ({
    type: 'profile-scope-changed', server_origin: server, user_id: userId,
    profile_id: 'profile-2', revision,
});

class Channel extends EventTarget {
    close = vi.fn();
    postMessage = vi.fn();
    callback: any;
    addEventListener(type: string, listener: any) { this.callback = listener; super.addEventListener(type, listener); }
    send(data: unknown) { this.dispatchEvent(new MessageEvent('message', { data })); }
}

function harness(channelAvailable = true) {
    const target = Object.assign(new EventTarget(), { localStorage: { setItem: vi.fn(), removeItem: vi.fn() } });
    let context = { serverOrigin: origin, userId: 'account-1' };
    const channel = new Channel();
    const createChannel = vi.fn(() => channelAvailable ? channel : null);
    const sync = createProfileScopeSync({ readContext: () => context, target: target as any, createChannel: createChannel as any });
    const callback = vi.fn();
    const dispose = sync.start(callback);
    const storage = (value: unknown, key = 'duskcue-profile-scope-event') => target.dispatchEvent(Object.assign(new Event('storage'), { key, newValue: typeof value === 'string' ? value : JSON.stringify(value) }));
    return { sync, callback, dispose, target, channel, createChannel, storage, setContext(value: typeof context) { context = value; } };
}

describe('account and server scoped profile coordination', () => {
    it('delivers a valid same-context event once across BroadcastChannel and storage fallback', () => {
        const h = harness();
        h.channel.send(payload());
        h.storage(payload());
        expect(h.callback).toHaveBeenCalledExactlyOnceWith(payload());
        h.dispose();
    });

    it('rejects the same account ID on another server and a foreign or unknown account', () => {
        const h = harness();
        h.channel.send(payload('foreign-server', 'https://second.example'));
        h.storage(payload('foreign-user', origin, 'account-2'));
        h.setContext({ serverOrigin: origin, userId: null as any });
        h.channel.send(payload('unvalidated'));
        expect(h.callback).not.toHaveBeenCalled();
        h.dispose();
    });

    it('reads the current context after a selected server switch', () => {
        const h = harness();
        h.setContext({ serverOrigin: 'https://second.example', userId: 'account-1' });
        h.channel.send(payload('old'));
        h.channel.send(payload('new', 'https://second.example'));
        expect(h.callback).toHaveBeenCalledExactlyOnceWith(payload('new', 'https://second.example'));
        h.dispose();
    });

    it('publishes only the validated context and retains storage transport if a channel cannot be created', () => {
        const h = harness(false);
        h.sync.publish({ userId: 'account-1', profileId: 'profile-2' });
        const sent = JSON.parse(h.target.localStorage.setItem.mock.calls[0][1]);
        expect(sent).toMatchObject({ type: 'profile-scope-changed', user_id: 'account-1', profile_id: 'profile-2', server_origin: origin });
        expect(typeof sent.revision).toBe('string');
        expect(h.target.localStorage.removeItem).toHaveBeenCalledWith('duskcue-profile-scope-event');
        h.sync.publish({ userId: 'account-2', profileId: 'profile-2' });
        expect(h.target.localStorage.setItem).toHaveBeenCalledOnce();
        h.storage(payload());
        expect(h.callback).toHaveBeenCalledOnce();
        h.dispose();
    });

    it('rejects malformed and unrelated storage events without a profile transition', () => {
        const h = harness();
        h.storage('{');
        h.storage(payload(), 'other-key');
        h.channel.send({ ...payload(), revision: '' });
        h.channel.send({ ...payload(), profile_id: null });
        h.channel.send({ ...payload(), server_origin: 'bad origin' });
        h.channel.send({ ...payload(), server_origin: undefined });
        expect(h.callback).not.toHaveBeenCalled();
        h.dispose();
    });

    it('disposes only its own channel and ignores an old callback after restart', () => {
        const first = harness(); const second = harness();
        const oldCallback = first.channel.callback;
        first.dispose();
        first.storage(payload('disposed'));
        expect(first.callback).not.toHaveBeenCalled();
        expect(first.channel.close).toHaveBeenCalledOnce();
        expect(second.channel.close).not.toHaveBeenCalled();
        const replacement = vi.fn();
        first.sync.start(replacement);
        oldCallback(new MessageEvent('message', { data: payload('late') }));
        expect(replacement).not.toHaveBeenCalled();
        second.channel.send(payload('other-instance'));
        expect(second.callback).toHaveBeenCalledOnce();
        first.sync.dispose(); second.dispose();
    });

    it('bounds event deduplication instead of accumulating the entire session history', () => {
        const h = harness();
        for (let index = 0; index < 65; index += 1) h.channel.send(payload(`event-${index}`));
        h.storage(payload('event-64'));
        expect(h.callback).toHaveBeenCalledTimes(65);
        h.storage(payload('event-0'));
        expect(h.callback).toHaveBeenCalledTimes(66);
        h.dispose();
    });
});
