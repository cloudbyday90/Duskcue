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

use base64::Engine as _;
use base64::engine::general_purpose::STANDARD as BASE64;
use ring::rand::SecureRandom as _;

pub const ENCRYPTED_PREFIX: &str = "encrypted:";
const KEY_LEN: usize = 32;
const NONCE_LEN: usize = 12;
const TAG_LEN: usize = 16;

#[derive(Debug, thiserror::Error)]
pub enum EncryptionError {
    #[error("Invalid hex encoding: {0}")]
    InvalidHex(String),
    #[error("Encryption key must be exactly 32 bytes (64 hex characters)")]
    InvalidKeyLength,
    #[error("Failed to generate random bytes")]
    RandomGenerationFailed,
    #[error("Encryption failed")]
    EncryptionFailed,
    #[error("Value does not have the encrypted: prefix")]
    InvalidFormat,
    #[error("Invalid base64 encoding")]
    InvalidBase64,
    #[error("Ciphertext is too short")]
    InvalidCiphertextLength,
    #[error("Decryption failed — data may be corrupted or key is wrong")]
    DecryptionFailed,
    #[error("Decrypted value is not valid UTF-8")]
    InvalidUtf8,
    #[error("Failed to write encryption key to config file: {0}")]
    ConfigWriteFailed(String),
}

fn hex_decode(hex: &str) -> Result<Vec<u8>, EncryptionError> {
    if hex.len() != KEY_LEN * 2 {
        return Err(EncryptionError::InvalidHex(format!(
            "expected {} hex characters, got {}",
            KEY_LEN * 2,
            hex.len()
        )));
    }
    let mut bytes = vec![0u8; KEY_LEN];
    for i in 0..KEY_LEN {
        bytes[i] = u8::from_str_radix(&hex[i * 2..i * 2 + 2], 16)
            .map_err(|e| EncryptionError::InvalidHex(e.to_string()))?;
    }
    Ok(bytes)
}

fn hex_encode(bytes: &[u8]) -> String {
    bytes.iter().map(|b| format!("{:02x}", b)).collect()
}

#[derive(Clone)]
pub struct EncryptionKey {
    key: ring::aead::LessSafeKey,
}

impl EncryptionKey {
    pub fn from_hex(hex: &str) -> Result<Self, EncryptionError> {
        let bytes = hex_decode(hex)?;
        let unbound = ring::aead::UnboundKey::new(&ring::aead::AES_256_GCM, &bytes)
            .map_err(|_| EncryptionError::InvalidKeyLength)?;
        Ok(Self {
            key: ring::aead::LessSafeKey::new(unbound),
        })
    }

    pub fn generate() -> (Self, String) {
        let mut key_bytes = [0u8; KEY_LEN];
        ring::rand::SystemRandom::new()
            .fill(&mut key_bytes)
            .expect("failed to generate random encryption key");
        let hex = hex_encode(&key_bytes);
        let sk = Self::from_hex(&hex).expect("generated key should be valid");
        (sk, hex)
    }

    pub fn encrypt(&self, plaintext: &str) -> Result<String, EncryptionError> {
        let mut nonce_bytes = [0u8; NONCE_LEN];
        ring::rand::SystemRandom::new()
            .fill(&mut nonce_bytes)
            .map_err(|_| EncryptionError::RandomGenerationFailed)?;
        let nonce = ring::aead::Nonce::assume_unique_for_key(nonce_bytes);

        let mut in_out = plaintext.as_bytes().to_vec();
        self.key
            .seal_in_place_append_tag(nonce, ring::aead::Aad::empty(), &mut in_out)
            .map_err(|_| EncryptionError::EncryptionFailed)?;

        let mut combined = Vec::with_capacity(NONCE_LEN + in_out.len());
        combined.extend_from_slice(&nonce_bytes);
        combined.extend_from_slice(&in_out);

        Ok(format!("{}{}", ENCRYPTED_PREFIX, BASE64.encode(&combined)))
    }

