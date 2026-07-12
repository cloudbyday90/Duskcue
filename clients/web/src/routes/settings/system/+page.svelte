<!--
  Duskcue — Self-hosted media streaming server
  Copyright (C) 2026-2026 Duskcue Contributors

  This program is free software: licensed under AGPL-3.0
  See LICENSE file for details.
-->
<script>
    import { m } from '$lib/paraglide/messages.js';
    import { page } from '$app/stores';
    import { getServerConfig, updateConfigGroup } from '$lib/api/settings.js';
    import { hasCapability } from '$lib/stores/auth.js';
    import { notifications } from '$lib/stores/notifications.js';
    import ConfigGroupForm from '$lib/components/ConfigGroupForm.svelte';
    import {
        cloneConfig,
        getConfigPath,
        hydrateConfigGroup,
        isConfigGroupDirty,
        serializeConfigGroup,
        updateConfigField,
    } from '$lib/admin/configForms.js';

    let loading = $state(true);
    let canManage = $state(false);
    let loadError = $state(null);
    let savingGroup = $state(null);
    let activeGroup = $state('auth');
    let config = $state({});
    let original = $state({});
    let loadedOnce = $state(false);

    $effect(() => {
        const unsub = hasCapability('can_manage_server').subscribe((value) => (canManage = value));
        return unsub;
    });

    $effect(() => {
        if (!canManage) {
            loading = false;
            return;
        }
        if (loadedOnce) return;
        loadedOnce = true;
        load();
    });

    const groupSchemas = [
        {
            key: 'auth',
            title: m.routes_settings_system_page_authentication(),
            desc: m.routes_settings_system_page_login_sessions_device_linking_and_rate_limits(),
            fields: [
                select('network_mode', 'Network Mode', ['local', 'exposed']),
                toggle('auth_required', 'Require authentication'),
                toggle('require_https', 'Require HTTPS'),
                text('rp_id', 'WebAuthn RP ID'),
                text('rp_origin', 'WebAuthn RP Origin'),
                number('max_login_attempts', 'Max Login Attempts', 1, 20, 1),
                number('lockout_duration_minutes', 'Lockout Duration', 1, 240, 1, 'minutes'),
                number('invite_code_length', 'Invite Code Length', 8, 64, 1),
                number('invite_code_default_expiry_days', 'Invite Expiry', 1, 365, 1, 'days'),
                number('invite_code_max_attempts_per_ip', 'Invite Attempts per IP', 1, 50, 1),
                number('invite_code_attempt_window_minutes', 'Invite Attempt Window', 1, 240, 1, 'minutes'),
                number('device_linking_code_length', 'Device Linking Code Length', 4, 16, 1),
                number('device_linking_code_expiry_seconds', 'Device Linking Expiry', 60, 3600, 30, 'seconds'),
                number('device_linking_poll_interval_seconds', 'Device Poll Interval', 1, 30, 1, 'seconds'),
                number('reauth_code_length', 'Reauth Code Length', 8, 64, 1),
                number('reauth_code_expiry_hours', 'Reauth Expiry', 1, 168, 1, 'hours'),
                number('reauth_max_requests_per_user_per_day', 'Reauth Requests per User', 1, 20, 1),
                number('session_absolute_timeout_days', 'Session Absolute Timeout', 1, 365, 1, 'days'),
                number('session_idle_timeout_hours', 'Session Idle Timeout', 1, 720, 1, 'hours', true),
                number('session_renewal_timeout_hours', 'Session Renewal Timeout', 1, 2160, 1, 'hours'),
                number('rate_limits.global_per_minute', 'Global Rate Limit', 1, 5000, 10, 'requests/min'),
                number('rate_limits.global_burst', 'Global Burst', 1, 1000, 1),
                number('rate_limits.auth_per_minute', 'Auth Rate Limit', 1, 500, 1, 'requests/min'),
                number('rate_limits.auth_burst', 'Auth Burst', 1, 100, 1),
                number('rate_limits.authenticated_per_minute', 'Authenticated Rate Limit', 1, 5000, 10, 'requests/min'),
                number('rate_limits.authenticated_burst', 'Authenticated Burst', 1, 1000, 1),
                number('rate_limits.streaming_per_minute', 'Streaming Rate Limit', 1, 10000, 10, 'requests/min'),
                number('rate_limits.streaming_burst', 'Streaming Burst', 1, 1000, 1),
                number('rate_limits.admin_per_minute', 'Admin Rate Limit', 1, 5000, 10, 'requests/min'),
                number('rate_limits.admin_burst', 'Admin Burst', 1, 1000, 1),
            ],
        },
        {
            key: 'security',
            title: m.routes_settings_system_page_security(),
            desc: m.routes_settings_system_page_cors_tls_signed_streams_and_vpn_interface_detect(),
            fields: [
                list('allowed_origins', 'Allowed Origins'),
                toggle('tls.enabled', 'Enable TLS'),
                number('tls.port', 'TLS Port', 1, 65535, 1),
                text('tls.acme_directory', 'ACME Directory'),
                text('tls.acme_email', 'ACME Email'),
                select('tls.challenge_type', 'ACME Challenge Type', ['http-01', 'dns-01']),
                text('tls.cert_path', 'Certificate Path'),
                text('tls.key_path', 'Private Key Path'),
                number('tls.hsts_max_age_seconds', 'HSTS Max Age', 0, 63072000, 3600, 'seconds'),
                select('tls.min_tls_version', 'Minimum TLS Version', ['1.2', '1.3']),
                toggle('stream_signing.enabled', 'Enable Stream Signing'),
                number('stream_signing.manifest_ttl_seconds', 'Manifest TTL', 10, 3600, 10, 'seconds'),
                number('stream_signing.segment_ttl_seconds', 'Segment TTL', 30, 7200, 30, 'seconds'),
                number('stream_signing.key_rotation_hours', 'Key Rotation', 1, 168, 1, 'hours'),
                toggle('vpn_detection.auto_detect', 'Auto-detect VPN Interfaces'),
                list('vpn_detection.vpn_interfaces', 'VPN Interfaces'),
            ],
        },
        {
            key: 'quality',
            title: m.routes_settings_system_page_quality(),
            desc: m.routes_settings_system_page_device_capability_network_probing_and_playback_q(),
            fields: [
                toggle('capability_wizard_enabled', 'Capability Wizard'),
                number('network_probe_interval_minutes', 'Probe Interval', 1, 120, 1, 'minutes'),
                number('network_probe_browsing_interval_minutes', 'Browsing Probe Interval', 1, 240, 1, 'minutes'),
                number('network_probe_paused_interval_minutes', 'Paused Probe Interval', 1, 240, 1, 'minutes'),
                number('network_probe_bytes', 'Probe Size', 10240, 10485760, 10240, 'bytes'),
                number('throughput_estimate_window', 'Throughput Window', 1, 30, 1),
                number('throughput_safety_factor', 'Throughput Safety Factor', 0.1, 1, 0.05),
                select('default_transcode_codec', 'Default Transcode Codec', ['h264', 'hevc', 'av1']),
                select('fallback_max_resolution', 'Fallback Max Resolution', ['480p', '720p', '1080p', '1440p', '2160p']),
                number('fallback_max_bitrate_bps', 'Fallback Max Bitrate', 500000, 100000000, 500000, 'bps'),
                number('qoe_report_interval_seconds', 'QoE Report Interval', 5, 300, 5, 'seconds'),
                toggle('allow_client_side_dv_fallback', 'Allow Dolby Vision Client Fallback'),
                select('tone_mapping_algorithm', 'Tone Mapping Algorithm', ['bt2390', 'hable', 'mobius', 'reinhard']),
                number('tone_mapping_peak_nits', 'Tone Mapping Peak', 50, 1000, 10, 'nits'),
                toggle('audio_passthrough_enabled', 'Audio Passthrough'),
                select('subtitle_burn_in_policy', 'Subtitle Burn-in Policy', ['never', 'last_resort', 'always']),
                select('default_quality_mode', 'Default Quality Mode', ['auto', 'direct', 'transcode']),
            ],
        },
        {
            key: 'transcoding',
            title: m.routes_settings_system_page_transcoding(),
            desc: m.routes_settings_system_page_ffmpeg_hls_segments_and_storyboard_generation(),
            fields: [
                select('hardware_accel', 'Hardware Acceleration', ['auto', 'none', 'vaapi', 'qsv', 'nvenc', 'videotoolbox']),
                text('transcode_path', 'Transcode Path'),
                number('max_concurrent_transcodes', 'Max Concurrent Transcodes', 1, 16, 1),
                number('segment_duration_seconds', 'Segment Duration', 2, 30, 1, 'seconds'),
                toggle('allow_hw_tone_mapping', 'Hardware Tone Mapping'),
                toggle('allow_hw_subtitle_burn_in', 'Hardware Subtitle Burn-in'),
                select('default_video_codec', 'Default Video Codec', ['h264', 'hevc', 'av1']),
                select('default_audio_codec', 'Default Audio Codec', ['aac', 'opus', 'eac3', 'ac3']),
                select('max_downscale_resolution', 'Max Downscale Resolution', ['1280x720', '1920x1080', '2560x1440', '3840x2160']),
                toggle('enable_thumb_extraction', 'Extract Thumbnails'),
                number('thread_count', 'FFmpeg Thread Count', 1, 64, 1, '', true),
                select('thread_type', 'FFmpeg Thread Type', ['frame', 'slice']),
                toggle('prefer_hw_decode', 'Prefer Hardware Decode'),
                toggle('segment_detection_enabled', 'Segment Detection'),
                number('segment_safety.intro_end_padding_ms', 'Intro End Padding', 0, 10000, 250, 'ms'),
                number('segment_safety.credits_end_padding_ms', 'Credits End Padding', 0, 10000, 250, 'ms'),
                number('segment_safety.min_confidence', 'Minimum Segment Confidence', 0, 1, 0.05),
                number('segment_analysis.max_concurrent_analyses', 'Max Segment Analyses', 1, 8, 1),
                number('segment_analysis.chromaprint_sample_rate', 'Chromaprint Sample Rate', 8000, 48000, 1000, 'Hz'),
                number('segment_analysis.blackframe_amount', 'Blackframe Amount', 1, 100, 1),
                number('segment_analysis.blackframe_threshold', 'Blackframe Threshold', 1, 255, 1),
                number('segment_analysis.silence_noise_db', 'Silence Noise', -90, -10, 1, 'dB'),
                number('segment_analysis.silence_min_duration_ms', 'Silence Min Duration', 250, 10000, 250, 'ms'),
                toggle('storyboards_enabled', 'Storyboards'),
                select('storyboard_interval_mode', 'Storyboard Interval Mode', ['adaptive', 'fixed']),
                number('storyboard_fixed_interval_seconds', 'Fixed Storyboard Interval', 1, 120, 1, 'seconds'),
                number('storyboard_width', 'Storyboard Width', 160, 640, 10, 'px'),
                number('storyboard_quality', 'Storyboard Quality', 1, 100, 1),
                toggle('storyboard_keyframe_only', 'Keyframe-only Storyboards'),
                number('storyboard_sprite_columns', 'Sprite Columns', 1, 20, 1),
                number('storyboard_sprite_rows', 'Sprite Rows', 1, 40, 1),
            ],
        },
        {
            key: 'metadata',
            title: m.routes_settings_system_page_metadata(),
            desc: m.routes_settings_system_page_artwork_overlays_collections_and_metadata_provid(),
            fields: [
                list('artwork_language_priority', 'Artwork Language Priority'),
                toggle('artwork_auto_download', 'Auto-download Artwork'),
                toggle('artwork_download_originals_only', 'Download Originals Only'),
                text('asset_directory', 'Asset Directory'),
                toggle('overlays_enabled', 'Enable Overlays'),
                text('overlay_apply_schedule', 'Overlay Apply Schedule'),
                select('overlay_image_format', 'Overlay Image Format', ['webp', 'jpg', 'png']),
                number('overlay_image_quality', 'Overlay Image Quality', 1, 100, 1),
                number('overlay_max_image_size_mb', 'Overlay Max Image Size', 1, 100, 1, 'MB'),
                text('overlay_default_font', 'Overlay Default Font'),
                toggle('overlay_reapply_on_artwork_change', 'Reapply on Artwork Change'),
                toggle('collections_enabled', 'Enable Collections'),
                text('collection_sync_schedule', 'Collection Sync Schedule'),
                select('collection_default_poster_source', 'Collection Poster Source', ['auto', 'tmdb', 'asset_directory', 'community']),
                number('collection_max_items_default', 'Collection Max Items', 1, 1000, 1),
                toggle('collection_track_missing', 'Track Missing External Items'),
                number('collection_external_rate_limit_per_minute', 'External Collection Rate Limit', 1, 120, 1, 'requests/min'),
                toggle('providers.tmdb.enabled', 'TMDB Enabled'),
                password('providers.tmdb.api_key', 'TMDB API Key'),
                password('providers.tmdb.access_token', 'TMDB Access Token'),
                toggle('providers.tmdb.include_adult', 'TMDB Include Adult Content'),
                toggle('providers.tvdb.enabled', 'TVDB Enabled'),
                password('providers.tvdb.api_key', 'TVDB API Key'),
                toggle('providers.fanart.enabled', 'Fanart Enabled'),
                password('providers.fanart.api_key', 'Fanart API Key'),
                toggle('providers.omdb.enabled', 'OMDb Enabled'),
                password('providers.omdb.api_key', 'OMDb API Key'),
                number('auto_refresh_hours', 'Metadata Auto-refresh', 1, 720, 1, 'hours'),
                number('max_concurrent_probes', 'Max Concurrent Probes', 1, 16, 1),
                text('metadata_language', 'Metadata Language'),
                number('enrichment_timeout_seconds', 'Enrichment Timeout', 5, 300, 5, 'seconds'),
                number('export_cache_days', 'Export Cache Retention', 1, 90, 1, 'days'),
            ],
        },
        {
            key: 'backup',
            title: m.routes_settings_system_page_backup(),
            desc: m.routes_settings_system_page_wal_g_pg_dump_retention_and_verification_setting(),
            fields: [
                toggle('wal_g_enabled', 'Enable WAL-G Backups'),
                select('wal_g_storage_type', 'WAL-G Storage Type', ['local', 's3']),
                text('wal_g_storage_path', 'WAL-G Local Path'),
                text('wal_g_s3_endpoint', 'WAL-G S3 Endpoint'),
                password('wal_g_s3_bucket', 'WAL-G S3 Bucket'),
                text('wal_g_s3_prefix', 'WAL-G S3 Prefix'),
                text('wal_g_s3_region', 'WAL-G S3 Region'),
                toggle('wal_g_encryption_enabled', 'Enable WAL-G Encryption'),
                password('wal_g_encryption_key_id', 'WAL-G Encryption Key ID'),
                toggle('wal_g_encryption_auto_s3', 'Auto-enable Encryption for S3'),
                number('wal_g_retention_full', 'Full Backup Retention', 1, 90, 1),
                number('wal_g_retention_weekly', 'Weekly Backup Retention', 1, 52, 1),
                number('wal_g_retention_monthly', 'Monthly Backup Retention', 1, 120, 1),
                toggle('pg_dump_enabled', 'Enable pg_dump Backups'),
                text('pg_dump_storage_path', 'pg_dump Storage Path'),
                number('pg_dump_retention_daily', 'Daily Dump Retention', 1, 365, 1, 'days'),
                number('pg_dump_retention_monthly', 'Monthly Dump Retention', 1, 120, 1, 'months'),
                number('archive_timeout_seconds', 'Archive Timeout', 10, 3600, 10, 'seconds'),
                toggle('data_checksums', 'Data Checksums'),
                toggle('verification_enabled', 'Backup Verification'),
            ],
        },
        {
            key: 'storage',
            title: m.routes_settings_system_page_storage(),
            desc: m.routes_settings_system_page_cache_locations_cache_limits_and_disk_warning_th(),
            fields: [
                text('storyboard_path', 'Storyboard Cache Path'),
                text('image_cache_path', 'Image Cache Path'),
                text('hls_cache_path', 'HLS Cache Path'),
                text('transcode_path', 'Transcode Path'),
                number('storyboard_max_cache_gb', 'Storyboard Cache Limit', 1, 10000, 1, 'GB', true),
                number('image_cache_max_size_mb', 'Image Cache Limit', 128, 100000, 128, 'MB'),
                number('hls_cache_max_size_mb', 'HLS Cache Limit', 128, 100000, 128, 'MB'),
                select('storyboard_eviction_policy', 'Storyboard Eviction Policy', ['lru', 'ttl', 'manual']),
                number('disk_space_warnings.data_threshold_percent', 'Data Volume Warning', 1, 99, 1, '%'),
                number('disk_space_warnings.cache_threshold_percent', 'Cache Volume Warning', 1, 99, 1, '%'),
                number('disk_space_warnings.transcode_threshold_percent', 'Transcode Volume Warning', 1, 99, 1, '%'),
                number('disk_space_warnings.check_interval_seconds', 'Disk Check Interval', 60, 86400, 60, 'seconds'),
                toggle('disk_space_warnings.notify_on_warning', 'Notify on Disk Warnings'),
            ],
        },
        {
            key: 'maintenance',
            title: m.routes_settings_system_page_maintenance(),
            desc: m.routes_settings_system_page_autovacuum_index_bloat_partition_retention_and_a(),
            fields: [
                toggle('autovacuum_tuning_enabled', 'Autovacuum Tuning'),
                toggle('reindex_enabled', 'Reindex Maintenance'),
                text('reindex_schedule', 'Reindex Schedule'),
                number('reindex_bloat_threshold_percent', 'Reindex Bloat Threshold', 1, 90, 1, '%'),
                number('reindex_min_index_size_mb', 'Minimum Index Size', 1, 10240, 1, 'MB'),
                number('partition_retention_months.play_sessions', 'Play Session Retention', 1, 120, 1, 'months'),
                number('partition_retention_months.play_events', 'Play Event Retention', 1, 120, 1, 'months'),
                number('partition_retention_months.audit_log', 'Audit Log Retention', 1, 120, 1, 'months'),
                toggle('analyze_parent_tables_enabled', 'Analyze Parent Tables'),
                text('analyze_parent_schedule', 'Analyze Parent Schedule'),
            ],
        },
        {
            key: 'resource_limits',
            title: m.routes_settings_system_page_resource_limits(),
            desc: m.routes_settings_system_page_memory_stale_session_and_ffmpeg_process_limits(),
            fields: [
                number('max_concurrent_transcodes', 'Max Concurrent Transcodes', 1, 16, 1),
                number('transcode_mem_threshold_percent', 'Transcode Memory Threshold', 1, 99, 1, '%'),
                number('ffmpeg_idle_timeout_secs', 'FFmpeg Idle Timeout', 30, 3600, 30, 'seconds'),
                number('ffmpeg_shutdown_grace_secs', 'FFmpeg Shutdown Grace', 1, 120, 1, 'seconds'),
                number('watchdog_interval_secs', 'Watchdog Interval', 5, 600, 5, 'seconds'),
                number('memory_warning_percent', 'Memory Warning', 1, 99, 1, '%'),
                number('memory_critical_percent', 'Memory Critical', 1, 99, 1, '%'),
                number('stale_session_timeout_secs', 'Stale Session Timeout', 60, 86400, 60, 'seconds'),
            ],
        },
        {
            key: 'cpu',
            title: m.routes_settings_system_page_cpu(),
            desc: m.routes_settings_system_page_ffmpeg_scheduling_cpu_thresholds_and_thermal_gua(),
            fields: [
                number('transcode_cpu_threshold_percent', 'Transcode CPU Threshold', 1, 99, 1, '%'),
                number('cpu_warning_percent', 'CPU Warning', 1, 99, 1, '%'),
                number('cpu_critical_percent', 'CPU Critical', 1, 99, 1, '%'),
                number('ffmpeg_threads', 'FFmpeg Threads', 1, 64, 1, '', true),
                select('ffmpeg_thread_type', 'FFmpeg Thread Type', ['frame', 'slice']),
                toggle('ffmpeg_nice', 'Use nice for FFmpeg'),
                toggle('ffmpeg_ionice', 'Use ionice for FFmpeg'),
                text('cpu_affinity', 'CPU Affinity'),
                toggle('hw_accel_auto_detect', 'Hardware Accel Auto-detect'),
                toggle('thermal_throttle_enabled', 'Thermal Throttling'),
                number('thermal_warning_celsius', 'Thermal Warning', 30, 110, 1, 'C'),
                number('thermal_critical_celsius', 'Thermal Critical', 30, 120, 1, 'C'),
            ],
        },
        {
            key: 'network',
            title: m.routes_settings_system_page_network(),
            desc: m.routes_settings_system_page_operational_network_allowlists(),
            fields: [list('allowed_metrics_subnets', 'Allowed Metrics Subnets')],
        },
        {
            key: 'integrations',
            title: m.routes_settings_system_page_integrations(),
            desc: m.routes_settings_system_page_trakt_integration_credentials(),
            fields: [
                text('trakt.client_id', 'Trakt Client ID'),
                password('trakt.client_secret', 'Trakt Client Secret'),
                text('trakt.redirect_uri', 'Trakt Redirect URI'),
            ],
        },
        {
            key: 'analytics',
            title: m.routes_settings_system_page_analytics(),
            desc: m.routes_settings_system_page_geoip_and_impossible_travel_trust_event_threshol(),
            fields: [
                toggle('geoip_enabled', 'GeoIP Enrichment'),
                toggle('impossible_travel_enabled', 'Impossible Travel Detection'),
                number('velocity_threshold_kmh', 'Velocity Threshold', 100, 5000, 50, 'km/h'),
                number('min_distance_km', 'Minimum Distance', 1, 5000, 10, 'km'),
                number('lookback_hours', 'Lookback Window', 1, 168, 1, 'hours'),
                toggle('same_country_suppress', 'Suppress Same-country Events'),
                list('trusted_ips', 'Trusted IPs'),
                list('trusted_cidrs', 'Trusted CIDRs'),
            ],
        },
        {
            key: 'logging',
            title: m.routes_settings_system_page_logging(),
            desc: m.routes_settings_system_page_log_level_rotation_and_output_format(),
            fields: [
                select('level', 'Log Level', ['trace', 'debug', 'info', 'warn', 'error']),
                number('max_file_size_mb', 'Max File Size', 1, 1024, 1, 'MB'),
                number('max_files', 'Max Files', 1, 100, 1),
                select('format', 'Format', ['json', 'pretty', 'compact']),
            ],
        },
        {
            key: 'notifications',
            title: m.routes_settings_system_page_notifications(),
            desc: m.routes_settings_system_page_notification_dispatch_webhook_active_cleanup_and(),
            fields: [
                number('cleanup_max_age_days', 'Cleanup Max Age', 1, 3650, 1, 'days'),
                toggle('push.enabled', 'Enable Push Notifications', 'Mobile push sends to registered FCM, APNs, or UnifiedPush devices.'),
                select('push.provider', 'Push Provider', ['fcm', 'apns', 'unifiedpush'], 'Active outbound mobile push provider.'),
                text('push.fcm.project_id', 'FCM Project ID'),
                text('push.fcm.client_email', 'FCM Client Email'),
                password('push.fcm.private_key', 'FCM Private Key', 'Service account private_key value from Firebase.'),
                text('push.apns.team_id', 'APNs Team ID'),
                text('push.apns.key_id', 'APNs Key ID'),
                text('push.apns.bundle_id', 'APNs Bundle ID'),
                password('push.apns.private_key', 'APNs Private Key', '.p8 token-auth private key.'),
                toggle('push.apns.sandbox', 'APNs Sandbox', 'Use api.sandbox.push.apple.com for development builds.'),
                toggle('push.unifiedpush.enabled', 'Enable UnifiedPush', 'Uses each Android device endpoint URL as the delivery target.'),
                text('webhook.url', 'Webhook URL', 'Destination URL. For ntfy/gotify/discord/slack, include any required token in the URL.'),
                select('webhook.format', 'Webhook Format', ['generic', 'ntfy', 'gotify', 'discord', 'slack'], 'Payload shape. ntfy = plain text + headers; gotify/discord/slack = native JSON; generic = full Duskcue JSON with HMAC signature.'),
                password('webhook.secret', 'Webhook Secret', 'Optional shared secret for X-Duskcue-Signature HMAC-SHA256. Applied to all formats.'),
            ],
        },
    ];

    const navigationSections = [
        {
            label: m.routes_admin_page_server(),
            keys: ['auth', 'security', 'backup', 'storage', 'maintenance', 'resource_limits', 'cpu', 'network', 'logging'],
        },
        {
            label: m.routes_admin_page_access_and_delivery(),
            keys: ['quality', 'transcoding', 'notifications'],
        },
        {
            label: m.routes_admin_page_library_management(),
            keys: ['metadata'],
        },
        {
            label: m.routes_admin_page_advanced(),
            keys: ['integrations', 'analytics'],
        },
    ];

    let activeSchema = $derived(groupSchemas.find((group) => group.key === activeGroup) || groupSchemas[0]);
    let activeDirty = $derived(isGroupDirty(activeGroup));

    $effect(() => {
        const group = $page.url.searchParams.get('group');
        if (groupSchemas.some((item) => item.key === group)) {
            activeGroup = group;
        }
    });

    function toggle(path, label, hint = '') {
        return { path, label, type: 'boolean', hint };
    }

    function number(path, label, min, max, step, unit = '', nullable = false) {
        return { path, label, type: 'number', min, max, step, unit, nullable };
    }

    function select(path, label, options, hint = '') {
        return { path, label, type: 'select', options, hint };
    }

    function text(path, label, hint = '') {
        return { path, label, type: 'text', hint };
    }

    function password(path, label, hint = '') {
        return { path, label, type: 'password', hint };
    }

    function list(path, label, hint = 'Comma-separated values') {
        return { path, label, type: 'list', hint };
    }

    async function load() {
        loading = true;
        loadError = null;
        try {
            const response = await getServerConfig();
            const next = {};
            for (const group of groupSchemas) {
                next[group.key] = hydrateConfigGroup(response.config?.[group.key] || {}, group.fields);
            }
            config = next;
            original = cloneConfig(next);
        } catch (err) {
            loadError = err.detail || err.message || m.routes_settings_system_page_failed_to_load_server_configuration();
        } finally {
            loading = false;
        }
    }

    function serializeGroup(groupKey) {
        return serializeGroupFrom(groupKey, config);
    }

    function serializeGroupFrom(groupKey, source) {
        const group = groupSchemas.find((item) => item.key === groupKey);
        return serializeConfigGroup(source[groupKey] || {}, group?.fields || []);
    }

    function isGroupDirty(groupKey) {
        const group = groupSchemas.find((item) => item.key === groupKey);
        if (!group || !config[groupKey] || !original[groupKey]) return false;
        return isConfigGroupDirty(config[groupKey], original[groupKey], group.fields);
    }

    async function saveActiveGroup() {
        savingGroup = activeGroup;
        try {
            const payload = serializeGroup(activeGroup);
            const response = await updateConfigGroup(activeGroup, payload);
            const group = activeSchema;
            config[activeGroup] = hydrateConfigGroup(response.value || payload, group.fields);
            original[activeGroup] = cloneConfig(config[activeGroup]);
            notifications.success(`${group.title} settings saved`);
        } catch (err) {
            notifications.error(err.detail || err.message || m.routes_settings_system_page_failed_to_save_settings());
        } finally {
            savingGroup = null;
        }
    }

    function fieldValue(field) {
        const value = getConfigPath(config[activeGroup] || {}, field.path);
        if (field.type === 'boolean') return Boolean(value);
        if (value === undefined || value === null) return '';
        return value;
    }

    function updateField(field, value) {
        config[activeGroup] = updateConfigField(config[activeGroup] || {}, field, value);
    }

    function schemaFor(key) {
        return groupSchemas.find((group) => group.key === key);
    }
