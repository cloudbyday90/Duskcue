# Duskcue server-side notification templates (English source).
#
# Message IDs are kebab-case and match the `notification_types.in_app_template`
# value after migration `20260628020000_migrate_notification_templates_to_fluent.sql`.
# The `notification_types.name` column (snake_case) is the programmatic identifier
# and is NOT used as a Fluent key.
#
# Variables use Fluent `{ $name }` syntax. Callers pass notification `metadata`
# entries as FluentArgs; unknown variables render empty (Fluent spec).
#
# See docs/design/I18N.md for the locale negotiation chain, plural rules, and
# the `fluent-templates` static_loader! wiring in server/src/services/i18n.rs.

new-media-added =
    { $title } was added to { $library }

library-scan-complete =
    Library scan completed: { $stats }

playback-started =
    { $username } started watching { $title }

classifarr-decision =
    Classifarr routed { $title } to { $library }

server-alert =
    { $message }

server-update =
    Duskcue { $version } is available

task-failed =
    Task { $task-name } failed: { $error }

trust-alert =
    Suspicious activity detected for { $username }: { $details }

new-login =
    { $username } logged in from { $ip } on { $device }

user-invited =
    Invitation { $action } for { $email }

trakt-sync-error =
    Trakt sync failed for { $username }: { $error }

migration-completed =
    Migration { $source-name } completed: { $imported-count } item(s) imported

migration-failed =
    Migration { $source-name } failed: { $error }

download-ready-title =
    Download ready

download-ready =
    { $title } is ready for offline download

download-failed-title =
    Download failed

download-failed =
    { $title } could not be prepared for offline download: { $reason }
