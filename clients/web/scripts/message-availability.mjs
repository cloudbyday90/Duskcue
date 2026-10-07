import { readFile, writeFile } from 'node:fs/promises';
import { fileURLToPath } from 'node:url';

const project = new URL('../project.inlang/settings.json', import.meta.url);
const legalNotice = '/*\n * Duskcue — Self-hosted media streaming server\n * Copyright (C) 2026-2026 Duskcue Contributors\n * Licensed under AGPL-3.0. See LICENSE for details.\n */\n\n';

export function buildMessageAvailability(catalogs, locales, baseLocale) {
    return Object.fromEntries(Object.keys(catalogs[baseLocale]).filter((key) => !key.startsWith('$') && !key.startsWith('__')).sort().flatMap((key) => {
        const available = locales.filter((locale) => Object.hasOwn(catalogs[locale], key));
        return available.length === locales.length ? [] : [[key, available]];
    }));
}

export async function readMessageAvailability() {
    const { locales, baseLocale } = JSON.parse(await readFile(project, 'utf8'));
    const catalogs = Object.fromEntries(await Promise.all(locales.map(async (locale) => [locale, JSON.parse(await readFile(new URL(`../messages/${locale}.json`, import.meta.url), 'utf8'))])));
    return { catalogs, locales, baseLocale, incompleteMessages: buildMessageAvailability(catalogs, locales, baseLocale) };
}

if (process.argv[1] === fileURLToPath(import.meta.url)) {
    const { incompleteMessages } = await readMessageAvailability();
    await writeFile(new URL('../src/lib/localization/message-availability.js', import.meta.url), `${legalNotice}export const incompleteMessages = ${JSON.stringify(incompleteMessages, null, 4)};\n`);
}
