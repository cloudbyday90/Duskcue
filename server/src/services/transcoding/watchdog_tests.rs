// Duskcue — Self-hosted media streaming server
// Copyright (C) 2026-2026 Duskcue Contributors
// Licensed under AGPL-3.0. See LICENSE for details.

use super::*;

#[test]
fn only_advancing_media_time_extends_the_no_progress_deadline() {
    let mut watchdog = ProgressWatchdog::new(NO_PROGRESS_TIMEOUT);
    let began = watchdog.advanced_at;
    watchdog.observe_at(0, began + Duration::from_secs(5));
    assert_eq!(watchdog.deadline(), began + NO_PROGRESS_TIMEOUT);
    watchdog.observe_at(1000, began + Duration::from_secs(10));
    assert_eq!(watchdog.deadline(), began + Duration::from_secs(40));
    watchdog.observe_at(1000, began + Duration::from_secs(20));
    watchdog.observe_at(999, began + Duration::from_secs(25));
    assert_eq!(watchdog.deadline(), began + Duration::from_secs(40));
    watchdog.observe_at(1001, began + Duration::from_secs(28));
    assert_eq!(watchdog.deadline(), began + Duration::from_secs(58));
}
