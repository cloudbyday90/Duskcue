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

use uuid::Uuid;

use crate::domains::quality::error::QualityError;

pub async fn report_capabilities(
    _user_id: Uuid,
    _device_identifier: &str,
) -> Result<(), QualityError> {
    todo!()
}

pub async fn get_device_profile(
    _user_id: Uuid,
    _device_identifier: &str,
) -> Result<(), QualityError> {
    todo!()
}

pub async fn list_capability_tests(
    _device_identifier: &str,
) -> Result<(), QualityError> {
    todo!()
}

pub async fn start_wizard(
    _user_id: Uuid,
    _device_identifier: &str,
) -> Result<(), QualityError> {
    todo!()
}

pub async fn submit_wizard_test_result(
    _test_id: Uuid,
    _result: &str,
) -> Result<(), QualityError> {
    todo!()
}

pub async fn submit_segment_telemetry(
    _user_id: Uuid,
    _session_id: Uuid,
) -> Result<(), QualityError> {
    todo!()
}

pub async fn submit_bandwidth_probe_result(
    _user_id: Uuid,
    _session_id: Uuid,
) -> Result<(), QualityError> {
    todo!()
}

pub async fn submit_qoe_report(
    _user_id: Uuid,
    _session_id: Uuid,
) -> Result<(), QualityError> {
    todo!()
}

pub async fn get_network_quality_summary() -> Result<(), QualityError> {
    todo!()
}

pub async fn get_device_capability_summary() -> Result<(), QualityError> {
    todo!()
}

pub async fn get_qoe_summary() -> Result<(), QualityError> {
    todo!()
}

pub async fn get_transcode_breakdown() -> Result<(), QualityError> {
    todo!()
}
