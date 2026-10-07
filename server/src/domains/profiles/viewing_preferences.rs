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

use sqlx::{PgPool, Row};
use uuid::Uuid;

use super::error::ProfilesError;
use super::service::lock_profile_session;
use super::types::*;

pub async fn get_viewing_preferences(
    pool: &PgPool,
    owner_user_id: Uuid,
    session_id: Uuid,
    authenticated_profile_id: Uuid,
) -> Result<CurrentViewingPreferencesResponse, ProfilesError> {
    let row = sqlx::query(
        "SELECT s.active_profile_id, s.profile_selection_required, p.metadata \
         FROM user_sessions s JOIN user_profiles p ON p.id = s.active_profile_id \
         WHERE s.id = $1 AND s.user_id = $2 AND p.owner_user_id = $2",
    )
    .bind(session_id)
    .bind(owner_user_id)
    .fetch_optional(pool)
    .await?
    .ok_or(ProfilesError::AccessDenied)?;
    assert_viewing_preference_scope(&row, authenticated_profile_id)?;
    viewing_preferences_response(authenticated_profile_id, row.get("metadata"))
}

pub async fn update_viewing_preferences(
    pool: &PgPool,
    owner_user_id: Uuid,
    session_id: Uuid,
    authenticated_profile_id: Uuid,
    req: UpdateViewingPreferencesRequest,
) -> Result<CurrentViewingPreferencesResponse, ProfilesError> {
    if req.expected_profile_id != authenticated_profile_id {
        return Err(ProfilesError::ActiveProfileChanged);
    }
    let viewing_preferences = normalize_viewing_preferences(req.viewing_preferences)?;
    let value = serde_json::to_value(&viewing_preferences)
        .map_err(|_| ProfilesError::InvalidStoredViewingPreferences)?;
    let mut transaction = pool.begin().await?;
    let (profile, session) = lock_profile_session(
        &mut transaction,
        owner_user_id,
        session_id,
        req.expected_profile_id,
    )
    .await?;
    assert_viewing_preference_scope(&session, req.expected_profile_id)?;
    let metadata: serde_json::Value = profile.get("metadata");
    if !metadata.is_object() {
        return Err(ProfilesError::InvalidStoredViewingPreferences);
    }
    sqlx::query(
        "UPDATE user_profiles SET metadata = jsonb_set(metadata, '{viewing_preferences}', $3, true), \
         updated_at = now() WHERE id = $1 AND owner_user_id = $2",
    )
    .bind(req.expected_profile_id)
    .bind(owner_user_id)
    .bind(value)
    .execute(&mut *transaction)
    .await?;
    transaction.commit().await?;
    Ok(CurrentViewingPreferencesResponse {
        profile_id: req.expected_profile_id,
        has_saved_preferences: true,
        viewing_preferences,
    })
}

fn assert_viewing_preference_scope(
    session: &sqlx::postgres::PgRow,
    expected_profile_id: Uuid,
) -> Result<(), ProfilesError> {
    if session.get::<bool, _>("profile_selection_required") {
        return Err(ProfilesError::SelectionRequired);
    }
    if session.get::<Uuid, _>("active_profile_id") != expected_profile_id {
        return Err(ProfilesError::ActiveProfileChanged);
    }
    Ok(())
}

fn viewing_preferences_response(
    profile_id: Uuid,
    metadata: serde_json::Value,
) -> Result<CurrentViewingPreferencesResponse, ProfilesError> {
    if !metadata.is_object() {
        return Err(ProfilesError::InvalidStoredViewingPreferences);
    }
    let saved = metadata.get("viewing_preferences");
    let viewing_preferences = match saved {
        Some(value) => {
            let request = serde_json::from_value(value.clone())
                .map_err(|_| ProfilesError::InvalidStoredViewingPreferences)?;
            normalize_viewing_preferences(request)
                .map_err(|_| ProfilesError::InvalidStoredViewingPreferences)?
        }
        None => ViewingPreferencesResponse::default(),
    };
    Ok(CurrentViewingPreferencesResponse {
        profile_id,
        has_saved_preferences: saved.is_some(),
        viewing_preferences,
    })
}

fn normalize_viewing_preferences(
    req: ViewingPreferencesRequest,
) -> Result<ViewingPreferencesResponse, ProfilesError> {
    let audio_language = req
        .audio_language
        .as_deref()
        .map(canonical_viewing_language)
        .transpose()?;
    let subtitle_language = req
        .subtitle_language
        .as_deref()
        .map(canonical_viewing_language)
        .transpose()?;
    if req.subtitle_mode == SubtitleMode::Always && subtitle_language.is_none() {
        return Err(ProfilesError::InvalidViewingPreferences(
            "subtitle_language is required when subtitle_mode is always".into(),
        ));
    }
    Ok(ViewingPreferencesResponse {
        autoplay_next_episode: req.autoplay_next_episode,
        audio_language,
        prefer_audio_description: req.prefer_audio_description,
        subtitle_mode: req.subtitle_mode,
        subtitle_language,
        prefer_sdh: req.prefer_sdh,
    })
}

