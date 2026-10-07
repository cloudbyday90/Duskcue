/*
 * Duskcue — Self-hosted media streaming server
 * Copyright (C) 2026-2026 Duskcue Contributors
 * Licensed under AGPL-3.0. See LICENSE for details.
 */

export function facetOptions(facets = [], selected = '') {
    const options = Array.isArray(facets) ? [...facets] : [];
    if (selected && !options.some((option) => String(option.value) === String(selected))) options.push({ value: selected, label: selected, count: null });
    return options;
}
