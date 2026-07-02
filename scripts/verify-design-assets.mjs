import assert from 'node:assert/strict';
import fs from 'node:fs';
import path from 'node:path';
import { fileURLToPath } from 'node:url';

const root = path.resolve(path.dirname(fileURLToPath(import.meta.url)), '..');
const fixtureDir = path.join(root, 'docs', 'api', 'fixtures', 'design', 'v1');
const manifest = readJson(path.join(fixtureDir, 'manifest.json'));

const fixtures = new Map();
for (const entry of manifest.fixtures ?? []) {
  const fixture = readJson(path.join(fixtureDir, entry.file));
  assert.equal(fixture.fixture, entry.id, `${entry.file} fixture id mismatch`);
  fixtures.set(entry.id, fixture);
}

for (const id of manifest.fixtures.map((entry) => entry.id)) {
  assert(fixtures.has(id), `missing design fixture ${id}`);
}

assertDesignTokens(fixtures.get('design-tokens'));
assertAssetInventory(fixtures.get('asset-inventory'));
assertArtworkRules(fixtures.get('artwork-rules'));
assertStringOwnership(fixtures.get('string-ownership'));
assertMediaStateBadges(fixtures.get('media-state-badges'));
assertPlatformMapping(fixtures.get('platform-mapping'));

console.log(`Verified ${fixtures.size} design asset/token fixtures.`);

function readJson(filePath) {
  return JSON.parse(fs.readFileSync(filePath, 'utf8'));
}

function assertDesignTokens(fixture) {
  for (const group of manifest.required_token_groups) {
    assert(fixture.token_groups?.[group], `missing token group ${group}`);
    assert(Object.keys(fixture.token_groups[group]).length > 0, `token group ${group} is empty`);
  }

  const colorTokens = fixture.token_groups.color;
  for (const [name, token] of Object.entries(colorTokens)) {
    assert.equal(token.$type, 'color', `${name} must be a color token`);
    assert(/^#[0-9a-f]{6}$/i.test(token.$value), `${name} has invalid hex color ${token.$value}`);
  }

  const requiredCssMappings = [
    '--color-bg-deep',
    '--color-bg-surface',
    '--color-bg-elevated',
    '--color-text-primary',
    '--color-accent',
    '--color-success',
    '--color-warning',
    '--color-error',
    '--focus-ring'
  ];
  const mappings = new Set(Object.values(fixture.css_mapping ?? {}));
  for (const mapping of requiredCssMappings.slice(0, -1)) {
    assert(mappings.has(mapping), `missing CSS mapping ${mapping}`);
  }
  assert.equal(fixture.token_groups.focus['ring.color'].$value, fixture.token_groups.color['accent.default'].$value);
  assert.equal(fixture.token_groups.focus['ring.width'].$value, '2px');
}

function assertAssetInventory(fixture) {
  const assets = new Map(fixture.assets.map((asset) => [asset.role, asset]));
  for (const role of manifest.required_asset_roles) {
    assert(assets.has(role), `missing asset role ${role}`);
  }
  for (const asset of fixture.assets) {
    assert.equal(asset.type, 'svg', `${asset.role} must use SVG source`);
    assert(Array.isArray(asset.platforms) && asset.platforms.length > 0, `${asset.role} needs platforms`);
    for (const platform of asset.platforms) {
      assert(manifest.required_platforms.includes(platform), `${asset.role} invalid platform ${platform}`);
    }
    const assetPath = path.join(root, asset.path);
    assert(fs.existsSync(assetPath), `${asset.role} source asset missing at ${asset.path}`);
    assert(fs.readFileSync(assetPath, 'utf8').includes('<svg'), `${asset.path} is not an SVG source`);
  }
}

function assertArtworkRules(fixture) {
  const rules = new Map(fixture.rules.map((rule) => [rule.id, rule]));
  for (const rule of manifest.required_artwork_rules) {
    assert(rules.has(rule), `missing artwork rule ${rule}`);
  }
  for (const [type, variant] of Object.entries(fixture.variants ?? {})) {
    assert(Array.isArray(variant.sizes) && variant.sizes.includes(variant.default_size), `${type} default size missing from size list`);
    assert(variant.aspect_ratio, `${type} missing aspect ratio`);
  }
  assert.equal(fixture.variants.poster.aspect_ratio, '2:3');
  assert.equal(fixture.variants.backdrop.aspect_ratio, '16:9');
  assert(rules.get('authenticated_url').url_shape.startsWith('/api/v1/items/'));
  assert(rules.get('signed_url').must_not.some((item) => /plaintext/i.test(item)));
  assert(rules.get('cache_busting').validator_priority.includes('ETag'));
  assert(rules.get('fallback').sequence.includes('type-specific placeholder asset'));
  assert(rules.get('offline').must_not.some((item) => /expired remote signed URLs/i.test(item)));
  assert(rules.get('unavailable').must.some((item) => /server revalidation/i.test(item)));
}

function assertStringOwnership(fixture) {
  for (const section of manifest.required_string_sections) {
    assert(Array.isArray(fixture[section]) && fixture[section].length > 0, `missing string ownership section ${section}`);
  }
  assert(fixture.server_owned.some((item) => item.category === 'media_metadata'), 'server-owned media metadata missing');
  assert(fixture.server_owned.some((item) => item.category === 'problem_details'), 'server-owned Problem Details missing');
  assert(fixture.client_owned.some((item) => item.category === 'navigation'), 'client-owned navigation missing');
  assert(fixture.client_owned.some((item) => item.category === 'accessibility_labels'), 'client-owned accessibility labels missing');
  assert(fixture.shared_key_reuse.some((item) => item.rule === 'badge_label_keys'), 'badge label key reuse missing');
}

function assertMediaStateBadges(fixture) {
  const badges = new Map(fixture.badges.map((badge) => [badge.state, badge]));
  for (const state of manifest.required_badge_states) {
    assert(badges.has(state), `missing media-state badge ${state}`);
  }
  for (const badge of fixture.badges) {
    assert(/^badge\.tone\.(neutral|success|warning|error|info)$/.test(badge.tone_token), `${badge.state} invalid tone token`);
    assert(badge.icon_hint && badge.label_key.startsWith('media_state.'), `${badge.state} missing icon hint or label key`);
    assert(badge.visibility, `${badge.state} missing visibility rule`);
  }
  assert(fixture.requirements.some((item) => /color alone/i.test(item)), 'badge color-alone requirement missing');
}

function assertPlatformMapping(fixture) {
  const platforms = new Map(fixture.platforms.map((platform) => [platform.id, platform]));
  for (const platform of manifest.required_platforms) {
    assert(platforms.has(platform), `missing platform mapping ${platform}`);
  }
  for (const platform of fixture.platforms) {
    assert(platform.token_output, `${platform.id} missing token output`);
    assert(platform.asset_output, `${platform.id} missing asset output`);
    assert(platform.focus_behavior, `${platform.id} missing focus behavior`);
  }
}
