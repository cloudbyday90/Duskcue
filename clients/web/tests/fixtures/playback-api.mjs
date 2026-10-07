import { stat } from 'node:fs/promises';
import { join } from 'node:path';
import { mediaIds, profileIds } from './catalog.js';
import { createMediaResponder } from './media-response.mjs';
import { createProgressiveHlsResponder } from './progressive-hls.mjs';

const sessionId = (number) => `00000000-0000-7000-8000-${String(700 + number).padStart(12, '0')}`;

export async function installPlaybackApi(page, scenario, media, options = null) {
    options ||= {};
    const clip = options.clip === 'ending' ? 'ending' : 'standard';
    const duration = clip === 'ending' ? media.endingDuration : media.standardDuration;
    const size = (await stat(join(media.directory, `${clip}.mp4`))).size;
    for (const item of scenario.mediaItems) {
        if (!['movie', 'episode'].includes(item.type)) continue;
        item.runtime_seconds = duration;
        for (const file of scenario.files[item.id]) {
            file.runtime_seconds = duration;
            file.file_size = size;
        }
        for (const profile of Object.values(scenario.watchByProfile)) profile[item.id].resume_position_ms = 0;
    }
    if (options.resumePositionMs) scenario.watchByProfile[profileIds.alex][mediaIds.resumeMovie].resume_position_ms = options.resumePositionMs;
    for (const saved of Object.values(scenario.preferencesByProfile)) {
        saved.has_saved_preferences = true;
        saved.viewing_preferences.autoplay_next_episode = options.autoplay !== false;
    }

    const sessions = new Map();
    const state = { sessions, starts: [], stops: [], stopAttempts: [], seeks: [], heartbeats: [], qoe: [], failureResponses: [] };
    let lostStopAcknowledgements = options.lostStopAcknowledgements || 0;
    const serve = createMediaResponder(media);
    const progressive = options.progressiveHls ? await createProgressiveHlsResponder(media, { clip }) : null;
    state.progressive = progressive;

    await page.route(/\/api\/v1(?:\/|$)/, async (route) => {
        const request = route.request();
        const url = new URL(request.url());
        const path = url.pathname.replace(/^\/api\/v1/, '');
        const method = request.method();
        const playbackRoute = /^\/(playback\/(start|heartbeat|stop|seek|qoe|info\/[^/]+)|stream\/[^/]+|transcode\/[^/]+\/(manifest\.m3u8|v0\/(index\.m3u8|segment-\d+\.ts))|items\/[^/]+\/(segments|storyboard(?:\/(index\.vtt|sprite-000\.webp))?))$/.test(path);
        if (!playbackRoute) return route.fallback();
        const body = request.postData() ? request.postDataJSON() : null;
        if (method === 'POST' && path === '/playback/stop') state.stopAttempts.push(body);
        scenario.requests.push({ method, path, body, query: Object.fromEntries(url.searchParams) });
        const json = (value, status = 200) => route.fulfill({ status, contentType: status >= 400 ? 'application/problem+json' : 'application/json', body: JSON.stringify(value) });
        const failure = scenario.failures.get(`${method} ${path}`);
        if (failure) {
            state.failureResponses.push({ method, path, status: failure.status });
            return json({ status: failure.status, title: 'FIXTURE_PLAYBACK', detail: failure.detail }, failure.status);
        }

        if (method === 'POST' && path === '/playback/start') {
            const item = scenario.mediaItems.find((candidate) => candidate.id === body.media_item_id);
            const file = scenario.files[item?.id]?.find((candidate) => candidate.id === body.media_file_id && candidate.is_healthy);
            if (!item || !file) return json({ status: 404, detail: 'Fixture playback media unavailable' }, 404);
            const id = sessionId(sessions.size + 1);
            const decision = options.mode === 'transcode' ? 'transcode' : 'direct_play';
            const transcodeId = decision === 'transcode' ? id : null;
            const session = { id, mediaItemId: item.id, mediaFileId: file.id, profileId: scenario.activeProfileId, decision, transcodeId, positionMs: 0, stopped: false };
            sessions.set(id, session);
            state.starts.push({ ...body, session_id: id });
            if (options.startDelayMs) await new Promise((resolve) => setTimeout(resolve, options.startDelayMs));
            return json({
                session_id: id, stream_decision: decision, stream_url: decision === 'transcode' ? `/api/v1/transcode/${id}/manifest.m3u8` : `/api/v1/stream/${file.id}`,
                media_item_id: item.id, media_file_id: file.id, source_video_codec: 'h264', source_audio_codec: 'aac', target_video_codec: decision === 'transcode' ? 'h264' : null,
                target_audio_codec: decision === 'transcode' ? 'aac' : null, transcode_session_id: transcodeId, selected_audio_stream_index: body.audio_stream_index ?? null,
                selected_subtitle_stream_index: body.subtitle_stream_index ?? null, restart_required: false, playback_mode: 'interactive', ambient_channel_id: null, ambient_channel_updated_at: null,
            });
        }
        if (method === 'POST' && ['/playback/heartbeat', '/playback/stop', '/playback/seek'].includes(path)) {
            const session = sessions.get(body.session_id);
            if (!session) return json({ status: 404, detail: 'Fixture playback session unavailable' }, 404);
            if (path === '/playback/stop' && session.stopResponse) return json(session.stopResponse);
            session.positionMs = Math.max(0, Number(body.position_ms) || 0);
            if (path === '/playback/heartbeat') {
                state.heartbeats.push(body);
                return json({ session_id: session.id, position_ms: session.positionMs, playback_mode: 'interactive' });
            }
            if (path === '/playback/seek') {
                state.seeks.push(body);
                if (options.seekReplacement && session.decision === 'transcode') session.transcodeId = sessionId(1000 + state.seeks.length);
                return json({ session_id: session.id, position_ms: session.positionMs, stream_url: options.seekReplacement ? `/api/v1/transcode/${session.transcodeId}/manifest.m3u8` : null, transcode_session_id: options.seekReplacement ? session.transcodeId : null, playback_mode: 'interactive' });
            }
            state.stops.push(body);
            session.stopped = true;
            const watch = scenario.watchByProfile[session.profileId][session.mediaItemId];
            if (!body.cancelled_before_start) {
                watch.is_watched = watch.is_watched || session.positionMs >= duration * 900;
                watch.resume_position_ms = watch.is_watched ? 0 : Math.min(Math.floor(session.positionMs), duration * 1000);
                watch.last_played_at = new Date().toISOString();
                watch.play_count += 1;
            }
            session.stopResponse = { session_id: session.id, media_item_id: session.mediaItemId, duration_seconds: Math.floor(session.positionMs / 1000), percent_complete: session.positionMs / (duration * 10), is_watched: watch.is_watched, play_count: watch.play_count, playback_mode: 'interactive' };
            if (lostStopAcknowledgements > 0) { lostStopAcknowledgements -= 1; return route.abort('connectionreset'); }
            return json(session.stopResponse);
        }
        if (method === 'POST' && path === '/playback/qoe') {
            state.qoe.push(body);
            return route.fulfill({ status: 204 });
        }
        const info = path.match(/^\/playback\/info\/([^/]+)$/);
        if (method === 'GET' && info) {
            const session = sessions.get(info[1]);
            return session ? json({ session_id: session.id, media_item_id: session.mediaItemId, stream_decision: session.decision, position_ms: session.positionMs, duration_ms: duration * 1000, transcode_progress: null, is_paused: false, started_at: new Date().toISOString() }) : json({ status: 404 }, 404);
        }
        if (method === 'GET' && /^\/stream\/[^/]+$/.test(path)) return serve(route, `${clip}.mp4`, 'video/mp4');
        const hls = path.match(/^\/transcode\/([^/]+)\/(manifest\.m3u8|v0\/(index\.m3u8|segment-\d+\.ts))$/);
        if (method === 'GET' && hls) {
            if (![...sessions.values()].some((session) => session.transcodeId === hls[1])) return json({ status: 404 }, 404);
            const name = hls[2].replace('v0/', '');
            const responder = progressive ? progressive.serve : serve;
            return responder(route, `${clip}/${name}`, name.endsWith('.ts') ? 'video/mp2t' : 'application/vnd.apple.mpegurl');
        }
        if (method === 'GET' && path.endsWith('/segments')) return json({ segments: options.segments || [] });
        if (method === 'GET' && path.endsWith('/storyboard/index.vtt')) return route.fulfill({ contentType: 'text/vtt', body: `WEBVTT\n\n00:00:00.000 --> 00:00:${String(duration).padStart(2, '0')}.000\nsprite-000.webp#xywh=0,0,160,90\n` });
        if (method === 'GET' && path.endsWith('/storyboard/sprite-000.webp')) return serve(route, 'storyboard.webp', 'image/webp');
        if (method === 'GET' && path.endsWith('/storyboard')) {
            const itemId = path.split('/')[2];
            const fileId = url.searchParams.get('media_file_id') || scenario.files[itemId]?.[0]?.id;
            return json({ media_file_id: fileId, interval_seconds: duration, width: 160, height: 90, sprite_count: 1, total_thumbnails: 1,
                index_url: `/api/v1/items/${itemId}/storyboard/index.vtt`, sprites: [{ url: `/api/v1/items/${itemId}/storyboard/sprite-000.webp`, thumbnails: 1, columns: 1, rows: 1 }], generated_at: new Date().toISOString() });
        }
        scenario.unhandledRequests.push(`${method} ${path}`);
        return json({ status: 501, detail: 'Unhandled playback fixture request' }, 501);
    });

    return state;
}
