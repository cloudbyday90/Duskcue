/*
 * Duskcue — Self-hosted media streaming server
 * Copyright (C) 2026-2026 Duskcue Contributors
 * Licensed under AGPL-3.0. See LICENSE for details.
 */

import { valid } from './schema.js';

function bytes(value) {
    valid(typeof value === 'string' && value.length <= 4096 && /^[A-Za-z0-9_-]+={0,2}$/.test(value));
    const binary = atob(value.replaceAll('-', '+').replaceAll('_', '/'));
    return Uint8Array.from(binary, (letter) => letter.charCodeAt(0));
}
function encoded(value) {
    if (value == null) return null;
    valid(value.byteLength <= 49_152);
    return btoa(String.fromCharCode(...new Uint8Array(value))).replaceAll('+', '-').replaceAll('/', '_').replace(/=+$/, '');
}

export function ceremonyOptions(value, registration, credentialClass = globalThis.PublicKeyCredential) {
    const options = value?.publicKey || value;
    valid(options && typeof options === 'object' && typeof options.challenge === 'string');
    const parser = registration ? credentialClass?.parseCreationOptionsFromJSON : credentialClass?.parseRequestOptionsFromJSON;
    if (parser) return parser.call(credentialClass, options);
    const result = { ...options, challenge: bytes(options.challenge) };
    for (const key of ['allowCredentials', 'excludeCredentials']) {
        if (options[key]) {
            valid(Array.isArray(options[key]) && options[key].length <= 64);
            result[key] = options[key].map((entry) => ({ ...entry, id: bytes(entry.id) }));
        }
    }
    if (registration) result.user = { ...options.user, id: bytes(options.user?.id) };
    return result;
}

export function credentialJson(credential, registration) {
    valid(credential?.type === 'public-key' && credential.response);
    if (typeof credential.toJSON === 'function') return credential.toJSON();
    const response = credential.response;
    return { id: credential.id, rawId: encoded(credential.rawId), type: credential.type, authenticatorAttachment: credential.authenticatorAttachment,
        clientExtensionResults: credential.getClientExtensionResults?.() || {},
        response: registration ? { clientDataJSON: encoded(response.clientDataJSON), attestationObject: encoded(response.attestationObject), transports: response.getTransports?.() || [] }
            : { clientDataJSON: encoded(response.clientDataJSON), authenticatorData: encoded(response.authenticatorData), signature: encoded(response.signature), userHandle: encoded(response.userHandle) } };
}

export async function ownerCeremony(options, registration, signal) {
    valid(globalThis.isSecureContext && globalThis.navigator?.credentials && globalThis.PublicKeyCredential);
    const timeout = new AbortController();
    const cancel = () => timeout.abort();
    signal.addEventListener('abort', cancel, { once: true });
    if (signal.aborted) cancel();
    const timer = setTimeout(cancel, 90_000);
    try {
        const publicKey = ceremonyOptions(options, registration);
        const credential = registration ? await navigator.credentials.create({ publicKey, signal: timeout.signal }) : await navigator.credentials.get({ publicKey, signal: timeout.signal });
        return credentialJson(credential, registration);
    } finally { clearTimeout(timer); signal.removeEventListener('abort', cancel); }
}
