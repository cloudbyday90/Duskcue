import { redirect } from '@sveltejs/kit';

export function load({ url }) {
    if (url.searchParams.get('group') === 'subtitles') {
        throw redirect(308, '/settings/subtitles');
    }
    if (url.searchParams.get('group') === 'downloads') {
        throw redirect(308, '/settings/downloads');
    }
}
