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

use thiserror::Error;

#[derive(Error, Debug)]
pub enum SubtitleError {
    #[error("subtitle file not found: {subtitle_id}")]
    FileNotFound { subtitle_id: uuid::Uuid },

    #[error("OCR engine unavailable (PaddleOCR and Tesseract both missing)")]
    OcrUnavailable,

    #[error("OCR confidence {confidence} below threshold {threshold}")]
    OcrLowConfidence { confidence: f64, threshold: f64 },

    #[error("subtitle provider unavailable: {provider}")]
    ProviderUnavailable { provider: String },

    #[error("subtitle provider rate limited: {provider}")]
    ProviderRateLimited { provider: String },

    #[error("voice activity analysis failed: {reason}")]
    VoiceAnalysisFailed { reason: String },

    #[error("media item not found: {media_item_id}")]
    MediaItemNotFound { media_item_id: uuid::Uuid },

    #[error("invalid subtitle format: {0}")]
    InvalidSubtitleFormat(String),

    #[error("invalid language code: {0}")]
    InvalidLanguageCode(String),

    #[error("invalid subtitle mode: {0}")]
    InvalidSubtitleMode(String),

    #[error("invalid OCR engine: {0}")]
    InvalidOcrEngine(String),

    #[error("subtitle fetch failed: {reason}")]
    FetchFailed { reason: String },

    #[error("subtitle conversion failed: {reason}")]
    ConversionFailed { reason: String },

    #[error("subtitle sync data not found for subtitle {subtitle_id}")]
    SyncDataNotFound { subtitle_id: uuid::Uuid },

    #[error(transparent)]
    Database(#[from] sqlx::Error),
}