    pub fn decrypt(&self, ciphertext: &str) -> Result<String, EncryptionError> {
        let encoded = ciphertext
            .strip_prefix(ENCRYPTED_PREFIX)
            .ok_or(EncryptionError::InvalidFormat)?;
        let combined = BASE64
            .decode(encoded)
            .map_err(|_| EncryptionError::InvalidBase64)?;

        if combined.len() < NONCE_LEN + TAG_LEN {
            return Err(EncryptionError::InvalidCiphertextLength);
        }

        let nonce_bytes: [u8; NONCE_LEN] = combined[..NONCE_LEN]
            .try_into()
            .map_err(|_| EncryptionError::InvalidCiphertextLength)?;
        let nonce = ring::aead::Nonce::assume_unique_for_key(nonce_bytes);

        let mut in_out = combined[NONCE_LEN..].to_vec();
        let plaintext = self
            .key
            .open_in_place(nonce, ring::aead::Aad::empty(), &mut in_out)
            .map_err(|_| EncryptionError::DecryptionFailed)?;

        String::from_utf8(plaintext.to_vec()).map_err(|_| EncryptionError::InvalidUtf8)
    }

    pub fn decrypt_if_encrypted(&self, value: &str) -> String {
        if value.starts_with(ENCRYPTED_PREFIX) {
            self.decrypt(value).unwrap_or_else(|e| {
                tracing::error!(error = %e, "Failed to decrypt encrypted value");
                String::new()
            })
        } else {
            value.to_string()
        }
    }

    pub fn decrypt_optional(&self, value: &Option<String>) -> Option<String> {
        value.as_ref().map(|v| self.decrypt_if_encrypted(v))
    }
}

pub fn mask_secret(value: &str) -> String {
    if value.is_empty() {
        return String::new();
    }
    if value.starts_with(ENCRYPTED_PREFIX) {
        return "***encrypted***".to_string();
    }
    if value.len() <= 6 {
        return "***".to_string();
    }
    format!("{}...{}", &value[..3], &value[value.len() - 3..])
}

pub fn decrypt_provider_config(config: &mut crate::state::ProviderConfig, key: &EncryptionKey) {
    config.tmdb.api_key = key.decrypt_if_encrypted(&config.tmdb.api_key);
    config.tmdb.access_token = key.decrypt_if_encrypted(&config.tmdb.access_token);
    config.tvdb.api_key = key.decrypt_optional(&config.tvdb.api_key);
    config.fanart.api_key = key.decrypt_optional(&config.fanart.api_key);
    config.omdb.api_key = key.decrypt_optional(&config.omdb.api_key);
}

pub fn encrypt_provider_config(config: &mut crate::state::ProviderConfig, key: &EncryptionKey) {
    if !config.tmdb.api_key.is_empty() && !config.tmdb.api_key.starts_with(ENCRYPTED_PREFIX) {
        match key.encrypt(&config.tmdb.api_key) {
            Ok(encrypted) => config.tmdb.api_key = encrypted,
            Err(e) => tracing::error!(error = %e, "Failed to encrypt TMDB api_key"),
        }
    }
    if !config.tmdb.access_token.is_empty()
        && !config.tmdb.access_token.starts_with(ENCRYPTED_PREFIX)
    {
        match key.encrypt(&config.tmdb.access_token) {
            Ok(encrypted) => config.tmdb.access_token = encrypted,
            Err(e) => tracing::error!(error = %e, "Failed to encrypt TMDB access_token"),
        }
    }
    if let Some(ref api_key) = config.tvdb.api_key
        && !api_key.is_empty()
        && !api_key.starts_with(ENCRYPTED_PREFIX)
    {
        match key.encrypt(api_key) {
            Ok(encrypted) => config.tvdb.api_key = Some(encrypted),
            Err(e) => tracing::error!(error = %e, "Failed to encrypt TVDB api_key"),
        }
    }
    if let Some(ref api_key) = config.fanart.api_key
        && !api_key.is_empty()
        && !api_key.starts_with(ENCRYPTED_PREFIX)
    {
        match key.encrypt(api_key) {
            Ok(encrypted) => config.fanart.api_key = Some(encrypted),
            Err(e) => tracing::error!(error = %e, "Failed to encrypt Fanart api_key"),
        }
    }
    if let Some(ref api_key) = config.omdb.api_key
        && !api_key.is_empty()
        && !api_key.starts_with(ENCRYPTED_PREFIX)
    {
        match key.encrypt(api_key) {
            Ok(encrypted) => config.omdb.api_key = Some(encrypted),
            Err(e) => tracing::error!(error = %e, "Failed to encrypt OMDb api_key"),
        }
    }
}

