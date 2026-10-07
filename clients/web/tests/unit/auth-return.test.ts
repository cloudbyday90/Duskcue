import { describe, expect, it } from 'vitest';
import { authReturnDestination, playbackSignInPath } from '../../src/lib/navigation/auth-return.js';

describe('authentication return destinations', () => {
    it('retains complete internal Title, catalog and administrative destinations', () => {
        for (const path of ['/media/series-1?season=season-2&episode=episode-3&from=%2Fsearch%3Fq%3Dmoon', '/media?type=movie&sort=year&order=desc', '/admin/users']) {
            expect(authReturnDestination(path)).toBe(path);
        }
    });

    it('rejects external, malformed and normalized escape destinations', () => {
        for (const path of [null, '', 'https://other.example/media/one', '//other.example', '/\\other.example', '/media/../auth/login', '/media/one\n']) {
            expect(authReturnDestination(path)).toBe('/dashboard');
        }
    });

    it('uses the actual visible player URL and preserves its full canonical Title through sign-in', () => {
        const title = '/media/series-1?season=season-2&episode=episode-3&from=%2Fmedia%3Ftype%3Dseries';
        const path = playbackSignInPath(new URL(`https://app.example/play/episode-3?${new URLSearchParams({ return_to: title })}`));
        const login = new URL(path, 'https://app.example');
        expect(login.pathname).toBe('/auth/login');
        expect(login.searchParams.get('return_to')).toBe(title);
    });

    it('does not turn a player return parameter into an external or non-Title login redirect', () => {
        for (const destination of ['https://other.example/media/one', '//other.example/media/one', '/play/one', '/admin/users', '/media/../admin']) {
            expect(playbackSignInPath(new URL(`https://app.example/play/one?${new URLSearchParams({ return_to: destination })}`))).toBe('/auth/login');
        }
    });
});
