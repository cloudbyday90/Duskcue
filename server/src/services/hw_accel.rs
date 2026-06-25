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

use std::collections::HashSet;

use crate::services::transcoding::HwAccelMethod;
use crate::state::{CpuConfig, TranscodingConfig};

#[derive(Debug, Clone)]
pub struct HwAccelDetectionResult {
    pub method: HwAccelMethod,
    pub nvidia_detected: bool,
    pub vaapi_available: bool,
    pub qsv_available: bool,
    pub amf_available: bool,
    pub videotoolbox_available: bool,
    pub verified_encoders: Vec<String>,
    pub source: String,
}

pub fn detect_hw_accel_runtime(
    transcoding: &TranscodingConfig,
    cpu: &CpuConfig,
) -> HwAccelDetectionResult {
    if !cpu.hw_accel_auto_detect {
        tracing::info!("HW accel auto-detect disabled by config, using software encoding");
        let result = software_result("config_disabled");
        emit_metrics(&result);
        return result;
    }

    let forced = match transcoding.hardware_accel.as_str() {
        "nvenc" => Some(HwAccelMethod::Nvenc),
        "qsv" => Some(HwAccelMethod::Qsv),
        "vaapi" => Some(HwAccelMethod::Vaapi),
        "videotoolbox" => Some(HwAccelMethod::VideoToolbox),
        "amf" => Some(HwAccelMethod::Amf),
        "software" => {
            tracing::info!("HW accel forced to software by config");
            let result = software_result("config_forced");
            emit_metrics(&result);
            return result;
        }
        _ => None,
    };

    let encoders = probe_ffmpeg_encoders();
    let hwaccels = probe_ffmpeg_hwaccels();

    let has_nvenc = encoders.contains("h264_nvenc") || encoders.contains("hevc_nvenc");
    let has_qsv = encoders.contains("h264_qsv") || encoders.contains("hevc_qsv");
    let has_vaapi = encoders.contains("h264_vaapi") || encoders.contains("hevc_vaapi");
    let has_amf = encoders.contains("h264_amf") || encoders.contains("hevc_amf");
    let has_vt = encoders.contains("h264_videotoolbox") || encoders.contains("hevc_videotoolbox");

    let nvidia_hw = check_nvidia_hardware();
    let vaapi_hw = check_vaapi_hardware();
    let vaapi_driver = detect_vaapi_driver();
    let is_intel = vaapi_driver.as_deref() == Some("i915");
    let _is_amd = vaapi_driver.as_deref() == Some("amdgpu");

    #[cfg(not(target_os = "linux"))]
    let _ = is_intel;

    let vt_hw = cfg!(target_os = "macos");

    if let Some(method) = forced {
        let encoder = method.ffmpeg_encoder("h264");
        if encoders.contains(&encoder) {
            let verified = collect_encoders(&encoders, &method);
            tracing::info!(
                method = method.as_str(),
                encoder = %encoder,
                source = "config",
                "HW accel method forced by config, encoder verified"
            );
            let result = HwAccelDetectionResult {
                method,
                nvidia_detected: nvidia_hw,
                vaapi_available: vaapi_hw && has_vaapi,
                qsv_available: has_qsv && hwaccels.contains("qsv"),
                amf_available: has_amf,
                videotoolbox_available: vt_hw,
                verified_encoders: verified,
                source: "config".to_string(),
            };
            emit_metrics(&result);
            return result;
        }
        tracing::warn!(
            method = method.as_str(),
            encoder = %encoder,
            "Forced HW accel method not available in FFmpeg, falling back to software"
        );
        let result = software_result("config_unavailable");
        emit_metrics(&result);
        return result;
    }

    if nvidia_hw && has_nvenc {
        return auto_result(
            HwAccelMethod::Nvenc,
            true,
            vaapi_hw && has_vaapi,
            has_qsv && hwaccels.contains("qsv"),
            has_amf,
            vt_hw,
            &encoders,
        );
    }

    #[cfg(target_os = "linux")]
    if is_intel && has_qsv && hwaccels.contains("qsv") {
        return auto_result(
            HwAccelMethod::Qsv,
            false,
            vaapi_hw && has_vaapi,
            true,
            has_amf,
            vt_hw,
            &encoders,
        );
    }

    #[cfg(not(target_os = "linux"))]
    if has_qsv && hwaccels.contains("qsv") {
        return auto_result(
            HwAccelMethod::Qsv,
            false,
            false,
            true,
            has_amf,
            vt_hw,
            &encoders,
        );
    }

    if vaapi_hw && has_vaapi && hwaccels.contains("vaapi") {
        return auto_result(
            HwAccelMethod::Vaapi,
            false,
            true,
            has_qsv && hwaccels.contains("qsv"),
            has_amf,
            vt_hw,
            &encoders,
        );
    }

    if vt_hw && has_vt {
        return auto_result(
            HwAccelMethod::VideoToolbox,
            false,
            false,
            false,
            false,
            true,
            &encoders,
        );
    }

    if has_amf {
        return auto_result(
            HwAccelMethod::Amf,
            false,
            vaapi_hw && has_vaapi,
            has_qsv && hwaccels.contains("qsv"),
            true,
            vt_hw,
            &encoders,
        );
    }

    tracing::info!("No hardware acceleration detected, using software encoding");
    let result = software_result("auto_fallback");
    emit_metrics(&result);
    result
}

