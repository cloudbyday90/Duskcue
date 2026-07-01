DO $$
BEGIN
    IF NOT EXISTS (
        SELECT 1
        FROM information_schema.columns
        WHERE table_name = 'server_config'
          AND column_name = 'downloads'
    ) THEN
        ALTER TABLE server_config
            ADD COLUMN downloads JSONB NOT NULL DEFAULT '{
                "enabled": true,
                "max_quality_resolution": "1080p",
                "max_bytes_per_user": 107374182400,
                "max_bytes_per_device": 53687091200,
                "max_active_jobs_per_user": 3,
                "max_active_jobs_per_device": 2,
                "max_retained_packages_per_user": 50,
                "max_retained_packages_per_device": 25,
                "allow_lan_downloads": true,
                "allow_remote_downloads": true,
                "allow_transcoded_downloads": true,
                "default_package_expiry_days": 30,
                "ready_package_retention_days": 7,
                "user_overrides": {},
                "library_overrides": {}
            }'::jsonb;
    END IF;
END $$;
