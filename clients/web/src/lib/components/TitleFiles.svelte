<script>
    import { m } from '$lib/paraglide/messages.js';
    import { messageLocale } from '$lib/localization/message-locale.js';
    let { files = [], error = '', busy = false, onplay, onretry } = $props();
</script>

<details class="title-files">
    <summary lang={messageLocale('tonight_title_technical')}>{m.tonight_title_technical()}</summary>
    {#if error}
        <p lang={messageLocale('tonight_title_files_failed')} role="alert">{m.tonight_title_files_failed()}</p>
        <button lang={messageLocale('tonight_title_retry')} type="button" onclick={onretry}>{m.tonight_title_retry()}</button>
    {:else if !files.length}<p lang={messageLocale('tonight_title_no_files')}>{m.tonight_title_no_files()}</p>
    {:else}
        <ul>
            {#each files as file (file.id)}
                {@const name = file.file_path?.split(/[\\/]/).pop() || file.container_format || file.id}
                <li>
                    <div class="file-copy"><strong>{name}</strong><p>{[file.video_resolution, file.video_codec, file.video_dynamic_range, file.audio_codec, file.audio_language].filter(Boolean).join(' · ')}</p></div>
                    <span lang={file.is_healthy === true ? messageLocale('tonight_title_available') : messageLocale('tonight_title_unavailable')}>{file.is_healthy === true ? m.tonight_title_available() : m.tonight_title_unavailable()}</span>
                    <button lang={messageLocale('tonight_title_play_file')} type="button" disabled={busy || file.is_healthy !== true} onclick={() => onplay(file.id)}>{m.tonight_title_play_file({ name })}</button>
                </li>
            {/each}
        </ul>
    {/if}
</details>

<style>
    .title-files { margin-top: 1.5rem; border-top: 1px solid var(--color-border-subtle); padding-top: .75rem; }
    summary { cursor: pointer; padding: .6rem 0; min-height: 44px; color: var(--color-text-secondary); }
    ul { list-style: none; padding: 0; }
    li { display: flex; align-items: center; gap: 1rem; flex-wrap: wrap; padding: 1rem 0; border-bottom: 1px solid var(--color-border-subtle); }
    .file-copy { flex: 1; min-width: min(100%, 12rem); overflow-wrap: anywhere; }
    p { color: var(--color-text-muted); font-size: .85rem; }
    button { min-height: 44px; padding: .5rem .8rem; border: 1px solid var(--color-border); border-radius: var(--radius-sm); background: var(--color-bg-elevated); color: var(--color-text-primary); }
</style>
