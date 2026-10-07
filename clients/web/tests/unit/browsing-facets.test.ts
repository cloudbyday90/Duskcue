import { describe, expect, it } from 'vitest';
import { facetOptions } from '../../src/lib/browsing/facets.js';

describe('search facet selections', () => {
    it('preserves actual labels/counts and retains a submitted filter missing from new facets', () => {
        const facets = [{ value: 'drama', label: 'Drama', count: 12 }];
        expect(facetOptions(facets, 'family')).toEqual([...facets, { value: 'family', label: 'family', count: null }]);
        expect(facets).toHaveLength(1);
    });

    it('does not duplicate a selected facet or invent options when the endpoint is empty', () => {
        expect(facetOptions([{ value: '2026', label: '2026', count: 4 }], '2026')).toHaveLength(1);
        expect(facetOptions(undefined, '')).toEqual([]);
    });
});
