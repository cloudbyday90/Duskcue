<script>
    import { m } from '$lib/paraglide/messages.js';
    import { messageLocale } from '$lib/localization/message-locale.js';
    import { onDestroy, tick, untrack } from 'svelte';
    import { page } from '$app/stores';
    import { goto } from '$app/navigation';
    import Artwork from '$lib/components/Artwork.svelte';
    import EpisodeGallery from '$lib/components/EpisodeGallery.svelte';
    import TitleWatchActions from '$lib/components/TitleWatchActions.svelte';
    import TitleFiles from '$lib/components/TitleFiles.svelte';
    import { loadTitleData, loadTitleEpisodes, loadTitleWatchFiles } from '$lib/media/title-data.js';
    import { selectTitleSeason, selectTitleEpisode, selectedSeasonRoute, titleOrigin, titleDurationMs } from '$lib/media/title-selection.js';
    import { prepareTitlePlayback } from '$lib/media/title-playback.js';
    import { titleRoute } from '$lib/navigation/routes.js';
    import { formatDuration, formatYear, formatRating, formatTimestamp, formatPercent } from '$lib/utils/format.js';

    let itemId = $derived($page.params.id);
    let data = $state(null);
    let loading = $state(true);
    let loadError = $state('');
    let loadErrorLocale = $state('en');
    let reloadTitle = $state(0);
    let titleDetailsLoading = $state(false);
    let titleDetailsController;
    let episodes = $state([]);
    let episodesLoading = $state(false);
    let episodesError = $state('');
    let episodesErrorLocale = $state('en');
    let reloadEpisodes = $state(0);
    let episodeDetails = $state(null);
    let episodeDetailsLoading = $state(false);
    let reloadEpisodeDetails = $state(0);
    let preparing = $state(false);
    let playbackError = $state('');
    let playbackErrorLocale = $state('en');
    let retryFocus = $state(null);
    let playbackController;
    let playbackTargetId;
    let active = true;

    let item = $derived(data?.item);
    let seasons = $derived(data?.seasons ?? []);
    let origin = $derived(titleOrigin($page.url.searchParams.get('from'), item?.type));
    let requestedSeason = $derived($page.url.searchParams.get('season'));
    let requestedEpisode = $derived($page.url.searchParams.get('episode'));
    let season = $derived(selectTitleSeason(seasons, requestedSeason));
    let seasonId = $derived(season?.id);
    let selectedEpisode = $derived(selectTitleEpisode(episodes, requestedEpisode));
    let selectedEpisodeId = $derived(selectedEpisode?.id);
    let selectedError = $derived(!episodesLoading && ((requestedSeason && !season) || (requestedEpisode && !selectedEpisode)));
    let galleryError = $derived(data?.seasonsError ? m.tonight_title_episodes_failed() : episodesError || (selectedError ? m.tonight_title_invalid_selection() : ''));
    let movieDuration = $derived(titleDurationMs(item, data?.files));
    let movieResume = $derived(data?.watch?.is_watched ? 0 : data?.watch?.resume_position_ms ?? 0);
    let episodeDuration = $derived(titleDurationMs(selectedEpisode, episodeDetails?.files));
    let episodeResume = $derived(episodeDetails?.watch?.is_watched ? 0 : episodeDetails?.watch?.resume_position_ms ?? 0);

    $effect(() => {
        const id = itemId;
        const revision = reloadTitle;
        const controller = new AbortController();
        titleDetailsController?.abort();
        titleDetailsLoading = false;
        loading = true;
        data = null;
        loadError = '';
        playbackError = '';
        untrack(() => {
            loadTitleData(id, { signal: controller.signal }).then(async (result) => {
                if (controller.signal.aborted) return;
                if (['episode', 'season'].includes(result.item.type) && result.item.series_id) {
                    const destination = titleRoute(result.item, titleOrigin($page.url.searchParams.get('from'), 'series'));
                    await goto(destination, { replaceState: true });
                    return;
                }
                data = result;
            }).catch((error) => {
                if (!controller.signal.aborted && error.name !== 'AbortError') {
                    loadError = error.detail || m.tonight_title_load_failed();
                    loadErrorLocale = error.detail ? 'en' : messageLocale('tonight_title_load_failed');
                }
            }).finally(() => { if (!controller.signal.aborted) loading = false; });
        });
        return () => controller.abort();
    });

    $effect(() => {
        const id = seasonId;
        const revision = reloadEpisodes;
        const controller = new AbortController();
        episodes = [];
        episodesError = '';
        if (!id) { episodesLoading = false; return () => controller.abort(); }
        episodesLoading = true;
        untrack(() => {
            loadTitleEpisodes(id, { signal: controller.signal }).then((items) => {
                if (!controller.signal.aborted) episodes = items;
            }).catch((error) => {
                if (!controller.signal.aborted && error.name !== 'AbortError') {
                    episodesError = error.detail || m.tonight_title_episodes_failed();
                    episodesErrorLocale = error.detail ? 'en' : messageLocale('tonight_title_episodes_failed');
                }
            }).finally(() => { if (!controller.signal.aborted) episodesLoading = false; });
        });
        return () => controller.abort();
    });

    $effect(() => {
        const id = selectedEpisodeId;
        const revision = reloadEpisodeDetails;
        const controller = new AbortController();
        episodeDetails = null;
        if (!id) { episodeDetailsLoading = false; return () => controller.abort(); }
        episodeDetailsLoading = true;
        untrack(() => {
            loadTitleWatchFiles(id, { signal: controller.signal }).then((details) => {
                if (!controller.signal.aborted) episodeDetails = details;
            }).finally(() => { if (!controller.signal.aborted) episodeDetailsLoading = false; }).catch(() => {});
        });
        return () => controller.abort();
    });

    $effect(() => {
        const selected = selectedEpisode;
        if (!selected || requestedEpisode || episodesLoading) return;
        untrack(() => { goto(titleRoute(selected, origin), { replaceState: true, keepFocus: true, noScroll: true }); });
    });

    $effect(() => {
        const selectedId = selectedEpisodeId || itemId;
        untrack(() => {
            if (preparing && playbackTargetId !== selectedId) {
                playbackController?.abort();
                preparing = false;
                playbackError = '';
            }
        });
    });

    $effect(() => {
        const target = retryFocus;
        if (!target || loading || titleDetailsLoading || (target === 'episode-gallery-heading' && episodesLoading) || (target === 'selected-episode-heading' && episodeDetailsLoading)) return;
        retryFocus = null;
        tick().then(() => {
            if (!active || document.activeElement !== document.body) return;
            (document.getElementById(target) || document.getElementById('title-heading'))?.focus({ preventScroll: true });
        });
    });

    onDestroy(() => { active = false; playbackController?.abort(); titleDetailsController?.abort(); });

    function selectSeason(id) {
        const selected = seasons.find((candidate) => candidate.id === id);
        if (selected) goto(selectedSeasonRoute(item, selected, origin), { keepFocus: true, noScroll: true });
    }

    function selectEpisode(episode) {
        playbackError = '';
        goto(titleRoute(episode, origin), { keepFocus: true, noScroll: true });
    }

    function retryGallery() {
        retryFocus = 'episode-gallery-heading';
        if (data?.seasonsError) reloadTitle += 1;
        else reloadEpisodes += 1;
    }

    function retryTitle() {
        retryFocus = 'title-heading';
        reloadTitle += 1;
    }

    function retryEpisodeDetails() {
        retryFocus = 'selected-episode-heading';
        reloadEpisodeDetails += 1;
    }

    async function retryTitleDetails() {
        if (!item || titleDetailsLoading) return;
        retryFocus = 'title-heading';
        titleDetailsController?.abort();
        titleDetailsController = new AbortController();
        const controller = titleDetailsController;
        const id = item.id;
        titleDetailsLoading = true;
        try {
            const details = await loadTitleWatchFiles(id, { signal: controller.signal });
            if (active && !controller.signal.aborted && data?.item.id === id) data = { ...data, ...details };
        } catch (error) {
            if (active && !controller.signal.aborted && data?.item.id === id) data = { ...data, watch: null, watchError: error, files: [], filesError: error };
        } finally { if (active && !controller.signal.aborted) titleDetailsLoading = false; }
    }

    function updateEpisodeWatch(watch) {
        if (!selectedEpisode) return;
        const id = selectedEpisode.id;
        episodeDetails = { ...episodeDetails, watch };
        episodes = episodes.map((episode) => episode.id === id ? { ...episode, watch_state: watch } : episode);
    }

    async function play(target, fileId = undefined) {
        if (preparing || !target) return;
        playbackController?.abort();
        playbackController = new AbortController();
        playbackTargetId = target.id;
        const controller = playbackController;
        preparing = true;
        playbackError = '';
        try {
            const prepared = await prepareTitlePlayback(target, { fileId, origin, destination: titleRoute(target, origin), signal: controller.signal });
            if (active && !controller.signal.aborted) await goto(prepared.route);
        } catch (error) {
            if (active && !controller.signal.aborted && error.name !== 'AbortError') {
                playbackError = error.detail || (['FILE_UNAVAILABLE', 'UNPLAYABLE'].includes(error.code) ? m.tonight_title_no_files() : m.tonight_title_playback_failed());
                playbackErrorLocale = error.detail ? 'en' : messageLocale(['FILE_UNAVAILABLE', 'UNPLAYABLE'].includes(error.code) ? 'tonight_title_no_files' : 'tonight_title_playback_failed');
            }
        } finally { if (active && !controller.signal.aborted) preparing = false; }
    }
