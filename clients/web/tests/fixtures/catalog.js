const uuid = (number) => `00000000-0000-7000-8000-${String(number).padStart(12, '0')}`;
const timestamp = '2026-10-03T12:00:00Z';

export const profileIds = { alex: uuid(1), morgan: uuid(2), kids: uuid(3) };
export const mediaIds = {
    resumeMovie: uuid(101),
    completedMovie: uuid(102),
    kidsMovie: uuid(103),
    unavailableMovie: uuid(104),
    series: uuid(200),
    seasonOne: uuid(201),
    seasonTwo: uuid(202),
    episodeOne: uuid(211),
    episodeTwo: uuid(212),
    episodeThree: uuid(221),
    episodeFour: uuid(222),
};
export const libraryId = uuid(500);
export const tvLibraryId = uuid(501);
export const collectionId = uuid(600);

export function createCatalog() {
    const profiles = Object.entries(profileIds).map(([key, id]) => ({
        id,
        name: key === 'kids' ? 'Kids' : key[0].toUpperCase() + key.slice(1),
        avatar: null,
        profile_type: key === 'kids' ? 'kids' : 'standard',
        is_default: key === 'alex',
        max_content_rating: key === 'kids' ? 'PG' : 'unrestricted',
        library_ids: [libraryId, tvLibraryId],
        allow_search: true,
        allow_downloads: key !== 'kids',
        allow_external_links: key !== 'kids',
        allow_ambient_channels: true,
        parent_pin_configured: key === 'kids',
        created_at: timestamp,
        updated_at: timestamp,
    }));
    const user = {
        id: uuid(10),
        username: 'fixture-account',
        display_name: 'Fixture account',
        role: 'user',
        capabilities: [],
        active_profile_id: profileIds.alex,
        profile_selection_required: false,
    };
    const definitions = [
        { id: mediaIds.resumeMovie, title: 'The Lighthouse Archive', type: 'movie', content_rating: 'PG', runtime_seconds: 5400 },
        { id: mediaIds.completedMovie, title: 'Completed Journey', type: 'movie', content_rating: 'R', runtime_seconds: 6000 },
        { id: mediaIds.kidsMovie, title: 'Cloud Parade', type: 'movie', content_rating: 'G', runtime_seconds: 3600 },
        { id: mediaIds.unavailableMovie, title: 'Unavailable Feature', type: 'movie', file_count: 0 },
        { id: mediaIds.series, title: 'Harbor Stories', type: 'series', content_rating: 'PG' },
        { id: mediaIds.seasonOne, title: 'Harbor Stories — Season 1', type: 'season', series_id: mediaIds.series, season_number: 1 },
        { id: mediaIds.seasonTwo, title: 'Harbor Stories — Season 2', type: 'season', series_id: mediaIds.series, season_number: 2 },
        { id: mediaIds.episodeOne, title: 'A New Arrival', type: 'episode', series_id: mediaIds.series, season_id: mediaIds.seasonOne, season_number: 1, episode_number: 1, runtime_seconds: 1800 },
        { id: mediaIds.episodeTwo, title: 'The Last Ferry', type: 'episode', series_id: mediaIds.series, season_id: mediaIds.seasonOne, season_number: 1, episode_number: 2, runtime_seconds: 1800 },
        { id: mediaIds.episodeThree, title: 'Open Water', type: 'episode', series_id: mediaIds.series, season_id: mediaIds.seasonTwo, season_number: 2, episode_number: 1, runtime_seconds: 1800 },
        { id: mediaIds.episodeFour, title: 'Home Again', type: 'episode', series_id: mediaIds.series, season_id: mediaIds.seasonTwo, season_number: 2, episode_number: 2, runtime_seconds: 1800 },
    ];
    const mediaItems = definitions.map((definition) => ({
        library_id: ['series', 'season', 'episode'].includes(definition.type) ? tvLibraryId : libraryId,
        created_at: timestamp,
        updated_at: timestamp,
        sort_title: definition.title,
        original_title: null,
        overview: `Synopsis for ${definition.title}.`,
        premiere_date: '2026-01-01',
        end_date: null,
        content_rating: 'PG',
        runtime_seconds: null,
        tmdb_id: null,
        imdb_id: null,
        tvdb_id: null,
        trakt_id: null,
        rating_average: 7.5,
        rating_vote_count: 20,
        metadata: {},
        match_state: 'matched',
        identification_source: 'manual',
        file_count: 1,
        ...definition,
    }));
    const files = Object.fromEntries(mediaItems.map((item) => [item.id, item.file_count === 0 || ['series', 'season'].includes(item.type) ? [] : [{
        id: uuid(Number(item.id.slice(-3)) + 1000),
        media_item_id: item.id,
        file_path: `${item.title.replaceAll(' ', '_')}.mp4`,
        file_size: 1_000_000,
        file_hash: null,
        file_modified_at: timestamp,
        container_format: 'mp4',
        video_codec: 'h264',
        video_resolution: '1920x1080',
        video_bitrate: 4_000_000,
        video_dynamic_range: 'sdr',
        video_frame_rate: 24,
        audio_codec: 'aac',
        audio_channels: 2,
        audio_language: 'eng',
        audio_bitrate: 128_000,
        runtime_seconds: item.runtime_seconds || 1800,
        last_scanned_at: timestamp,
        is_healthy: true,
        additional_streams: { audio: [], subtitles: [] },
        created_at: timestamp,
        updated_at: timestamp,
    }]]));
    const watchByProfile = Object.fromEntries(profiles.map((profile) => [profile.id, Object.fromEntries(mediaItems.map((item) => [item.id, {
        id: uuid(Number(item.id.slice(-3)) + 2000),
        media_item_id: item.id,
        is_watched: false,
        play_count: 0,
        last_played_at: null,
        resume_position_ms: 0,
        is_favorite: false,
        user_rating: null,
    }]))]));
    Object.assign(watchByProfile[profileIds.alex][mediaIds.resumeMovie], { resume_position_ms: 600_000, last_played_at: timestamp });
    Object.assign(watchByProfile[profileIds.alex][mediaIds.completedMovie], { is_watched: true, play_count: 1, last_played_at: timestamp });
    Object.assign(watchByProfile[profileIds.alex][mediaIds.episodeOne], { resume_position_ms: 300_000, last_played_at: timestamp });
    Object.assign(watchByProfile[profileIds.morgan][mediaIds.resumeMovie], { resume_position_ms: 900_000, last_played_at: timestamp });
    Object.assign(watchByProfile[profileIds.kids][mediaIds.kidsMovie], { resume_position_ms: 120_000, last_played_at: timestamp });

    return {
        user,
        profiles,
        mediaItems,
        files,
        watchByProfile,
        libraries: [
            { id: libraryId, name: 'Fixture movies', type: 'movies', item_count: 4 },
            { id: tvLibraryId, name: 'Fixture TV', type: 'tvshows', item_count: 7 },
        ],
        collections: [{ id: collectionId, name: 'Weekend stories', description: 'A fixture collection.', members: [mediaIds.resumeMovie, mediaIds.completedMovie, mediaIds.kidsMovie, mediaIds.series] }],
    };
}