pub fn canonical_viewing_language(value: &str) -> Result<String, ProfilesError> {
    if value.len() > 32 {
        return Err(ProfilesError::InvalidViewingPreferences(
            "language code is too long".into(),
        ));
    }
    let value = value.trim().to_ascii_lowercase();
    if !(2..=3).contains(&value.len())
        || !value.bytes().all(|byte| byte.is_ascii_lowercase())
        || matches!(value.as_str(), "und" | "mul" | "zxx")
    {
        return Err(ProfilesError::InvalidViewingPreferences(
            "language must be a two- or three-letter code".into(),
        ));
    }
    let canonical = match value.as_str() {
        "ara" => "ar",
        "ben" => "bn",
        "bul" => "bg",
        "cat" => "ca",
        "ces" | "cze" => "cs",
        "chi" | "zho" => "zh",
        "dan" => "da",
        "deu" | "ger" => "de",
        "dut" | "nld" => "nl",
        "ell" | "gre" => "el",
        "eng" => "en",
        "est" => "et",
        "fas" | "per" => "fa",
        "fin" => "fi",
        "fra" | "fre" => "fr",
        "heb" => "he",
        "hin" => "hi",
        "hrv" => "hr",
        "hun" => "hu",
        "ice" | "isl" => "is",
        "ind" => "id",
        "ita" => "it",
        "jpn" => "ja",
        "kor" => "ko",
        "lav" => "lv",
        "lit" => "lt",
        "may" | "msa" => "ms",
        "nob" => "nb",
        "nno" => "nn",
        "nor" => "no",
        "pol" => "pl",
        "por" => "pt",
        "ron" | "rum" => "ro",
        "rus" => "ru",
        "slk" | "slo" => "sk",
        "slv" => "sl",
        "spa" => "es",
        "srp" => "sr",
        "swe" => "sv",
        "tam" => "ta",
        "tel" => "te",
        "tha" => "th",
        "tur" => "tr",
        "ukr" => "uk",
        "urd" => "ur",
        "vie" => "vi",
        _ => &value,
    };
    Ok(canonical.to_string())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn preference_request() -> ViewingPreferencesRequest {
        ViewingPreferencesRequest {
            autoplay_next_episode: true,
            audio_language: None,
            prefer_audio_description: false,
            subtitle_mode: SubtitleMode::None,
            subtitle_language: None,
            prefer_sdh: false,
        }
    }

    #[test]
    fn viewing_preferences_distinguish_unsaved_from_explicit_default_values() {
        let profile_id = Uuid::now_v7();
        let unsaved = viewing_preferences_response(profile_id, serde_json::json!({})).unwrap();
        assert!(!unsaved.has_saved_preferences);
        assert_eq!(
            unsaved.viewing_preferences,
            ViewingPreferencesResponse::default()
        );
        let saved = viewing_preferences_response(
            profile_id,
            serde_json::json!({
                "viewing_preferences": ViewingPreferencesResponse::default()
            }),
        )
        .unwrap();
        assert!(saved.has_saved_preferences);
        assert_eq!(saved.viewing_preferences, unsaved.viewing_preferences);
    }

    #[test]
    fn saved_viewing_preferences_preserve_off_and_normalize_language_aliases() {
        let mut request = preference_request();
        request.autoplay_next_episode = false;
        request.audio_language = Some(" ENG ".into());
        request.subtitle_mode = SubtitleMode::Always;
        request.subtitle_language = Some("FRE".into());
        request.prefer_sdh = true;
        let normalized = normalize_viewing_preferences(request).unwrap();
        assert!(!normalized.autoplay_next_episode);
        assert_eq!(normalized.audio_language.as_deref(), Some("en"));
        assert_eq!(normalized.subtitle_language.as_deref(), Some("fr"));
        assert!(normalized.prefer_sdh);
    }

    #[test]
    fn viewing_languages_reject_display_names_locales_and_undefined_tags() {
        for invalid in ["", "English", "en-US", "en_US", "und", "mul", "zxx", "éé"] {
            assert!(canonical_viewing_language(invalid).is_err(), "{invalid}");
        }
        for (input, canonical) in [
            ("deu", "de"),
            ("ger", "de"),
            ("fra", "fr"),
            ("CHI", "zh"),
            ("JA", "ja"),
            ("fil", "fil"),
        ] {
            assert_eq!(canonical_viewing_language(input).unwrap(), canonical);
        }
    }

    #[test]
    fn always_subtitles_require_a_language_but_off_can_retain_one() {
        let mut request = preference_request();
        request.subtitle_mode = SubtitleMode::Always;
        assert!(normalize_viewing_preferences(request.clone()).is_err());
        request.subtitle_language = Some("spa".into());
        assert!(normalize_viewing_preferences(request.clone()).is_ok());
        request.subtitle_mode = SubtitleMode::None;
        let normalized = normalize_viewing_preferences(request).unwrap();
        assert_eq!(normalized.subtitle_language.as_deref(), Some("es"));
    }

    #[test]
    fn malformed_saved_preferences_do_not_masquerade_as_unsaved_defaults() {
        for metadata in [
            serde_json::json!([]),
            serde_json::json!({"viewing_preferences": null}),
            serde_json::json!({"viewing_preferences": {"autoplay_next_episode": "false"}}),
        ] {
            assert!(matches!(
                viewing_preferences_response(Uuid::now_v7(), metadata),
                Err(ProfilesError::InvalidStoredViewingPreferences)
            ));
        }
    }
}
