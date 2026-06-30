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

use std::path::Path;

use metrics_exporter_prometheus::{Matcher, PrometheusBuilder, PrometheusHandle};
use tracing_appender::non_blocking::WorkerGuard;
use tracing_appender::rolling::{RollingFileAppender, Rotation};
use tracing_error::ErrorLayer;
use tracing_subscriber::{EnvFilter, Registry, layer::SubscriberExt, util::SubscriberInitExt};

pub fn init_logging(log_level: &str, data_dir: &Path) -> WorkerGuard {
    let log_dir = data_dir.join("logs");

    let env_filter =
        EnvFilter::try_from_default_env().unwrap_or_else(|_| EnvFilter::new(log_level));

    let file_appender = RollingFileAppender::builder()
        .rotation(Rotation::DAILY)
        .filename_prefix("server")
        .filename_suffix("log")
        .max_log_files(5)
        .build(&log_dir)
        .expect("failed to create log directory");

    let (non_blocking, guard) = tracing_appender::non_blocking(file_appender);

    let console_layer = tracing_subscriber::fmt::layer().pretty();

    let file_layer = tracing_subscriber::fmt::layer()
        .json()
        .with_writer(non_blocking);

    Registry::default()
        .with(env_filter)
        .with(ErrorLayer::default())
        .with(console_layer)
        .with(file_layer)
        .init();

    guard
}

pub fn init_metrics() -> PrometheusHandle {
    PrometheusBuilder::new()
        .set_buckets_for_metric(
            Matcher::Full("http_request_duration".to_string()),
            &[
                0.005, 0.01, 0.025, 0.05, 0.1, 0.25, 0.5, 1.0, 2.5, 5.0, 10.0,
            ],
        )
        .expect("failed to set histogram buckets")
        .set_buckets_for_metric(
            Matcher::Full("search_query_duration_seconds".to_string()),
            &[0.005, 0.01, 0.025, 0.05, 0.1, 0.2, 0.5, 1.0, 2.5],
        )
        .expect("failed to set search histogram buckets")
        .set_buckets_for_metric(
            Matcher::Full("image_variant_generation_duration_seconds".to_string()),
            &[0.01, 0.025, 0.05, 0.1, 0.25, 0.5, 1.0, 2.5, 5.0],
        )
        .expect("failed to set image variant histogram buckets")
        .install_recorder()
        .expect("failed to install Prometheus metrics recorder")
}
