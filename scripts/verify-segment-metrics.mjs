/*
 * Duskcue — Self-hosted media streaming server
 * Copyright (C) 2026-2026 Duskcue Contributors
 *
 * This program is free software: licensed under AGPL-3.0
 * See LICENSE file for details.
 */

import assert from 'node:assert/strict';
import fs from 'node:fs';
import path from 'node:path';
import { fileURLToPath } from 'node:url';

const root = path.resolve(path.dirname(fileURLToPath(import.meta.url)), '..');
const read = (relativePath) => fs.readFileSync(path.join(root, relativePath), 'utf8');

const design = read('docs/design/SEGMENT_DETECTION.md');
const worker = read('server/src/workers/segment_detector.rs');

for (const metric of [
  'segment_analysis_files_total',
  'segment_analysis_duration_seconds',
  'segment_segments_created_total',
  'segment_segments_active',
  'segment_low_confidence_total',
  'segment_analysis_errors_total',
]) {
  assert.match(design, new RegExp(`\\\`${metric}\\\``));
  assert.match(worker, new RegExp(`"${metric}"`));
}

assert.match(design, /unbounded Prometheus time series/);
assert.match(design, /`segment_skip_total` remains deferred/);
assert.match(worker, /"segment_analysis_files_total", "method" => method\.as_str\(\)/);
assert.match(worker, /"segment_analysis_errors_total", "stage" => stage\.as_str\(\)/);
assert.match(worker, /"segment_segments_created_total",[\s\S]{0,160}"type" => detected\.segment_type\.as_str\(\)/);
assert.match(worker, /"segment_segments_active", "type" => \*segment_type/);
assert.match(worker, /metrics::histogram!\("segment_analysis_duration_seconds"\)/);
assert.doesNotMatch(
  worker,
  /segment_(?:analysis_files|analysis_duration|segments_created|segments_active|low_confidence|analysis_errors)[\s\S]{0,200}"library"\s*=>/,
);

console.log('Verified bounded Segment Analysis Prometheus metrics.');
