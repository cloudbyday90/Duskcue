// Duskcue — Self-hosted media streaming server
// Copyright (C) 2026-2026 Duskcue Contributors
// Licensed under AGPL-3.0. See LICENSE for details.

use super::ProgressUpdate;

pub struct ProgressParser {
    pending: ProgressUpdate,
    ended: bool,
}

impl Default for ProgressParser {
    fn default() -> Self {
        Self {
            pending: ProgressUpdate {
                frame: 0,
                fps: 0.0,
                bitrate_kbps: 0.0,
                total_size: 0,
                out_time_ms: 0,
                speed: 0.0,
                is_complete: false,
            },
            ended: false,
        }
    }
}

impl ProgressParser {
    pub fn push(&mut self, line: &str) -> Option<ProgressUpdate> {
        if self.ended || line.len() > 4096 {
            return None;
        }
        let (key, value) = line.trim().split_once('=')?;
        let value = value.trim();
        match key {
            "frame" => {
                if let Ok(frame) = value.parse() {
                    self.pending.frame = frame;
                }
            }
            "fps" => {
                if let Some(fps) = finite_number(value) {
                    self.pending.fps = fps;
                }
            }
            "bitrate" => {
                if let Some(bitrate) = finite_number(value.trim_end_matches("kbits/s").trim()) {
                    self.pending.bitrate_kbps = bitrate;
                }
            }
            "total_size" => {
                if let Ok(size) = value.parse() {
                    self.pending.total_size = size;
                }
            }
            "out_time_us" => {
                if let Ok(microseconds) = value.parse::<u64>() {
                    self.pending.out_time_ms = microseconds / 1000;
                }
            }
            "speed" => {
                if let Some(speed) = finite_number(value.trim_end_matches('x')) {
                    self.pending.speed = speed;
                }
            }
            "progress" if value == "continue" || value == "end" => {
                self.ended = value == "end";
                self.pending.is_complete = self.ended;
                return Some(self.pending.clone());
            }
            _ => {}
        }
        None
    }
}

fn finite_number(value: &str) -> Option<f64> {
    value
        .parse::<f64>()
        .ok()
        .filter(|number| number.is_finite() && *number >= 0.0)
}

#[cfg(test)]
#[path = "progress_tests.rs"]
mod tests;
