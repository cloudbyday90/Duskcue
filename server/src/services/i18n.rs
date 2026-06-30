// Duskcue — Self-hosted media streaming server
// Copyright (C) 2026-2026 Duskcue Contributors
//
// This program is free software: you can redistribute it and/or modify
// it under the terms of the GNU Affero General Public License as published by
// the Free Software Foundation, either version 3 of the License, or
// (at your option) any later version.
//
// This program is distributed in the hope that it will be useful,
// but WITHOUT ANY WARRANTY; without even the implied warranty of
// MERCHANTABILITY or FITNESS FOR A PARTICULAR PURPOSE. See the
// GNU Affero General Public License for more details.
//
// You should have received a copy of the GNU Affero General Public License
// along with this program. If not, see <https://www.gnu.org/licenses/>.

//! Server-side internationalization via Fluent (`fluent-templates`).
//!
//! Compiles `.ftl` files under `server/locales/` into the binary at build time
//! via the `static_loader!` macro. The `LOCALES` static is `Sync` and safe to
//! access from any async context — locale is passed explicitly per lookup,
//! which is the decisive difference from the thread-local `fluent-i18n` model
//! (see `docs/design/I18N.md` "Crate Selection Rationale").
//!
//! # Locale negotiation
//!
//! [`negotiate_locale`] implements the chain from `I18N.md`:
//! 1. User preference (`users.metadata.locale`)
//! 2. `Accept-Language` header
//! 3. Base locale (English)
//!
//! # Notification rendering
//!
//! [`render`] looks up a Fluent message ID with optional args. Call sites pass
//! the recipient's negotiated locale and the notification `metadata` map. When
//! a message ID is absent from the loaded resources the function logs a warning
//! and returns the ID verbatim so the failure is visible rather than silent
//! (an empty notification body is worse for debugging than a raw key).
//!
//! Task 2 (dispatch pipeline) is the intended consumer; Task 1 ships the
//! infrastructure and the template migration so the consumer never sees a raw
//! English template string.

use std::borrow::Cow;
use std::collections::HashMap;

use fluent_bundle::FluentValue;
use fluent_langneg::{NegotiationStrategy, negotiate_languages, parse_accepted_languages};
use fluent_templates::{Loader, static_loader};
use unic_langid::{LanguageIdentifier, langid};

static_loader! {
    static LOCALES = {
        locales: "locales",
        fallback_language: "en",
        customise: |bundle| bundle.set_use_isolating(false),
    };
}

pub const DEFAULT_LOCALE: LanguageIdentifier = langid!("en");
pub const LOCALE_FR: LanguageIdentifier = langid!("fr");
pub const LOCALE_DE: LanguageIdentifier = langid!("de");
pub const LOCALE_ES: LanguageIdentifier = langid!("es");
pub const LOCALE_IT: LanguageIdentifier = langid!("it");
pub const LOCALE_AR: LanguageIdentifier = langid!("ar");
pub const LOCALE_ZH_HANS: LanguageIdentifier = langid!("zh-Hans");
pub const LOCALE_ZH_HANT: LanguageIdentifier = langid!("zh-Hant");

pub const AVAILABLE_LOCALES: &[LanguageIdentifier] = &[
    DEFAULT_LOCALE,
    LOCALE_FR,
    LOCALE_DE,
    LOCALE_ES,
    LOCALE_IT,
    LOCALE_AR,
    LOCALE_ZH_HANS,
    LOCALE_ZH_HANT,
];

pub const REVIEWED_UI_LOCALES: &[LanguageIdentifier] = &[DEFAULT_LOCALE];

pub fn is_reviewed_ui_locale(locale: &str) -> bool {
    let Ok(locale) = locale.trim().parse::<LanguageIdentifier>() else {
        return false;
    };
    REVIEWED_UI_LOCALES.contains(&locale)
}

