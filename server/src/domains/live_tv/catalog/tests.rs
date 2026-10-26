// Duskcue — Self-hosted media streaming server
// Copyright (C) 2026-2026 Duskcue Contributors
// Licensed under AGPL-3.0. See LICENSE for details.

use super::{
    admission::CatalogReads,
    authority::Viewing,
    cursor::{self, Context, Position},
    error::CatalogError,
    limits::{Budget, MIB},
    query,
    types::ChannelsRequest,
};
use crate::{
    domains::{household_security::types::Scope, live_tv::feed::source::Fence},
    services::encryption::EncryptionKey,
};
use uuid::Uuid;

fn view() -> Viewing {
    let user = Uuid::now_v7();
    Viewing {
        actor: Scope {
            user,
            session: Uuid::now_v7(),
            profile: Uuid::now_v7(),
            version: 1,
        },
        profile_revision: 1,
        assignment_revision: 1,
        fence: Fence {
            user,
            assignment: Uuid::now_v7(),
            generation: 1,
        },
    }
}

#[test]
fn admission_is_bounded_nonqueued_and_permanently_closable() {
    let reads = CatalogReads::default();
    let permits = (0..4).map(|_| reads.admit().unwrap()).collect::<Vec<_>>();
    assert!(matches!(reads.admit(), Err(CatalogError::Unavailable)));
    drop(permits);
    assert!(reads.admit().is_ok());
    reads.close();
    assert!(reads.admit().is_err());
}

#[test]
fn window_requires_offsets_bounds_and_preserves_exact_instants() {
    let result = query::window("2026-10-25T01:30:00.123456-04:00", "2026-10-25T07:00:00Z").unwrap();
    assert_eq!(result.from.to_rfc3339(), "2026-10-25T05:30:00.123456+00:00");
    for (from, to) in [
        ("2026-10-25T01:00:00", "2026-10-25T02:00:00Z"),
        ("2026-10-25T01:00:00.0000001Z", "2026-10-25T02:00:00Z"),
        ("2026-10-25T01:00:00.0000000Z", "2026-10-25T02:00:00Z"),
        ("2026-10-25T01:00:00.1234560001Z", "2026-10-25T02:00:00Z"),
        ("2016-12-31T23:59:60Z", "2017-01-01T01:00:00Z"),
        ("2026-10-25T01:00:00Z", "2026-10-25T02:00:00.0000000001Z"),
        ("2026-10-25T01:00:00Z", "2026-10-25T01:00:00Z"),
        ("2026-10-25T02:00:00Z", "2026-10-25T01:00:00Z"),
        ("2026-10-25T00:00:00Z", "2026-10-26T00:00:01Z"),
    ] {
        assert!(query::window(from, to).is_err());
    }
    assert!(query::window("2026-10-25T00:00:00Z", "2026-10-26T00:00:00Z").is_ok());
}

#[test]
fn query_refuses_selector_injection_duplicate_fields_and_excess_limits() {
    for query in [
        "limit=1&limit=2",
        "profile_id=01950000-0000-7000-8000-000000000001",
        "household_id=x",
    ] {
        let uri = format!("/api/v1/live-tv/catalog/channels?{query}")
            .parse()
            .unwrap();
        assert!(axum::extract::Query::<ChannelsRequest>::try_from_uri(&uri).is_err());
    }
    assert_eq!(query::limit(None).unwrap(), 20);
    assert!(query::limit(Some(0)).is_err());
    assert!(query::limit(Some(101)).is_err());
    assert!(query::cursor(Some(&"x".repeat(2049))).is_err());
    assert!(query::id(Uuid::nil()).is_err());
    assert!(query::id(Uuid::now_v7()).is_ok());
}

