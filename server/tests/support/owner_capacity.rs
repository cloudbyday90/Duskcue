// Duskcue — Self-hosted media streaming server
// Copyright (C) 2026-2026 Duskcue Contributors
// Licensed under AGPL-3.0. See LICENSE for details.

use sqlx::PgPool;
use uuid::Uuid;

pub async fn seed(
    pool: &PgPool,
    user: Uuid,
    template: Uuid,
    count: usize,
    trusted: bool,
) -> anyhow::Result<Vec<Uuid>> {
    let bytes: Vec<u8> = sqlx::query_scalar("SELECT public_key FROM user_passkeys WHERE id=$1")
        .bind(template)
        .fetch_one(pool)
        .await?;
    let passkey: webauthn_rs::prelude::Passkey = serde_json::from_slice(&bytes)?;
    let credential: webauthn_rs::prelude::Credential = passkey.into();
    let mut ids = Vec::new();
    for _ in 0..count {
        let mut next = credential.clone();
        next.cred_id = Uuid::now_v7().as_bytes().to_vec();
        let key: webauthn_rs::prelude::Passkey = next.into();
        let id = sqlx::query_scalar("INSERT INTO user_passkeys(user_id,credential_id,public_key,name,owner_trusted) VALUES ($1,$2,$3,'Capacity fixture',$4) RETURNING id")
            .bind(user).bind(key.cred_id()).bind(serde_json::to_vec(&key)?).bind(trusted).fetch_one(pool).await?;
        ids.push(id);
    }
    Ok(ids)
}
