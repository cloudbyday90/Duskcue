// Duskcue — Self-hosted media streaming server
// Copyright (C) 2026-2026 Duskcue Contributors
// Licensed under AGPL-3.0. See LICENSE for details.

use super::*;

#[test]
fn context_bound_secrets_reject_substitution_and_legacy_values() {
    let (key, _) = EncryptionKey::generate();
    let context = RecordContext {
        namespace: "live-tv-assignment",
        owner: Uuid::now_v7(),
        record: Uuid::now_v7(),
        field: "playlist",
    };
    let value = "https://example.invalid/feed?password=private";
    let sealed = key.seal_record(value, context).unwrap();
    assert_eq!(key.open_record(&sealed, context).unwrap(), value);
    assert_ne!(key.seal_record(value, context).unwrap(), sealed);
    for altered in [
        RecordContext {
            namespace: "other",
            ..context
        },
        RecordContext {
            owner: Uuid::now_v7(),
            ..context
        },
        RecordContext {
            record: Uuid::now_v7(),
            ..context
        },
        RecordContext {
            field: "guide",
            ..context
        },
    ] {
        assert!(key.open_record(&sealed, altered).is_err());
    }
    let (other, _) = EncryptionKey::generate();
    assert!(other.open_record(&sealed, context).is_err());
    for invalid in [
        value.to_string(),
        key.encrypt(value).unwrap(),
        sealed.replacen("v1:", "v2:", 1),
        "duskcue-secret:v1:bad".into(),
        "x".repeat(MAX_RECORD_ENVELOPE_BYTES + 1),
    ] {
        assert!(key.open_record(&invalid, context).is_err());
    }
    let bytes = BASE64
        .decode(sealed.strip_prefix(RECORD_SECRET_PREFIX).unwrap())
        .unwrap();
    for index in [0, NONCE_LEN, bytes.len() - 1] {
        let mut corrupted = bytes.clone();
        corrupted[index] ^= 1;
        assert!(
            key.open_record(
                &format!("{RECORD_SECRET_PREFIX}{}", BASE64.encode(corrupted)),
                context
            )
            .is_err()
        );
    }
    let max = "x".repeat(MAX_RECORD_SECRET_BYTES);
    let envelope = key.seal_record(&max, context).unwrap();
    assert_eq!(envelope.len(), MAX_RECORD_ENVELOPE_BYTES);
    assert_eq!(key.open_record(&envelope, context).unwrap(), max);
    assert!(key.seal_record(&(max + "x"), context).is_err());
    assert!(key.seal_record("", context).is_err());
    assert!(
        key.seal_record(
            value,
            RecordContext {
                namespace: "",
                ..context
            }
        )
        .is_err()
    );
    assert!(
        key.seal_record(
            value,
            RecordContext {
                field: &"x".repeat(129),
                ..context
            }
        )
        .is_err()
    );
    let a = RecordContext {
        namespace: "ab",
        field: "c",
        ..context
    };
    let b = RecordContext {
        namespace: "a",
        field: "bc",
        ..context
    };
    assert_ne!(a.associated_data().unwrap(), b.associated_data().unwrap());
    let mut invalid_utf8 = vec![0xff];
    let nonce = [0x42; NONCE_LEN];
    key.key
        .seal_in_place_append_tag(
            aead::Nonce::assume_unique_for_key(nonce),
            aead::Aad::from(context.associated_data().unwrap()),
            &mut invalid_utf8,
        )
        .unwrap();
    let mut payload = nonce.to_vec();
    payload.extend(invalid_utf8);
    assert!(matches!(
        key.open_record(
            &format!("{RECORD_SECRET_PREFIX}{}", BASE64.encode(payload)),
            context
        ),
        Err(EncryptionError::InvalidUtf8)
    ));
}