</script>

<div class="system-settings">
    <div class="page-header">
        <div>
            <a href="/admin" class="back-link">{m.routes_admin_page_admin()}</a>
            <h1 class="page-title">{m.routes_settings_system_page_system_configuration()}</h1>
        </div>
        {#if !loading && canManage && !loadError}
            <button class="btn-primary" onclick={saveActiveGroup} disabled={!activeDirty || savingGroup}>
                {savingGroup === activeGroup ? 'Saving…' : 'Save Group'}
            </button>
        {/if}
    </div>

    {#if loading}
        <div class="loading-state"><div class="loading-spinner"></div></div>
    {:else if !canManage}
        <div class="empty-state">{m.routes_settings_system_page_you_do_not_have_permission_to_manage_server_conf()}</div>
    {:else if loadError}
        <div class="empty-state">
            <p class="error-text">{loadError}</p>
            <button class="btn-secondary" onclick={load}>{m.routes_settings_system_page_retry()}</button>
        </div>
    {:else}
        <div class="settings-layout">
            <nav class="group-nav" aria-label={m.routes_settings_system_page_configuration_groups()}>
                {#each navigationSections as section}
                    <div class="group-nav-section">
                        <span class="group-nav-label">{section.label}</span>
                        {#each section.keys as key}
                            {@const group = schemaFor(key)}
                            {#if group}
                                <a
                                    href={`/settings/system?group=${group.key}`}
                                    class:active={activeGroup === group.key}
                                    aria-current={activeGroup === group.key ? 'page' : undefined}
                                >
                                    <span>{group.title}</span>
                                    {#if isGroupDirty(group.key)}<span class="dirty-dot"></span>{/if}
                                </a>
                            {/if}
                        {/each}
                    </div>
                {/each}
            </nav>

            <section class="settings-card">
                <div class="card-header">
                    <div>
                        <h2 class="card-title">{activeSchema.title}</h2>
                        <p class="card-desc">{activeSchema.desc}</p>
                    </div>
                    <span class:dirty-badge={activeDirty} class="status-badge">
                        {activeDirty ? 'Unsaved' : 'Saved'}
                    </span>
                </div>

                <div class="card-body">
                    <ConfigGroupForm
                        fields={activeSchema.fields}
                        valueFor={fieldValue}
                        onchange={updateField}
                    />
                </div>
            </section>
        </div>
    {/if}
</div>

<style>
    .system-settings {
        display: flex;
        flex-direction: column;
        gap: 1.5rem;
        max-width: 1180px;
    }

    .page-header {
        display: flex;
        align-items: flex-start;
        justify-content: space-between;
        gap: 1rem;
    }

    .back-link {
        font-size: 0.8125rem;
        color: var(--color-text-muted);
    }

    .page-title {
        font-size: 1.5rem;
        font-weight: 700;
        color: var(--color-text-primary);
        margin-top: 0.25rem;
    }

    .settings-layout {
        display: grid;
        grid-template-columns: 230px minmax(0, 1fr);
        gap: 1rem;
        align-items: start;
    }

    .group-nav {
        display: flex;
        flex-direction: column;
        gap: 1rem;
        position: sticky;
        top: 1rem;
    }

    .group-nav-section {
        display: flex;
        flex-direction: column;
        gap: 0.25rem;
    }

    .group-nav-label {
        padding: 0 0.75rem;
        font-size: 0.625rem;
        font-weight: 600;
        letter-spacing: 0.05em;
        text-transform: uppercase;
        color: var(--color-text-muted);
    }

    .group-nav a {
        display: flex;
        align-items: center;
        justify-content: space-between;
        gap: 0.5rem;
        padding: 0.625rem 0.75rem;
        color: var(--color-text-secondary);
        background-color: transparent;
        border: 1px solid transparent;
        border-radius: var(--radius-sm);
        font-size: 0.8125rem;
        text-align: start;
    }

    .group-nav a:hover,
    .group-nav a.active {
        color: var(--color-text-primary);
        background-color: var(--color-bg-surface);
        border-color: var(--color-border-subtle);
    }

    .dirty-dot {
        width: 6px;
        height: 6px;
        border-radius: 50%;
        background-color: var(--color-warning);
        flex-shrink: 0;
    }

    .settings-card {
        background-color: var(--color-bg-surface);
        border: 1px solid var(--color-border-subtle);
        border-radius: var(--radius-md);
        overflow: hidden;
    }

    .card-header {
        display: flex;
        align-items: flex-start;
        justify-content: space-between;
        gap: 1rem;
        padding: 1rem 1.25rem;
        border-bottom: 1px solid var(--color-border-subtle);
    }

    .card-title {
        font-size: 1rem;
        font-weight: 600;
        color: var(--color-text-primary);
    }

    .card-desc {
        margin: 0.25rem 0 0;
        font-size: 0.75rem;
        color: var(--color-text-muted);
    }

    .card-body {
        padding: 1.25rem;
    }

    .status-badge {
        font-size: 0.625rem;
        font-weight: 600;
        text-transform: uppercase;
        letter-spacing: 0.05em;
        padding: 0.1875rem 0.5rem;
        border-radius: var(--radius-sm);
        color: var(--color-success);
        background-color: var(--color-success-bg);
        white-space: nowrap;
    }

    .dirty-badge {
        color: var(--color-warning);
        background-color: var(--color-warning-bg);
    }

    .btn-primary,
    .btn-secondary {
        padding: 0.5rem 1.25rem;
        font-size: 0.8125rem;
        font-weight: 600;
        border-radius: var(--radius-sm);
        white-space: nowrap;
    }

    .btn-primary {
        background-color: var(--color-accent);
        color: var(--color-bg-deep);
    }

    .btn-primary:disabled {
        opacity: 0.5;
    }

    .btn-secondary {
        background-color: var(--color-bg-elevated);
        color: var(--color-text-secondary);
        border: 1px solid var(--color-border);
    }

    .empty-state,
    .loading-state {
        display: flex;
        align-items: center;
        justify-content: center;
        min-height: 240px;
        color: var(--color-text-muted);
        font-size: 0.875rem;
    }

    .empty-state {
        flex-direction: column;
        gap: 1rem;
    }

    .error-text {
        color: var(--color-error);
    }

    .loading-spinner {
        width: 32px;
        height: 32px;
        border: 3px solid var(--color-border);
        border-top-color: var(--color-accent);
        border-radius: 50%;
        animation: spin 0.8s linear infinite;
    }

    @keyframes spin {
        to {
            transform: rotate(360deg);
        }
    }

    @media (max-width: 900px) {
        .settings-layout {
            grid-template-columns: 1fr;
        }

        .group-nav {
            position: static;
        }

        .group-nav-section {
            display: grid;
            grid-template-columns: repeat(auto-fill, minmax(140px, 1fr));
        }

        .group-nav-label {
            grid-column: 1 / -1;
        }
    }

    @media (max-width: 700px) {
        .page-header,
        .card-header {
            flex-direction: column;
            align-items: flex-start;
        }

    }
</style>
