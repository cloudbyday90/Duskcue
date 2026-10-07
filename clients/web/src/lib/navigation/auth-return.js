/*
 * Duskcue — Self-hosted media streaming server
 * Copyright (C) 2026-2026 Duskcue Contributors
 * Licensed under AGPL-3.0. See LICENSE for details.
 */

import { isAllowedDesktopRoute } from './routes.js';

export function authReturnDestination(candidate) {
    if (typeof candidate !== 'string' || !candidate.startsWith('/') || candidate.startsWith('//')
        || /[\\\u0000-\u0020\u007f]/u.test(candidate)) return '/dashboard';
    const url = new URL(candidate, 'https://duskcue.invalid');
    return url.origin === 'https://duskcue.invalid' && url.pathname === candidate.split(/[?#]/u, 1)[0]
        ? candidate : '/dashboard';
}

export function playbackSignInPath(url) {
    const destination = url.searchParams.get('return_to');
    return destination?.startsWith('/media/') && isAllowedDesktopRoute(destination)
        ? `/auth/login?${new URLSearchParams({ return_to: destination })}` : '/auth/login';
}