#[test]
fn cursors_are_sealed_context_bound_and_not_profile_roundtrip_revivable() {
    let (key, _) = EncryptionKey::generate();
    let mut owner = view();
    let snapshot = Uuid::now_v7();
    let original_profile = owner.actor.profile;
    let token = cursor::encode(
        &key,
        &owner,
        Context::new(&owner, snapshot, 20, None, None),
        Position::Channels { key: vec![9; 32] },
        &mut Budget::new(),
    )
    .unwrap();
    let position = cursor::decode(
        &key,
        &owner,
        &Context::new(&owner, snapshot, 20, None, None),
        Some(&token),
        &mut Budget::new(),
    )
    .unwrap();
    assert!(matches!(position,Some(Position::Channels {key}) if key==vec![9;32]));
    owner.actor.profile = Uuid::now_v7();
    owner.profile_revision = 2;
    assert!(matches!(
        cursor::decode(
            &key,
            &owner,
            &Context::new(&owner, snapshot, 20, None, None),
            Some(&token),
            &mut Budget::new()
        ),
        Err(CatalogError::Changed)
    ));
    owner.actor.profile = original_profile;
    owner.profile_revision = 3;
    assert!(matches!(
        cursor::decode(
            &key,
            &owner,
            &Context::new(&owner, snapshot, 20, None, None),
            Some(&token),
            &mut Budget::new()
        ),
        Err(CatalogError::Changed)
    ));
    owner.profile_revision = 1;
    for context in [
        Context::new(&owner, Uuid::now_v7(), 20, None, None),
        Context::new(&owner, snapshot, 21, None, None),
        Context::new(&owner, snapshot, 20, Some(Uuid::now_v7()), None),
    ] {
        assert!(matches!(
            cursor::decode(&key, &owner, &context, Some(&token), &mut Budget::new()),
            Err(CatalogError::Changed)
        ));
    }
}

#[test]
fn cursor_tampering_foreign_session_and_changed_authorities_fail_closed() {
    let (key, _) = EncryptionKey::generate();
    let mut owner = view();
    let snapshot = Uuid::now_v7();
    let token = cursor::encode(
        &key,
        &owner,
        Context::new(&owner, snapshot, 20, None, None),
        Position::Channels { key: vec![1; 32] },
        &mut Budget::new(),
    )
    .unwrap();
    let mut changed = token.clone().into_bytes();
    changed[32] = if changed[32] == b'A' { b'B' } else { b'A' };
    let changed = String::from_utf8(changed).unwrap();
    assert!(
        cursor::decode(
            &key,
            &owner,
            &Context::new(&owner, snapshot, 20, None, None),
            Some(&changed),
            &mut Budget::new()
        )
        .is_err()
    );
    owner.assignment_revision += 1;
    assert!(matches!(
        cursor::decode(
            &key,
            &owner,
            &Context::new(&owner, snapshot, 20, None, None),
            Some(&token),
            &mut Budget::new()
        ),
        Err(CatalogError::Changed)
    ));
    owner.assignment_revision -= 1;
    owner.actor.version += 1;
    assert!(matches!(
        cursor::decode(
            &key,
            &owner,
            &Context::new(&owner, snapshot, 20, None, None),
            Some(&token),
            &mut Budget::new()
        ),
        Err(CatalogError::Changed)
    ));
    owner.actor.session = Uuid::now_v7();
    assert!(matches!(
        cursor::decode(
            &key,
            &owner,
            &Context::new(&owner, snapshot, 20, None, None),
            Some(&token),
            &mut Budget::new()
        ),
        Err(CatalogError::Invalid)
    ));
}

#[test]
fn timed_and_unpositionable_cursors_cannot_retarget_each_other() {
    let (key, _) = EncryptionKey::generate();
    let owner = view();
    let snapshot = Uuid::now_v7();
    let channel = Uuid::now_v7();
    let window = query::window("2026-10-25T00:00:00Z", "2026-10-25T01:00:00Z").unwrap();
    let token = cursor::encode(
        &key,
        &owner,
        Context::new(&owner, snapshot, 20, Some(channel), Some(window)),
        Position::Timed {
            start: window.from,
            id: Uuid::now_v7(),
        },
        &mut Budget::new(),
    )
    .unwrap();
    assert!(matches!(
        cursor::decode(
            &key,
            &owner,
            &Context::new(&owner, snapshot, 20, Some(channel), None),
            Some(&token),
            &mut Budget::new()
        ),
        Err(CatalogError::Changed)
    ));
    assert!(
        cursor::encode(
            &key,
            &owner,
            Context::new(&owner, snapshot, 20, None, None),
            Position::Channels { key: vec![1; 31] },
            &mut Budget::new()
        )
        .is_err()
    );
}

#[test]
fn aggregate_and_serialization_budgets_refuse_excess_before_any_response() {
    let mut budget = Budget::new();
    budget.cipher(2 * MIB).unwrap();
    assert!(budget.cipher(1).is_err());
    let mut budget = Budget::new();
    budget.plain(MIB).unwrap();
    assert!(budget.plain(1).is_err());
    let budget = Budget::new();
    assert!(budget.serialize(&"x".repeat(MIB)).is_err());
    let mut budget = Budget::new();
    budget.deadline = tokio::time::Instant::now();
    assert!(budget.serialize(&"small").is_err());
}
