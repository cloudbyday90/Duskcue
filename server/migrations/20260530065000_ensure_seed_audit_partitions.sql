DO $$
DECLARE
    partition_start DATE;
    partition_end DATE;
    partition_name TEXT;
    month_offset INTEGER;
BEGIN
    FOR month_offset IN 0..1 LOOP
        partition_start := (date_trunc('month', current_timestamp) + make_interval(months => month_offset))::date;
        partition_end := (partition_start + interval '1 month')::date;
        partition_name := format('audit_log_%s', to_char(partition_start, 'YYYY_MM'));
        EXECUTE format(
            'CREATE TABLE IF NOT EXISTS %I PARTITION OF audit_log FOR VALUES FROM (%L) TO (%L)',
            partition_name,
            partition_start,
            partition_end
        );
    END LOOP;
END $$;
