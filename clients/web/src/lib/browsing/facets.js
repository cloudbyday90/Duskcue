export function facetOptions(facets = [], selected = '') {
    const options = Array.isArray(facets) ? [...facets] : [];
    if (selected && !options.some((option) => String(option.value) === String(selected))) options.push({ value: selected, label: selected, count: null });
    return options;
}
