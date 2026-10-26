<!--
  Duskcue — Self-hosted media streaming server
  Copyright (C) 2026-2026 Duskcue Contributors
  Licensed under AGPL-3.0. See LICENSE for details.
-->
<script>
    import { m } from '$lib/paraglide/messages.js';
    import { preferredText, textLanguage, formatInstant } from '$lib/live-tv/presentation.js';
    import TextEvidence from './TextEvidence.svelte';
    import SuppliedValue from './SuppliedValue.svelte';
    let { programme, locale } = $props();
    let title = $derived(preferredText(programme.titles, locale));
    let subtitle = $derived(preferredText(programme.subtitles, locale));
    let description = $derived(preferredText(programme.descriptions, locale));
</script>

<article class="programme">
    <h3>{#if title?.text}<bdi lang={textLanguage(title.language)}>{title.text}</bdi>{:else}{m.live_tv_untitled()}{/if}</h3>
    {#if subtitle}<p><bdi lang={textLanguage(subtitle.language)}>{subtitle.text}</bdi></p>{/if}
    <dl>
        <div><dt>{m.live_tv_start()}</dt><dd>{#if programme.starts_at}<time datetime={programme.starts_at}>{formatInstant(programme.starts_at, locale)}</time>{:else}{m.live_tv_time_unknown()}{/if}</dd></div>
        <div><dt>{m.live_tv_end()}</dt><dd>{#if programme.stops_at}<time datetime={programme.stops_at}>{formatInstant(programme.stops_at, locale)}</time>{:else}{m.live_tv_time_unknown()}{/if}</dd></div>
    </dl>
    {#if description}<p class="description"><bdi lang={textLanguage(description.language)}>{description.text}</bdi></p>{/if}
    {#if programme.quality.uncertain}<p>{m.live_tv_uncertain()}</p>{/if}
    {#if programme.quality.conflict}<p>{m.live_tv_conflict()}</p>{/if}
    {#if programme.quality.overlap}<p>{m.live_tv_overlap()}</p>{/if}
    {#if programme.quality.clump}<p>{m.live_tv_clump()}</p>{/if}
    <TextEvidence label={m.live_tv_titles()} variants={programme.titles} />
    <TextEvidence label={m.live_tv_subtitles()} variants={programme.subtitles} />
    <TextEvidence label={m.live_tv_descriptions()} variants={programme.descriptions} />
    <TextEvidence label={m.live_tv_categories()} variants={programme.categories} />
    <details>
        <summary>{m.live_tv_raw_evidence()}</summary>
        <dl>
            <div><dt>{m.live_tv_raw_start()}</dt><dd><SuppliedValue value={programme.start_raw} /></dd></div>
            <div><dt>{m.live_tv_raw_end()}</dt><dd><SuppliedValue value={programme.stop_raw} /></dd></div>
            <div><dt>{m.live_tv_exact_start()}</dt><dd><SuppliedValue value={programme.starts_at} /></dd></div>
            <div><dt>{m.live_tv_exact_end()}</dt><dd><SuppliedValue value={programme.stops_at} /></dd></div>
            <div><dt>{m.live_tv_clump_index()}</dt><dd><SuppliedValue value={programme.clump_index_raw} /></dd></div>
        </dl>
    </details>
    <details>
        <summary>{m.live_tv_ratings()}</summary>
        <p>{m.live_tv_ratings_help()}</p>
        {#if programme.ratings.length}<ul>{#each programme.ratings as rating}<li><SuppliedValue value={rating.system} />: <SuppliedValue value={rating.value} /></li>{/each}</ul>
        {:else}<p>{m.live_tv_no_rating()}</p>{/if}
    </details>
</article>

<style>
    .programme { display: grid; gap: 0.7rem; padding: 1.2rem; border: 1px solid var(--color-border); border-radius: var(--radius-md); background: var(--color-bg-surface); min-width: 0; overflow-wrap: anywhere; }
    h3 { font-size: 1.2rem; }
    dl { display: grid; gap: 0.5rem; }
    dt { color: var(--color-text-secondary); }
    dd { margin: 0; }
    summary { cursor: pointer; padding-block: 0.65rem; }
    .description { white-space: pre-wrap; }
    ul { padding-inline-start: 1.3rem; }
</style>
