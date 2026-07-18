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

const design = read('docs/design/STORYBOARDS.md');
const worker = read('server/src/workers/storyboard_generator.rs');
const handlers = read('server/src/domains/storyboards/handlers.rs');

for (const metric of [
  'storyboard_files_processed_total',
  'storyboard_generation_duration_seconds',
  'storyboard_sprites_created_total',
  'storyboard_storage_bytes',
  'storyboard_served_total',
  'storyboard_generation_errors_total',
]) {
  assert.match(design, new RegExp(`\\\`${metric}\\\``));
}

assert.match(design, /fixed outcome\/asset\/error-kind vocabularies/);
assert.match(design, /unbounded, private, and unsuitable for a long-lived self-hosted server/);
assert.match(worker, /"storyboard_files_processed_total", "outcome" => outcome\.as_str\(\)/);
assert.match(worker, /"storyboard_generation_errors_total", "kind" => kind\.as_str\(\)/);
assert.match(worker, /"storyboard_sprites_created_total"/);
assert.match(worker, /"storyboard_generation_duration_seconds"/);
assert.match(worker, /"storyboard_storage_bytes"/);
assert.match(worker, /fn storyboard_cache_bytes\(path: &Path\)/);
assert.match(handlers, /"storyboard_served_total"/);
assert.match(handlers, /record_storyboard_served\("index", result\.is_ok\(\)\)/);
assert.match(handlers, /record_storyboard_served\("sprite", result\.is_ok\(\)\)/);
assert.doesNotMatch(
  worker,
  /storyboard_(?:files_processed|generation_duration|sprites_created|storage|generation_errors)[\s\S]{0,160}"library"\s*=>/,
);

console.log('Verified bounded storyboard Prometheus metrics and cache-size measurement.');
