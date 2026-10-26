/*
 * Duskcue — Self-hosted media streaming server
 * Copyright (C) 2026-2026 Duskcue Contributors
 * Licensed under AGPL-3.0. See LICENSE for details.
 */

import { getTextDirection } from '../paraglide/runtime.js';

export function syncDocumentLanguage(document, locale) {
    document.documentElement.lang = locale;
    document.documentElement.dir = getTextDirection(locale);
}
