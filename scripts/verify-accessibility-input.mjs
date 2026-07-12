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
const fixtureDir = path.join(root, 'docs', 'api', 'fixtures', 'accessibility', 'v1');
const manifest = readJson(path.join(fixtureDir, 'manifest.json'));

const fixtures = new Map();
for (const entry of manifest.fixtures ?? []) {
  const fixture = readJson(path.join(fixtureDir, entry.file));
  assert.equal(fixture.fixture, entry.id, `${entry.file} fixture id mismatch`);
  fixtures.set(entry.id, fixture);
}

for (const id of manifest.fixtures.map((entry) => entry.id)) {
  assert(fixtures.has(id), `missing accessibility fixture ${id}`);
}

assertBaselineChecklist(fixtures.get('accessibility-baseline-checklist'));
assertFocusOrder(fixtures.get('focus-order-tests'));
assertRemoteNavigation(fixtures.get('remote-navigation-tests'));
assertPlatformReviews(fixtures.get('platform-review-checklists'));
assertLocalization(fixtures.get('localization-rtl-cases'));

console.log(`Verified ${fixtures.size} accessibility/input baseline fixtures.`);

function readJson(filePath) {
  return JSON.parse(fs.readFileSync(filePath, 'utf8'));
}

function assertBaselineChecklist(fixture) {
  const baselines = new Map(fixture.baselines.map((item) => [item.id, item]));
  for (const category of manifest.required_baseline_categories) {
    assert(baselines.has(category), `missing baseline category ${category}`);
  }
  for (const item of fixture.baselines) {
    assertPlatformFamilies(item.platform_families, item.id);
    assert(item.requirements.length >= 3, `${item.id} needs actionable requirements`);
    assert(item.evidence.length >= 1, `${item.id} needs evidence`);
  }
  assertCoversAllPlatformFamilies(fixture.baselines.flatMap((item) => item.platform_families), 'baseline checklist');
}

function assertFocusOrder(fixture) {
  const cases = new Map(fixture.cases.map((item) => [item.id, item]));
  for (const id of manifest.required_focus_cases) {
    assert(cases.has(id), `missing focus-order case ${id}`);
  }
  for (const item of fixture.cases) {
    assertPlatformFamilies(item.platform_families, item.id);
    assert(item.sequence.length >= 3, `${item.id} needs a sequence`);
    assertTruthyExpectations(item.expect, item.id);
  }
}

function assertRemoteNavigation(fixture) {
  const cases = new Map(fixture.cases.map((item) => [item.id, item]));
  for (const id of manifest.required_remote_cases) {
    assert(cases.has(id), `missing remote-navigation case ${id}`);
  }
  for (const item of fixture.cases) {
    assert(item.platform_families.includes('tv') || item.platform_families.includes('console'), `${item.id} must cover TV or console`);
    assert(item.input.length >= 3, `${item.id} needs remote/controller input steps`);
    assertTruthyExpectations(item.expect, item.id);
  }
}

function assertPlatformReviews(fixture) {
  const platforms = new Map(fixture.platforms.map((item) => [item.id, item]));
  for (const platform of manifest.required_platform_reviews) {
    assert(platforms.has(platform), `missing platform review ${platform}`);
  }
  for (const item of fixture.platforms) {
    assert(manifest.required_platform_families.includes(item.family), `${item.id} invalid family`);
    assert(item.assistive_technology.length >= 2, `${item.id} needs assistive technology coverage`);
    assert(item.required_checks.length >= 4, `${item.id} needs required checks`);
    assert(item.required_checks.some((check) => /caption/i.test(check)), `${item.id} must include caption/subtitle coverage`);
  }
}

function assertLocalization(fixture) {
  const cases = new Map(fixture.cases.map((item) => [item.id, item]));
  for (const id of manifest.required_localization_cases) {
    assert(cases.has(id), `missing localization case ${id}`);
  }
  for (const item of fixture.cases) {
    assertPlatformFamilies(item.platform_families, item.id);
    assertTruthyExpectations(item.expect, item.id);
  }
  assert.equal(cases.get('rtl_layout_mirroring').expect.layout_direction, 'rtl');
  assert.equal(cases.get('directional_icon_mirroring').expect.playback_timeline_does_not_reverse_time, true);
  assert.equal(cases.get('activation_gate').expect.rtl_layout_review_required_for_rtl_locales, true);
}

function assertPlatformFamilies(families, context) {
  assert(Array.isArray(families) && families.length > 0, `${context} needs platform families`);
  for (const family of families) {
    assert(manifest.required_platform_families.includes(family), `${context} invalid platform family ${family}`);
  }
}

function assertCoversAllPlatformFamilies(families, context) {
  const familySet = new Set(families);
  for (const family of manifest.required_platform_families) {
    assert(familySet.has(family), `${context} missing platform family ${family}`);
  }
}

function assertTruthyExpectations(expect, context) {
  assert(expect && typeof expect === 'object', `${context} needs expectations`);
  const values = Object.values(expect);
  assert(values.length > 0, `${context} expectations cannot be empty`);
  for (const value of values) {
    if (typeof value === 'boolean') {
      assert.equal(value, true, `${context} boolean expectation must be true`);
    } else {
      assert.notEqual(value, '', `${context} expectation string cannot be empty`);
    }
  }
}
