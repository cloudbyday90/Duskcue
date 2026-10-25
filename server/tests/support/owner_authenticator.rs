// Duskcue — Self-hosted media streaming server
// Copyright (C) 2026-2026 Duskcue Contributors
// Licensed under AGPL-3.0. See LICENSE for details.

use base64::{Engine, engine::general_purpose::URL_SAFE_NO_PAD};
use ring::{
    digest::{SHA256, digest},
    rand::{SecureRandom, SystemRandom},
    signature::{ECDSA_P256_SHA256_ASN1_SIGNING, EcdsaKeyPair, KeyPair},
};
use serde_json::{Value, json};
use uuid::Uuid;

pub struct Authenticator {
    key: EcdsaKeyPair,
    pub id: Vec<u8>,
    pub counter: u32,
}

fn encoded(bytes: impl AsRef<[u8]>) -> String {
    URL_SAFE_NO_PAD.encode(bytes)
}

impl Authenticator {
    pub fn new() -> anyhow::Result<Self> {
        let rng = SystemRandom::new();
        let pkcs8 = EcdsaKeyPair::generate_pkcs8(&ECDSA_P256_SHA256_ASN1_SIGNING, &rng)
            .map_err(|_| anyhow::anyhow!("fixture key generation failed"))?;
        let key = EcdsaKeyPair::from_pkcs8(&ECDSA_P256_SHA256_ASN1_SIGNING, pkcs8.as_ref(), &rng)
            .map_err(|_| anyhow::anyhow!("fixture key loading failed"))?;
        let mut id = vec![0; 32];
        rng.fill(&mut id)
            .map_err(|_| anyhow::anyhow!("fixture identity failed"))?;
        Ok(Self {
            key,
            id,
            counter: 0,
        })
    }

    pub fn registration(&self, options: &Value) -> anyhow::Result<Value> {
        let client = serde_json::to_vec(
            &json!({"type":"webauthn.create","challenge":options["publicKey"]["challenge"],"origin":"http://localhost:48027","crossOrigin":false}),
        )?;
        let key = self.key.public_key().as_ref();
        let mut cose = vec![0xa5, 0x01, 0x02, 0x03, 0x26, 0x20, 0x01, 0x21, 0x58, 0x20];
        cose.extend_from_slice(&key[1..33]);
        cose.extend_from_slice(&[0x22, 0x58, 0x20]);
        cose.extend_from_slice(&key[33..65]);
        let mut auth = digest(&SHA256, b"localhost").as_ref().to_vec();
        auth.push(0x45);
        auth.extend_from_slice(&0_u32.to_be_bytes());
        auth.extend_from_slice(&[0; 16]);
        auth.extend_from_slice(&32_u16.to_be_bytes());
        auth.extend_from_slice(&self.id);
        auth.extend(cose);
        anyhow::ensure!(auth.len() == 164, "fixture attestation size changed");
        let mut object = vec![
            0xa3, 0x63, b'f', b'm', b't', 0x64, b'n', b'o', b'n', b'e', 0x67, b'a', b't', b't',
            b'S', b't', b'm', b't', 0xa0, 0x68, b'a', b'u', b't', b'h', b'D', b'a', b't', b'a',
            0x58, 0xa4,
        ];
        object.extend(auth);
        Ok(
            json!({"id":encoded(&self.id),"rawId":encoded(&self.id),"type":"public-key","response":{"attestationObject":encoded(object),"clientDataJSON":encoded(client),"transports":["internal"]},"clientExtensionResults":{}}),
        )
    }

    pub fn assertion(&mut self, options: &Value, user: Uuid, fault: &str) -> anyhow::Result<Value> {
        self.counter += 1;
        let challenge = if fault == "challenge" {
            json!(encoded([9; 32]))
        } else {
            options["publicKey"]["challenge"].clone()
        };
        let origin = if fault == "origin" {
            "https://other.invalid"
        } else {
            "http://localhost:48027"
        };
        let mut client_data = json!({"type":"webauthn.get","challenge":challenge,"origin":origin,"crossOrigin":fault=="cross_origin"});
        if fault == "top_origin" {
            client_data["topOrigin"] = json!("https://other.invalid");
        }
        let client = serde_json::to_vec(&client_data)?;
        let rp = if fault == "rp" {
            b"other.invalid".as_slice()
        } else {
            b"localhost".as_slice()
        };
        let mut auth = digest(&SHA256, rp).as_ref().to_vec();
        auth.push(if fault == "uv" { 0x01 } else { 0x05 });
        auth.extend_from_slice(&self.counter.to_be_bytes());
        let mut signed = auth.clone();
        signed.extend_from_slice(digest(&SHA256, &client).as_ref());
        let signature = self
            .key
            .sign(&SystemRandom::new(), &signed)
            .map_err(|_| anyhow::anyhow!("fixture signing failed"))?;
        let mut signature = signature.as_ref().to_vec();
        if fault == "signature" {
            let last = signature.len() - 1;
            signature[last] ^= 1;
        }
        Ok(
            json!({"id":encoded(&self.id),"rawId":encoded(&self.id),"type":"public-key","response":{"authenticatorData":encoded(auth),"clientDataJSON":encoded(client),"signature":encoded(signature),"userHandle":if fault=="user_handle" {json!(encoded(Uuid::now_v7().as_bytes()))} else if fault=="no_handle" {Value::Null} else {json!(encoded(user.as_bytes()))}},"clientExtensionResults":{}}),
        )
    }
}