pub fn decrypt_trakt_config(config: &mut crate::state::TraktConfig, key: &EncryptionKey) {
    config.client_secret = key.decrypt_if_encrypted(&config.client_secret);
}

pub fn encrypt_trakt_config(config: &mut crate::state::TraktConfig, key: &EncryptionKey) {
    if !config.client_secret.is_empty() && !config.client_secret.starts_with(ENCRYPTED_PREFIX) {
        match key.encrypt(&config.client_secret) {
            Ok(encrypted) => config.client_secret = encrypted,
            Err(e) => tracing::error!(error = %e, "Failed to encrypt Trakt client_secret"),
        }
    }
}

pub fn decrypt_notification_config(
    config: &mut crate::state::NotificationConfig,
    key: &EncryptionKey,
) {
    config.webhook.secret = key.decrypt_optional(&config.webhook.secret);
}

pub fn encrypt_notification_config(
    config: &mut crate::state::NotificationConfig,
    key: &EncryptionKey,
) {
    if let Some(ref secret) = config.webhook.secret
        && !secret.is_empty()
        && !secret.starts_with(ENCRYPTED_PREFIX)
    {
        match key.encrypt(secret) {
            Ok(encrypted) => config.webhook.secret = Some(encrypted),
            Err(e) => tracing::error!(error = %e, "Failed to encrypt webhook secret"),
        }
    }
}

pub fn decrypt_subtitle_provider_config(
    config: &mut crate::state::SubtitleProviderConfig,
    key: &EncryptionKey,
) {
    config.subdl.api_key = key.decrypt_optional(&config.subdl.api_key);
    config.opensubtitles.api_key = key.decrypt_optional(&config.opensubtitles.api_key);
    config.opensubtitles.api_token = key.decrypt_optional(&config.opensubtitles.api_token);
}

pub fn encrypt_subtitle_provider_config(
    config: &mut crate::state::SubtitleProviderConfig,
    key: &EncryptionKey,
) {
    if let Some(ref api_key) = config.subdl.api_key
        && !api_key.is_empty()
        && !api_key.starts_with(ENCRYPTED_PREFIX)
    {
        match key.encrypt(api_key) {
            Ok(encrypted) => config.subdl.api_key = Some(encrypted),
            Err(e) => tracing::error!(error = %e, "Failed to encrypt SubDL api_key"),
        }
    }
    if let Some(ref api_key) = config.opensubtitles.api_key
        && !api_key.is_empty()
        && !api_key.starts_with(ENCRYPTED_PREFIX)
    {
        match key.encrypt(api_key) {
            Ok(encrypted) => config.opensubtitles.api_key = Some(encrypted),
            Err(e) => tracing::error!(error = %e, "Failed to encrypt OpenSubtitles api_key"),
        }
    }
    if let Some(ref token) = config.opensubtitles.api_token
        && !token.is_empty()
        && !token.starts_with(ENCRYPTED_PREFIX)
    {
        match key.encrypt(token) {
            Ok(encrypted) => config.opensubtitles.api_token = Some(encrypted),
            Err(e) => tracing::error!(error = %e, "Failed to encrypt OpenSubtitles api_token"),
        }
    }
}

pub fn ensure_encryption_key(
    bootstrap: &crate::config::BootstrapConfig,
) -> Result<(EncryptionKey, Option<String>), EncryptionError> {
    if let Some(ref hex) = bootstrap.encryption_key {
        let key = EncryptionKey::from_hex(hex)?;
        Ok((key, None))
    } else {
        let (key, hex) = EncryptionKey::generate();
        tracing::info!("Generated new encryption key — writing to config file");
        write_encryption_key_to_config(bootstrap, &hex)?;
        Ok((key, Some(hex)))
    }
}