pub fn negotiate_locale(
    user_preference: Option<&str>,
    accept_language: Option<&str>,
) -> LanguageIdentifier {
    let default_locale = DEFAULT_LOCALE;

    if let Some(preference) = user_preference.filter(|s| !s.trim().is_empty())
        && let Ok(requested) = preference.trim().parse::<LanguageIdentifier>()
    {
        let resolved = negotiate_languages(
            std::slice::from_ref(&requested),
            AVAILABLE_LOCALES,
            Some(&default_locale),
            NegotiationStrategy::Filtering,
        );
        if let Some(chosen) = resolved.first() {
            return (*chosen).clone();
        }
    }

    if let Some(header) = accept_language.filter(|s| !s.trim().is_empty()) {
        let requested: Vec<LanguageIdentifier> = parse_accepted_languages(header);
        if !requested.is_empty() {
            let resolved = negotiate_languages(
                &requested,
                AVAILABLE_LOCALES,
                Some(&default_locale),
                NegotiationStrategy::Filtering,
            );
            if let Some(chosen) = resolved.first() {
                return (*chosen).clone();
            }
        }
    }

    DEFAULT_LOCALE
}

pub fn render(
    message_id: &str,
    locale: &LanguageIdentifier,
    args: &HashMap<Cow<'static, str>, FluentValue>,
) -> String {
    let rendered = LOCALES.lookup_with_args(locale, message_id, args);
    if rendered.starts_with(NOT_FOUND_PREFIX) {
        tracing::warn!(
            message_id = %message_id,
            locale = %locale,
            "Fluent message ID not found in loaded resources; returning raw ID"
        );
        return message_id.to_string();
    }
    rendered
}

const NOT_FOUND_PREFIX: &str = "Unknown localization key:";

pub fn args_from_metadata(
    metadata: &serde_json::Map<String, serde_json::Value>,
) -> HashMap<Cow<'static, str>, FluentValue<'static>> {
    metadata
        .iter()
        .map(|(key, value)| {
            let fluent_value = match value {
                serde_json::Value::String(s) => FluentValue::String(s.clone().into()),
                serde_json::Value::Number(n) => {
                    if let Some(i) = n.as_i64() {
                        FluentValue::Number(i.into())
                    } else if let Some(f) = n.as_f64() {
                        FluentValue::Number(f.into())
                    } else {
                        FluentValue::String(value.to_string().into())
                    }
                }
                _ => FluentValue::String(value.to_string().into()),
            };
            (Cow::Owned(normalize_arg_key(key)), fluent_value)
        })
        .collect()
}

