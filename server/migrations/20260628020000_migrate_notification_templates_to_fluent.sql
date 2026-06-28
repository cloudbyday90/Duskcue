-- Phase 13b Task 1: Migrate notification_types.in_app_template from English
-- template strings to Fluent message IDs.
--
-- Before this migration, in_app_template held strings like
-- '{{title}} was added to {{library}}' and notification rendering would have
-- required simple {{key}} substitution, baking English into every notification.
--
-- After this migration, in_app_template holds a Fluent message ID (kebab-case)
-- such as 'new-media-added'. The English text now lives in
-- server/locales/en/notifications.ftl, and the dispatch pipeline (Phase 13b
-- Task 2) renders it via fluent_templates::Loader::lookup_with_args with the
-- recipient's negotiated locale.
--
-- See docs/design/I18N.md "Phase 13 Notification Template Pattern" for the
-- design and server/src/services/i18n.rs for the renderer.
--
-- The notification_types.name column (snake_case) is unchanged; it is the
-- programmatic identifier and is NOT a Fluent key. in_app_template (kebab-case)
-- is the Fluent key that the renderer looks up in server/locales/<lang>/.
--
-- Idempotent: each UPDATE is keyed on name, so re-running is safe.

UPDATE notification_types SET in_app_template = 'new-media-added'        WHERE name = 'new_media_added';
UPDATE notification_types SET in_app_template = 'library-scan-complete'  WHERE name = 'library_scan_complete';
UPDATE notification_types SET in_app_template = 'playback-started'       WHERE name = 'playback_started';
UPDATE notification_types SET in_app_template = 'classifarr-decision'    WHERE name = 'classifarr_decision';
UPDATE notification_types SET in_app_template = 'server-alert'           WHERE name = 'server_alert';
UPDATE notification_types SET in_app_template = 'server-update'          WHERE name = 'server_update';
UPDATE notification_types SET in_app_template = 'task-failed'            WHERE name = 'task_failed';
UPDATE notification_types SET in_app_template = 'trust-alert'            WHERE name = 'trust_alert';
UPDATE notification_types SET in_app_template = 'new-login'              WHERE name = 'new_login';
UPDATE notification_types SET in_app_template = 'user-invited'           WHERE name = 'user_invited';
UPDATE notification_types SET in_app_template = 'trakt-sync-error'       WHERE name = 'trakt_sync_error';
