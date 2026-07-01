ALTER TABLE download_events
    DROP CONSTRAINT IF EXISTS download_events_event_type_check;

ALTER TABLE download_events
    ADD CONSTRAINT download_events_event_type_check
    CHECK (event_type IN (
        'job_created',
        'job_started',
        'job_ready',
        'job_failed',
        'job_cancelled',
        'package_served',
        'package_deleted',
        'package_expired',
        'package_revoked',
        'package_renewed',
        'quota_denied',
        'policy_denied',
        'checksum_mismatch',
        'sync_submitted',
        'cleanup'
    ));
