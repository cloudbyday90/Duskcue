import { getLocale, baseLocale } from '../paraglide/runtime.js';
import { incompleteMessages } from './message-availability.js';

export function messageLocale(key, locale = getLocale()) {
    const available = Object.hasOwn(incompleteMessages, key) ? incompleteMessages[key] : null;
    return available && !available.includes(locale) ? baseLocale : locale;
}