</script>

<svelte:head><title>{item?.title || 'Duskcue'} · Duskcue</title></svelte:head>

<div class="title-page">
    <a lang={messageLocale('tonight_title_back')} class="back-link" href={origin}>{m.tonight_title_back()}</a>
    <p lang={preparing ? messageLocale('tonight_title_preparing') : undefined} class="play-status" role="status" aria-atomic="true">{preparing ? m.tonight_title_preparing() : ''}</p>
    {#if playbackError}<p lang={playbackErrorLocale} class="error-message" role="alert">{playbackError}</p>{/if}
    {#if loading}
        <p lang={messageLocale('tonight_title_loading')} role="status">{m.tonight_title_loading()}</p>
    {:else if loadError || !item}
        <div class="title-error"><h1 lang={messageLocale('tonight_title_load_failed')} id="title-heading" tabindex="-1">{m.tonight_title_load_failed()}</h1><p lang={loadErrorLocale} role="alert">{loadError}</p><button lang={messageLocale('tonight_title_retry')} type="button" onclick={retryTitle}>{m.tonight_title_retry()}</button></div>
    {:else}
        <section class="title-hero" aria-labelledby="title-heading">
            <div class="backdrop" aria-hidden="true"><Artwork itemId={item.id} type="backdrop" size="w1280" eager /></div>
            <div class="poster"><Artwork itemId={item.id} size="w342" mediaType={item.type} eager /></div>
            <div class="title-copy">
                <h1 id="title-heading" tabindex="-1">{item.title}</h1>
                <p class="title-meta">
                    {#if formatYear(item.premiere_date)}<span>{formatYear(item.premiere_date)}</span>{/if}
                    {#if item.content_rating}<span>{item.content_rating}</span>{/if}
                    {#if movieDuration}<span>{formatDuration(movieDuration / 1000)}</span>{/if}
                    {#if item.rating_average != null}<span>{formatRating(item.rating_average)}/10</span>{/if}
                </p>
                <p lang={item.overview ? undefined : messageLocale('tonight_title_synopsis_unavailable')} class="synopsis">{item.overview || m.tonight_title_synopsis_unavailable()}</p>
                {#if item.type === 'movie' || item.type === 'episode'}
                    <button lang={movieResume > 0 ? messageLocale('tonight_title_resume') : messageLocale('tonight_title_play')} class="play-button" type="button" disabled={preparing || titleDetailsLoading || !data.watch || !data.files.some((file) => file.is_healthy === true)} onclick={() => play(item)}>{movieResume > 0 ? m.tonight_title_resume() : m.tonight_title_play()}</button>
                    {#if data.filesError}<p lang={messageLocale('tonight_title_files_failed')} class="error-message" role="alert">{m.tonight_title_files_failed()}</p><button lang={messageLocale('tonight_title_retry')} type="button" disabled={titleDetailsLoading} onclick={retryTitleDetails}>{m.tonight_title_retry()}</button>{/if}
                    {#if !data.filesError && !data.files.some((file) => file.is_healthy === true)}<p lang={messageLocale('tonight_title_no_files')}>{m.tonight_title_no_files()}</p>{/if}
                    {#if movieResume > 0}<p lang={messageLocale('tonight_title_resume_from')} class="resume-label">{m.tonight_title_resume_from({ position: formatTimestamp(movieResume) })}</p>{/if}
                    {#if movieDuration && movieResume > 0}<progress lang={messageLocale('tonight_title_progress')} max="100" value={formatPercent(movieResume, movieDuration)} aria-label={m.tonight_title_progress({ title: item.title })}></progress>{/if}
                {/if}
                <TitleWatchActions itemId={item.id} itemTitle={item.title} watch={data.watch} loading={titleDetailsLoading} onchange={(watch) => data = { ...data, watch }} onretry={retryTitleDetails} />
            </div>
        </section>
        {#if item.type === 'series'}
            {#if selectedEpisode}
                <section class="selected-episode" aria-labelledby="selected-episode-heading">
                    <p lang={messageLocale('tonight_title_selected_episode')} class="eyebrow">{m.tonight_title_selected_episode()}</p>
                    <h2 lang={selectedEpisode.episode_number != null ? messageLocale('tonight_title_episode_number') : undefined} id="selected-episode-heading" tabindex="-1">{selectedEpisode.episode_number != null ? `${m.tonight_title_episode_number({ number: selectedEpisode.episode_number })} · ` : ''}{selectedEpisode.title}</h2>
                    <p class="title-meta">
                        {#if episodeDuration}<span>{formatDuration(episodeDuration / 1000)}</span>{/if}
                        {#if episodeDetails?.watch}<span lang={episodeDetails.watch.is_watched ? messageLocale('tonight_title_watched') : messageLocale('tonight_title_unwatched')}>{episodeDetails.watch.is_watched ? m.tonight_title_watched() : m.tonight_title_unwatched()}</span>{/if}
                    </p>
                    <p lang={selectedEpisode.overview ? undefined : messageLocale('tonight_title_synopsis_unavailable')} class="synopsis">{selectedEpisode.overview || m.tonight_title_synopsis_unavailable()}</p>
                    <button lang={episodeResume > 0 ? messageLocale('tonight_title_resume') : messageLocale('tonight_title_play')} class="play-button" type="button" disabled={preparing || episodeDetailsLoading || !episodeDetails?.watch || !episodeDetails?.files.some((file) => file.is_healthy === true)} onclick={() => play(selectedEpisode)}>{episodeResume > 0 ? m.tonight_title_resume() : m.tonight_title_play()}</button>
                    {#if episodeDetailsLoading}<p lang={messageLocale('tonight_title_loading')} role="status">{m.tonight_title_loading()}</p>{/if}
                    {#if episodeDetails?.filesError}<p lang={messageLocale('tonight_title_files_failed')} class="error-message" role="alert">{m.tonight_title_files_failed()}</p><button lang={messageLocale('tonight_title_retry')} type="button" onclick={retryEpisodeDetails}>{m.tonight_title_retry()}</button>{/if}
                    {#if episodeDetails && !episodeDetails.filesError && !episodeDetails.files.some((file) => file.is_healthy === true)}<p lang={messageLocale('tonight_title_no_files')}>{m.tonight_title_no_files()}</p>{/if}
                    {#if episodeResume > 0}<p lang={messageLocale('tonight_title_resume_from')} class="resume-label">{m.tonight_title_resume_from({ position: formatTimestamp(episodeResume) })}</p>{/if}
                    {#if episodeDuration && episodeResume > 0}<progress lang={messageLocale('tonight_title_progress')} max="100" value={formatPercent(episodeResume, episodeDuration)} aria-label={m.tonight_title_progress({ title: selectedEpisode.title })}></progress>{/if}
                    {#if !episodeDetailsLoading}
                        {#key selectedEpisode.id}<TitleWatchActions itemId={selectedEpisode.id} itemTitle={selectedEpisode.title} watch={episodeDetails?.watch} onchange={updateEpisodeWatch} onretry={retryEpisodeDetails} />{/key}
                        <TitleFiles files={episodeDetails?.files ?? []} error={episodeDetails?.filesError} busy={preparing || !episodeDetails?.watch} onplay={(fileId) => play(selectedEpisode, fileId)} onretry={retryEpisodeDetails} />
                    {/if}
                </section>
            {/if}
            <EpisodeGallery {seasons} seasonId={seasonId ?? ''} {episodes} selectedId={selectedEpisodeId ?? ''} {origin} loading={episodesLoading} error={galleryError} errorLocale={data?.seasonsError ? messageLocale('tonight_title_episodes_failed') : episodesError ? episodesErrorLocale : messageLocale('tonight_title_invalid_selection')} onseason={selectSeason} onselect={selectEpisode} onretry={retryGallery} />
        {/if}
        {#if item.type !== 'series' || data.files.length || data.filesError}
            <TitleFiles files={data.files} error={data.filesError} busy={preparing || titleDetailsLoading || !data.watch} onplay={(fileId) => play(item, fileId)} onretry={retryTitleDetails} />
        {/if}
    {/if}
</div>

<style>
    .title-page { max-width: 1300px; margin: 0 auto; padding-bottom: 3rem; }
    .back-link { display: inline-flex; align-items: center; min-height: 44px; color: var(--color-text-secondary); }
    .play-status { min-height: 1.3rem; color: var(--color-text-secondary); }
    .title-hero { display: flex; gap: clamp(1.25rem, 3vw, 3rem); position: relative; overflow: hidden; padding: 3rem 2rem; border-radius: var(--radius-lg); background: var(--color-bg-surface); }
    .backdrop { position: absolute; inset: 0; opacity: .15; pointer-events: none; }
    .poster { position: relative; width: 190px; aspect-ratio: 2 / 3; flex-shrink: 0; align-self: start; border-radius: var(--radius-md); overflow: hidden; box-shadow: var(--shadow-elevated); }
    .title-copy { position: relative; min-width: 0; flex: 1; }
    h1 { font-size: clamp(2rem, 4vw, 3.3rem); line-height: 1.1; overflow-wrap: anywhere; }
    .title-meta { display: flex; gap: 1rem; flex-wrap: wrap; margin-top: .8rem; color: var(--color-text-secondary); }
    .synopsis { margin: 1.25rem 0; max-width: 75ch; line-height: 1.65; color: var(--color-text-secondary); }
    button { min-height: 44px; padding: .65rem 1.2rem; border-radius: var(--radius-sm); border: 1px solid var(--color-border); color: var(--color-text-primary); background: var(--color-bg-elevated); }
    .play-button { background: var(--color-accent); color: var(--color-on-accent); border-color: var(--color-accent); min-width: 120px; font-weight: 600; }
    .play-button:disabled { background: var(--color-bg-elevated); color: var(--color-text-muted); border-color: var(--color-border); }
    progress { display: block; width: min(100%, 300px); height: .4rem; accent-color: var(--color-accent); margin-top: .5rem; }
    .resume-label { color: var(--color-text-secondary); margin-top: .5rem; }
    .selected-episode { margin-top: 2.5rem; padding: 1.5rem; border: 1px solid var(--color-border); border-radius: var(--radius-lg); background: var(--color-bg-surface); }
    .selected-episode h2 { margin-top: .5rem; font-size: 1.8rem; }
    .eyebrow { color: var(--color-accent); font-size: .85rem; }
    .error-message { color: var(--color-error); margin: .75rem 0; }
    .title-error { padding: 2rem 0; }
    .title-error p { margin: 1rem 0; }
    @media (max-width: 650px) {
        .title-hero { padding: 1.5rem; flex-direction: column; }
        .poster { width: 140px; }
        .selected-episode { padding: 1rem; }
    }
</style>