fn normalize_arg_key(key: &str) -> String {
    key.replace('_', "-")
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn negotiates_user_preference_first() {
        let resolved = negotiate_locale(Some("fr"), Some("de"));
        assert_eq!(resolved, LOCALE_FR);
    }

    #[test]
    fn negotiates_accept_language_without_user_preference() {
        let resolved = negotiate_locale(None, Some("de,en;q=0.8"));
        assert_eq!(resolved, LOCALE_DE);
    }

    #[test]
    fn negotiates_scripted_chinese_locale() {
        let resolved = negotiate_locale(Some("zh-Hant"), Some("zh-Hans,en;q=0.8"));
        assert_eq!(resolved, LOCALE_ZH_HANT);
    }

    #[test]
    fn falls_back_to_default_for_empty_inputs() {
        let resolved = negotiate_locale(None, None);
        assert_eq!(resolved, DEFAULT_LOCALE);
    }

    #[test]
    fn falls_back_to_default_for_blank_inputs() {
        let resolved = negotiate_locale(Some("   "), Some(""));
        assert_eq!(resolved, DEFAULT_LOCALE);
    }

    #[test]
    fn falls_back_to_default_for_unparseable_user_preference() {
        let resolved = negotiate_locale(Some("not-a-locale!!!"), None);
        assert_eq!(resolved, DEFAULT_LOCALE);
    }

    #[test]
    fn falls_back_to_default_for_unparseable_accept_language() {
        let resolved = negotiate_locale(None, Some("garbage,,,"));
        assert_eq!(resolved, DEFAULT_LOCALE);
    }

    #[test]
    fn renders_new_media_added_with_args() {
        let mut args = HashMap::new();
        args.insert(
            Cow::Borrowed("title"),
            FluentValue::String("Inception".to_string().into()),
        );
        args.insert(
            Cow::Borrowed("library"),
            FluentValue::String("Movies".to_string().into()),
        );
        let rendered = render("new-media-added", &DEFAULT_LOCALE, &args);
        assert_eq!(rendered, "Inception was added to Movies");
    }

    #[test]
    fn renders_server_alert_with_message_arg() {
        let mut args = HashMap::new();
        args.insert(
            Cow::Borrowed("message"),
            FluentValue::String("Disk 90% full".to_string().into()),
        );
        let rendered = render("server-alert", &DEFAULT_LOCALE, &args);
        assert_eq!(rendered, "Disk 90% full");
    }

    #[test]
    fn renders_server_update_with_version_arg() {
        let mut args = HashMap::new();
        args.insert(
            Cow::Borrowed("version"),
            FluentValue::String("1.0.0".to_string().into()),
        );
        let rendered = render("server-update", &DEFAULT_LOCALE, &args);
        assert_eq!(rendered, "Duskcue 1.0.0 is available");
    }

    #[test]
    fn renders_task_failed_with_kebab_case_arg() {
        let mut args = HashMap::new();
        args.insert(
            Cow::Borrowed("task-name"),
            FluentValue::String("Library Scan".to_string().into()),
        );
        args.insert(
            Cow::Borrowed("error"),
            FluentValue::String("permission denied".to_string().into()),
        );
        let rendered = render("task-failed", &DEFAULT_LOCALE, &args);
        assert_eq!(rendered, "Task Library Scan failed: permission denied");
    }

    #[test]
    fn returns_raw_id_for_unknown_message() {
        let args = HashMap::new();
        let rendered = render("does-not-exist", &DEFAULT_LOCALE, &args);
        assert_eq!(rendered, "does-not-exist");
    }

    #[test]
    fn renders_without_isolating_marks() {
        let mut args = HashMap::new();
        args.insert(
            Cow::Borrowed("title"),
            FluentValue::String("Test".to_string().into()),
        );
        args.insert(
            Cow::Borrowed("library"),
            FluentValue::String("Lib".to_string().into()),
        );
        let rendered = render("new-media-added", &DEFAULT_LOCALE, &args);
        assert!(!rendered.contains('\u{2068}'));
        assert!(!rendered.contains('\u{2069}'));
    }

    #[test]
    fn converts_metadata_string_values_to_fluent_args() {
        let mut metadata = serde_json::Map::new();
        metadata.insert("title".to_string(), json!("The Matrix"));
        metadata.insert("library".to_string(), json!("Action Movies"));
        let args = args_from_metadata(&metadata);
        let rendered = render("new-media-added", &DEFAULT_LOCALE, &args);
        assert_eq!(rendered, "The Matrix was added to Action Movies");
    }

    #[test]
    fn normalizes_snake_case_metadata_keys_to_kebab_case() {
        let mut metadata = serde_json::Map::new();
        metadata.insert("task_name".to_string(), json!("Backup"));
        metadata.insert("error".to_string(), json!("timeout"));
        let args = args_from_metadata(&metadata);
        let rendered = render("task-failed", &DEFAULT_LOCALE, &args);
        assert_eq!(rendered, "Task Backup failed: timeout");
    }

    #[test]
    fn returns_raw_id_when_required_arg_is_missing() {
        let args = HashMap::new();
        let rendered = render("server-alert", &DEFAULT_LOCALE, &args);
        assert_eq!(rendered, "server-alert");
    }

    #[test]
    fn renders_all_seeded_notification_ids() {
        let cases = [
            (
                "new-media-added",
                vec![("title", "Movie"), ("library", "Lib")],
            ),
            ("library-scan-complete", vec![("stats", "100 items")]),
            (
                "playback-started",
                vec![("username", "Bob"), ("title", "Show")],
            ),
            (
                "classifarr-decision",
                vec![("title", "Movie"), ("library", "Lib")],
            ),
            ("server-alert", vec![("message", "Alert")]),
            ("server-update", vec![("version", "1.0")]),
            ("task-failed", vec![("task-name", "Task"), ("error", "Err")]),
            (
                "trust-alert",
                vec![("username", "Bob"), ("details", "Suspicious")],
            ),
            (
                "new-login",
                vec![("username", "Bob"), ("ip", "1.2.3.4"), ("device", "Chrome")],
            ),
            (
                "user-invited",
                vec![("action", "created"), ("email", "a@b.com")],
            ),
            (
                "trakt-sync-error",
                vec![("username", "Bob"), ("error", "timeout")],
            ),
            (
                "migration-completed",
                vec![("source-name", "Plex"), ("imported-count", "42")],
            ),
            (
                "migration-failed",
                vec![("source-name", "Plex"), ("error", "timeout")],
            ),
        ];
        for (message_id, arg_pairs) in cases {
            let args: HashMap<Cow<'static, str>, FluentValue> = arg_pairs
                .into_iter()
                .map(|(k, v)| (Cow::Borrowed(k), FluentValue::String(v.to_string().into())))
                .collect();
            let rendered = render(message_id, &DEFAULT_LOCALE, &args);
            assert_ne!(
                rendered, message_id,
                "Fluent message `{message_id}` missing from en/notifications.ftl (render returned the raw ID)"
            );
        }
    }

    #[test]
    fn available_locales_includes_default() {
        assert!(AVAILABLE_LOCALES.contains(&DEFAULT_LOCALE));
    }

    #[test]
    fn reviewed_ui_locales_include_default() {
        assert!(REVIEWED_UI_LOCALES.contains(&DEFAULT_LOCALE));
    }

    #[test]
    fn reviewed_ui_locales_exclude_preview_translations() {
        assert!(is_reviewed_ui_locale("en"));
        assert!(!is_reviewed_ui_locale("fr"));
        assert!(!is_reviewed_ui_locale("ar"));
        assert!(!is_reviewed_ui_locale("not-a-locale"));
    }

    #[test]
    fn available_locales_matches_launch_window_targets() {
        assert_eq!(
            AVAILABLE_LOCALES,
            &[
                DEFAULT_LOCALE,
                LOCALE_FR,
                LOCALE_DE,
                LOCALE_ES,
                LOCALE_IT,
                LOCALE_AR,
                LOCALE_ZH_HANS,
                LOCALE_ZH_HANT
            ]
        );
    }

    #[test]
    fn all_available_locales_render_seeded_notifications() {
        let cases = [
            (
                "new-media-added",
                vec![("title", "Movie"), ("library", "Lib")],
            ),
            ("library-scan-complete", vec![("stats", "100 items")]),
            (
                "playback-started",
                vec![("username", "Bob"), ("title", "Show")],
            ),
            (
                "classifarr-decision",
                vec![("title", "Movie"), ("library", "Lib")],
            ),
            ("server-alert", vec![("message", "Alert")]),
            ("server-update", vec![("version", "1.0")]),
            ("task-failed", vec![("task-name", "Task"), ("error", "Err")]),
            (
                "trust-alert",
                vec![("username", "Bob"), ("details", "Suspicious")],
            ),
            (
                "new-login",
                vec![("username", "Bob"), ("ip", "1.2.3.4"), ("device", "Chrome")],
            ),
            (
                "user-invited",
                vec![("action", "created"), ("email", "a@b.com")],
            ),
            (
                "trakt-sync-error",
                vec![("username", "Bob"), ("error", "timeout")],
            ),
            (
                "migration-completed",
                vec![("source-name", "Plex"), ("imported-count", "42")],
            ),
            (
                "migration-failed",
                vec![("source-name", "Plex"), ("error", "timeout")],
            ),
        ];

        for locale in AVAILABLE_LOCALES {
            for (message_id, arg_pairs) in cases.clone() {
                let args: HashMap<Cow<'static, str>, FluentValue> = arg_pairs
                    .into_iter()
                    .map(|(k, v)| (Cow::Borrowed(k), FluentValue::String(v.to_string().into())))
                    .collect();
                let rendered = render(message_id, locale, &args);
                assert_ne!(
                    rendered, message_id,
                    "Fluent message `{message_id}` missing from {locale}/notifications.ftl"
                );
            }
        }
    }
}