fn write_encryption_key_to_config(
    bootstrap: &crate::config::BootstrapConfig,
    hex: &str,
) -> Result<(), EncryptionError> {
    let config_dir = bootstrap.data_dir.join("config");
    std::fs::create_dir_all(&config_dir)
        .map_err(|e| EncryptionError::ConfigWriteFailed(e.to_string()))?;

    let config_path = config_dir.join("config.toml");

    if config_path.exists() {
        let existing = std::fs::read_to_string(&config_path)
            .map_err(|e| EncryptionError::ConfigWriteFailed(e.to_string()))?;
        if existing.contains("encryption_key") {
            tracing::warn!(
                "encryption_key already exists in config file but was not loaded — possible config mismatch"
            );
            return Ok(());
        }
        let updated = format!("{}\nencryption_key = \"{}\"\n", existing.trim_end(), hex);
        std::fs::write(&config_path, updated)
            .map_err(|e| EncryptionError::ConfigWriteFailed(e.to_string()))?;
    } else {
        let content = format!("encryption_key = \"{}\"\n", hex);
        std::fs::write(&config_path, content)
            .map_err(|e| EncryptionError::ConfigWriteFailed(e.to_string()))?;
    }

    tracing::info!(
        path = %config_path.display(),
        "Encryption key written to config file"
    );
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_roundtrip_short_value() {
        let (key, _) = EncryptionKey::generate();
        let plaintext = "hello world";
        let encrypted = key.encrypt(plaintext).unwrap();
        assert!(encrypted.starts_with(ENCRYPTED_PREFIX));
        let decrypted = key.decrypt(&encrypted).unwrap();
        assert_eq!(decrypted, plaintext);
    }

    #[test]
    fn test_roundtrip_api_key_length() {
        let (key, _) = EncryptionKey::generate();
        let plaintext = "eyJhbGciOiJIUzI1NiIsInR5cCI6IkpXVCJ9.eyJzdWIiOiIxMjM0NTY3ODkwIn0.dozjgNryP4J3jVmNHl0w5N_XgL0n3I9PlFUP0THsR8U";
        let encrypted = key.encrypt(plaintext).unwrap();
        assert!(encrypted.starts_with(ENCRYPTED_PREFIX));
        assert_ne!(&encrypted[ENCRYPTED_PREFIX.len()..], plaintext);
        let decrypted = key.decrypt(&encrypted).unwrap();
        assert_eq!(decrypted, plaintext);
    }

    #[test]
    fn test_roundtrip_empty_value() {
        let (key, _) = EncryptionKey::generate();
        let plaintext = "";
        let encrypted = key.encrypt(plaintext).unwrap();
        let decrypted = key.decrypt(&encrypted).unwrap();
        assert_eq!(decrypted, plaintext);
    }

    #[test]
    fn test_different_nonces_per_encryption() {
        let (key, _) = EncryptionKey::generate();
        let plaintext = "same value";
        let enc1 = key.encrypt(plaintext).unwrap();
        let enc2 = key.encrypt(plaintext).unwrap();
        assert_ne!(enc1, enc2);
        assert_eq!(key.decrypt(&enc1).unwrap(), plaintext);
        assert_eq!(key.decrypt(&enc2).unwrap(), plaintext);
    }

    #[test]
    fn test_decrypt_if_encrypted_with_prefix() {
        let (key, _) = EncryptionKey::generate();
        let plaintext = "my-secret-key";
        let encrypted = key.encrypt(plaintext).unwrap();
        assert_eq!(key.decrypt_if_encrypted(&encrypted), plaintext);
    }

    #[test]
    fn test_decrypt_if_encrypted_without_prefix() {
        let (key, _) = EncryptionKey::generate();
        let plaintext = "my-plaintext-key";
        assert_eq!(key.decrypt_if_encrypted(plaintext), plaintext);
    }

    #[test]
    fn test_decrypt_optional() {
        let (key, _) = EncryptionKey::generate();
        let encrypted = key.encrypt("secret").unwrap();
        assert_eq!(
            key.decrypt_optional(&Some(encrypted)),
            Some("secret".to_string())
        );
        assert_eq!(
            key.decrypt_optional(&Some("plaintext".to_string())),
            Some("plaintext".to_string())
        );
        assert_eq!(key.decrypt_optional(&None), None);
    }

    #[test]
    fn test_mask_secret_encrypted() {
        assert_eq!(mask_secret("encrypted:abc123"), "***encrypted***");
    }

    #[test]
    fn test_mask_secret_short() {
        assert_eq!(mask_secret("abc"), "***");
    }

    #[test]
    fn test_mask_secret_empty() {
        assert_eq!(mask_secret(""), "");
    }

    #[test]
    fn test_mask_secret_long() {
        assert_eq!(mask_secret("abcdefghijklmnop"), "abc...nop");
    }

    #[test]
    fn test_from_hex_valid() {
        let hex = "0123456789abcdef0123456789abcdef0123456789abcdef0123456789abcdef";
        let key = EncryptionKey::from_hex(hex).unwrap();
        let plaintext = "test";
        let encrypted = key.encrypt(plaintext).unwrap();
        assert_eq!(key.decrypt(&encrypted).unwrap(), plaintext);
    }

    #[test]
    fn test_from_hex_wrong_length() {
        let hex = "0123456789abcdef";
        assert!(EncryptionKey::from_hex(hex).is_err());
    }

    #[test]
    fn test_from_hex_invalid_chars() {
        let hex = "ZZZZZZZZZZZZZZZZZZZZZZZZZZZZZZZZZZZZZZZZZZZZZZZZZZZZZZZZZZZZZZZZ";
        assert!(EncryptionKey::from_hex(hex).is_err());
    }

    #[test]
    fn test_decrypt_tampered_ciphertext() {
        let (key, _) = EncryptionKey::generate();
        let encrypted = key.encrypt("secret").unwrap();
        let prefix_len = ENCRYPTED_PREFIX.len();
        let mut bytes: Vec<u8> = encrypted.as_bytes()[prefix_len..].to_vec();
        if let Some(last_byte) = bytes.last_mut() {
            *last_byte = last_byte.wrapping_add(1);
        }
        let tampered = format!(
            "{}{}",
            ENCRYPTED_PREFIX,
            String::from_utf8(bytes).unwrap_or_default()
        );
        assert!(key.decrypt(&tampered).is_err());
    }

    #[test]
    fn test_decrypt_wrong_key() {
        let (key1, _) = EncryptionKey::generate();
        let (key2, _) = EncryptionKey::generate();
        let encrypted = key1.encrypt("secret").unwrap();
        assert!(key2.decrypt(&encrypted).is_err());
    }

    #[test]
    fn test_encrypt_provider_config_skip_already_encrypted() {
        let (key, _) = EncryptionKey::generate();
        let mut config = crate::state::ProviderConfig::default();
        config.tmdb.access_token = key.encrypt("test-token").unwrap();
        let already_encrypted = config.tmdb.access_token.clone();
        encrypt_provider_config(&mut config, &key);
        assert_eq!(config.tmdb.access_token, already_encrypted);
    }

    #[test]
    fn test_encrypt_decrypt_provider_config_roundtrip() {
        let (key, _) = EncryptionKey::generate();
        let mut config = crate::state::ProviderConfig {
            tmdb: crate::state::TmdbProviderConfig {
                api_key: "tmdb-api-key".to_string(),
                access_token: "tmdb-access-token".to_string(),
                enabled: true,
                include_adult: false,
            },
            tvdb: crate::state::OptionalProviderConfig {
                api_key: Some("tvdb-api-key".to_string()),
                enabled: true,
            },
            fanart: crate::state::OptionalProviderConfig {
                api_key: Some("fanart-api-key".to_string()),
                enabled: false,
            },
            omdb: crate::state::OptionalProviderConfig {
                api_key: None,
                enabled: false,
            },
        };

        encrypt_provider_config(&mut config, &key);

        assert!(config.tmdb.api_key.starts_with(ENCRYPTED_PREFIX));
        assert!(config.tmdb.access_token.starts_with(ENCRYPTED_PREFIX));
        assert!(
            config
                .tvdb
                .api_key
                .as_ref()
                .unwrap()
                .starts_with(ENCRYPTED_PREFIX)
        );
        assert!(
            config
                .fanart
                .api_key
                .as_ref()
                .unwrap()
                .starts_with(ENCRYPTED_PREFIX)
        );
        assert!(config.omdb.api_key.is_none());

        decrypt_provider_config(&mut config, &key);

        assert_eq!(config.tmdb.api_key, "tmdb-api-key");
        assert_eq!(config.tmdb.access_token, "tmdb-access-token");
        assert_eq!(config.tvdb.api_key, Some("tvdb-api-key".to_string()));
        assert_eq!(config.fanart.api_key, Some("fanart-api-key".to_string()));
        assert_eq!(config.omdb.api_key, None);
    }
}
