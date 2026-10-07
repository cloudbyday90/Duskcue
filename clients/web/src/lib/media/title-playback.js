import { isAllowedDesktopRoute, playRoute, titleRoute } from '../navigation/routes.js';
import { assertTitleRequestActive, titleReaders, TitleDataError } from './title-data.js';

export async function prepareTitlePlayback(item, { origin = '', destination = '', fileId = undefined, signal = undefined, readers = titleReaders } = {}) {
    if (!item || !['movie', 'episode'].includes(item.type)) throw new TitleDataError('UNPLAYABLE');
    const [filesResult, watch] = await Promise.all([
        readers.files(item.id, { signal }),
        readers.watch(item.id, { signal }),
    ]);
    assertTitleRequestActive(signal);
    const files = filesResult?.items ?? filesResult ?? [];
    const healthy = Array.isArray(files) ? files.filter((file) => file.is_healthy === true) : [];
    const file = fileId ? healthy.find((candidate) => candidate.id === fileId) : healthy[0];
    if (!file) throw new TitleDataError('FILE_UNAVAILABLE');
    const titleDestination = isAllowedDesktopRoute(destination) && destination.startsWith('/media/')
        ? destination : titleRoute(item, origin);
    const route = new URL(playRoute(item, origin, titleDestination), 'https://duskcue.invalid');
    route.searchParams.set('file', file.id);
    return { route: `${route.pathname}${route.search}`, watch, file };
}
