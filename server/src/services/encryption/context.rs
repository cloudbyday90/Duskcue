// Duskcue — Self-hosted media streaming server
// Copyright (C) 2026-2026 Duskcue Contributors
// Licensed under AGPL-3.0. See LICENSE for details.

use super::{BASE64, EncryptionError, EncryptionKey, NONCE_LEN, TAG_LEN};
use base64::Engine;
use ring::{aead, rand::SecureRandom};
use uuid::Uuid;

pub const RECORD_SECRET_PREFIX: &str = "duskcue-secret:v1:";
pub const MAX_RECORD_SECRET_BYTES: usize = 4096;
pub const MAX_RECORD_ENVELOPE_BYTES: usize = 5518;

#[derive(Clone, Copy)]
pub struct RecordContext<'a> {
    pub namespace: &'a str,
    pub owner: Uuid,
    pub record: Uuid,
    pub field: &'a str,
}

impl RecordContext<'_> {
    fn associated_data(self) -> Result<Vec<u8>, EncryptionError> {
        let mut data = b"duskcue-record-secret\0\x01".to_vec();
        for value in [self.namespace, self.field] {
            if value.is_empty() || value.len() > 128 {
                return Err(EncryptionError::InvalidFormat);
            }
            data.extend_from_slice(&(value.len() as u16).to_be_bytes());
            data.extend_from_slice(value.as_bytes());
        }
        data.extend_from_slice(self.owner.as_bytes());
        data.extend_from_slice(self.record.as_bytes());
        Ok(data)
    }
}

impl EncryptionKey {
    pub fn seal_record(
        &self,
        value: &str,
        context: RecordContext<'_>,
    ) -> Result<String, EncryptionError> {
        if value.is_empty() || value.len() > MAX_RECORD_SECRET_BYTES {
            return Err(EncryptionError::InvalidFormat);
        }
        let aad = context.associated_data()?;
        let mut nonce = [0; NONCE_LEN];
        ring::rand::SystemRandom::new()
            .fill(&mut nonce)
            .map_err(|_| EncryptionError::RandomGenerationFailed)?;
        let mut bytes = value.as_bytes().to_vec();
        self.key
            .seal_in_place_append_tag(
                aead::Nonce::assume_unique_for_key(nonce),
                aead::Aad::from(aad),
                &mut bytes,
            )
            .map_err(|_| EncryptionError::EncryptionFailed)?;
        let mut envelope = nonce.to_vec();
        envelope.extend_from_slice(&bytes);
        Ok(format!("{RECORD_SECRET_PREFIX}{}", BASE64.encode(envelope)))
    }

    pub fn open_record(
        &self,
        value: &str,
        context: RecordContext<'_>,
    ) -> Result<String, EncryptionError> {
        if value.len() > MAX_RECORD_ENVELOPE_BYTES {
            return Err(EncryptionError::InvalidFormat);
        }
        let encoded = value
            .strip_prefix(RECORD_SECRET_PREFIX)
            .ok_or(EncryptionError::InvalidFormat)?;
        let mut bytes = BASE64
            .decode(encoded)
            .map_err(|_| EncryptionError::InvalidBase64)?;
        if bytes.len() <= NONCE_LEN + TAG_LEN
            || bytes.len() > NONCE_LEN + TAG_LEN + MAX_RECORD_SECRET_BYTES
        {
            return Err(EncryptionError::InvalidCiphertextLength);
        }
        let nonce = bytes[..NONCE_LEN]
            .try_into()
            .map_err(|_| EncryptionError::InvalidCiphertextLength)?;
        let plaintext = self
            .key
            .open_in_place(
                aead::Nonce::assume_unique_for_key(nonce),
                aead::Aad::from(context.associated_data()?),
                &mut bytes[NONCE_LEN..],
            )
            .map_err(|_| EncryptionError::DecryptionFailed)?;
        String::from_utf8(plaintext.to_vec()).map_err(|_| EncryptionError::InvalidUtf8)
    }
}

#[cfg(test)]
mod tests;