fn auto_result(
    method: HwAccelMethod,
    nvidia_detected: bool,
    vaapi_available: bool,
    qsv_available: bool,
    amf_available: bool,
    videotoolbox_available: bool,
    all_encoders: &HashSet<String>,
) -> HwAccelDetectionResult {
    let verified = collect_encoders(all_encoders, &method);
    tracing::info!(
        method = method.as_str(),
        encoders = ?verified,
        source = "auto",
        "HW accel detected"
    );
    let result = HwAccelDetectionResult {
        method,
        nvidia_detected,
        vaapi_available,
        qsv_available,
        amf_available,
        videotoolbox_available,
        verified_encoders: verified,
        source: "auto".to_string(),
    };
    emit_metrics(&result);
    result
}

fn software_result(source: &str) -> HwAccelDetectionResult {
    HwAccelDetectionResult {
        method: HwAccelMethod::Software,
        nvidia_detected: false,
        vaapi_available: false,
        qsv_available: false,
        amf_available: false,
        videotoolbox_available: false,
        verified_encoders: vec![],
        source: source.to_string(),
    }
}

fn emit_metrics(result: &HwAccelDetectionResult) {
    for variant in [
        HwAccelMethod::Nvenc,
        HwAccelMethod::Qsv,
        HwAccelMethod::Vaapi,
        HwAccelMethod::VideoToolbox,
        HwAccelMethod::Amf,
        HwAccelMethod::Software,
    ] {
        let value = if variant == result.method { 1.0 } else { 0.0 };
        metrics::gauge!("system.cpu.hw_accel", "method" => variant.as_str()).set(value);
    }
}

fn probe_ffmpeg_encoders() -> HashSet<String> {
    let output = match std::process::Command::new("ffmpeg")
        .args(["-hide_banner", "-encoders"])
        .output()
    {
        Ok(o) => o,
        Err(e) => {
            tracing::debug!("FFmpeg encoder probe failed: {e}");
            return HashSet::new();
        }
    };

    let stdout = String::from_utf8_lossy(&output.stdout);
    let mut encoders = HashSet::new();
    for line in stdout.lines() {
        let trimmed = line.trim();
        if trimmed.starts_with('V') || trimmed.starts_with('A') {
            let mut parts = trimmed.split_whitespace();
            if let (Some(_flags), Some(name)) = (parts.next(), parts.next()) {
                encoders.insert(name.to_string());
            }
        }
    }
    encoders
}

fn probe_ffmpeg_hwaccels() -> HashSet<String> {
    let output = match std::process::Command::new("ffmpeg")
        .args(["-hide_banner", "-hwaccels"])
        .output()
    {
        Ok(o) => o,
        Err(_) => return HashSet::new(),
    };

    let stdout = String::from_utf8_lossy(&output.stdout);
    let mut methods = HashSet::new();
    for line in stdout.lines() {
        let trimmed = line.trim();
        if !trimmed.is_empty() && !trimmed.starts_with("Hardware") && !trimmed.starts_with('-') {
            methods.insert(trimmed.to_string());
        }
    }
    methods
}

fn check_nvidia_hardware() -> bool {
    #[cfg(target_os = "linux")]
    {
        if std::path::Path::new("/dev/nvidia0").exists()
            || std::path::Path::new("/dev/nvidia-uvm").exists()
        {
            return true;
        }
    }

    if std::process::Command::new("nvidia-smi")
        .stdout(std::process::Stdio::null())
        .stderr(std::process::Stdio::null())
        .status()
        .is_ok_and(|s| s.success())
    {
        return true;
    }

    false
}

fn check_vaapi_hardware() -> bool {
    #[cfg(target_os = "linux")]
    {
        if std::path::Path::new("/dev/dri/renderD128").exists() {
            return true;
        }
        if let Ok(entries) = std::fs::read_dir("/dev/dri") {
            for entry in entries.flatten() {
                let name = entry.file_name();
                let name_str = name.to_string_lossy();
                if name_str.starts_with("renderD") {
                    return true;
                }
            }
        }
    }

    false
}

fn detect_vaapi_driver() -> Option<String> {
    #[cfg(target_os = "linux")]
    {
        let render_node = find_render_node()?;
        let driver_link = std::fs::read_link(render_node.join("device/driver")).ok()?;
        let driver_name = driver_link.file_name()?.to_string_lossy().to_string();
        Some(driver_name)
    }

    #[cfg(not(target_os = "linux"))]
    None
}

#[cfg(target_os = "linux")]
fn find_render_node() -> Option<std::path::PathBuf> {
    if let Ok(entries) = std::fs::read_dir("/dev/dri") {
        for entry in entries.flatten() {
            let name = entry.file_name();
            let name_str = name.to_string_lossy();
            if name_str.starts_with("renderD") {
                let sys_path = std::path::PathBuf::from("/sys/class/drm").join(&*name_str);
                if sys_path.join("device/driver").exists() {
                    return Some(sys_path);
                }
            }
        }
    }
    None
}

fn collect_encoders(encoders: &HashSet<String>, method: &HwAccelMethod) -> Vec<String> {
    let targets: &[&str] = match method {
        HwAccelMethod::Nvenc => &["h264_nvenc", "hevc_nvenc", "av1_nvenc"],
        HwAccelMethod::Qsv => &["h264_qsv", "hevc_qsv", "vp9_qsv", "av1_qsv"],
        HwAccelMethod::Vaapi => &["h264_vaapi", "hevc_vaapi", "vp9_vaapi", "av1_vaapi"],
        HwAccelMethod::VideoToolbox => &["h264_videotoolbox", "hevc_videotoolbox"],
        HwAccelMethod::Amf => &["h264_amf", "hevc_amf", "av1_amf"],
        HwAccelMethod::Software => &["libx264", "libx265"],
    };
    targets
        .iter()
        .filter(|t| encoders.contains(**t))
        .map(|t| t.to_string())
        .collect()
}
