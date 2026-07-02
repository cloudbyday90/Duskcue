import assert from 'node:assert/strict';
import fs from 'node:fs';
import path from 'node:path';
import { fileURLToPath } from 'node:url';

const root = path.resolve(path.dirname(fileURLToPath(import.meta.url)), '..');
const fixtureDir = path.join(root, 'docs', 'api', 'fixtures', 'diagnostics', 'v1');
const manifest = readJson(path.join(fixtureDir, 'manifest.json'));

const fixtures = new Map();
for (const entry of manifest.fixtures ?? []) {
  const fixture = readJson(path.join(fixtureDir, entry.file));
  assert.equal(fixture.fixture, entry.id, `${entry.file} fixture id mismatch`);
  fixtures.set(entry.id, fixture);
}

for (const id of manifest.fixtures.map((entry) => entry.id)) {
  assert(fixtures.has(id), `missing diagnostics fixture ${id}`);
}

assertLogSchema(fixtures.get('client-log-schema'));
assertBundleManifest(fixtures.get('diagnostics-bundle-manifest'));
assertRedactionRules(fixtures.get('redaction-rules'));
assertCorrelationFields(fixtures.get('correlation-fields'));
assertPrivacyClassification(fixtures.get('privacy-classification'));
assertPlatformChecklists(fixtures.get('platform-export-checklists'));
assertNoForbiddenFixtureLeaks();

console.log(`Verified ${fixtures.size} client diagnostics fixtures.`);

function readJson(filePath) {
  return JSON.parse(fs.readFileSync(filePath, 'utf8'));
}

function assertLogSchema(fixture) {
  const fields = new Map(fixture.fields.map((field) => [field.name, field]));
  for (const field of manifest.required_log_fields) {
    assert(fields.has(field), `missing log field ${field}`);
    assert.equal(fields.get(field).required, true, `${field} must be required`);
  }
  for (const severity of ['info', 'warn', 'error']) {
    assert(fixture.severity_values.includes(severity), `missing severity ${severity}`);
  }
  for (const record of fixture.sample_records) {
    for (const field of manifest.required_log_fields) {
      assert.notEqual(record[field], undefined, `sample record missing ${field}`);
    }
    assert(isUtc(record.timestamp), `sample timestamp is not UTC RFC3339: ${record.timestamp}`);
    assert(fixture.severity_values.includes(record.severity), `invalid sample severity ${record.severity}`);
  }
}

function assertBundleManifest(fixture) {
  const sections = new Map(fixture.sections.map((section) => [section.id, section]));
  for (const section of manifest.required_bundle_sections) {
    assert(sections.has(section), `missing bundle section ${section}`);
    assert.equal(sections.get(section).required, true, `${section} must be required`);
  }
  assert.equal(fixture.manual_sharing_only, true, 'bundles must be manual sharing only');
  assert(sections.get('app_logs').limit.max_records <= 1000, 'app log cap too large');
  assert(sections.get('recent_request_ids').limit.max_records <= 100, 'request id cap too large');
  assert(sections.get('user_consented_private_context').requires_explicit_consent, 'private context must require consent');
}

function assertRedactionRules(fixture) {
  const rules = new Map(fixture.forbidden_data.map((rule) => [rule.id, rule]));
  for (const rule of manifest.required_forbidden_data) {
    assert(rules.has(rule), `missing forbidden data rule ${rule}`);
  }
  for (const rule of fixture.forbidden_data) {
    assert.notEqual(rule.bundle_action, 'allow', `${rule.id} cannot be allowed in bundles`);
    assert.notEqual(rule.log_action, 'allow', `${rule.id} cannot be allowed in logs`);
  }
  const transforms = new Set(fixture.allowed_transforms.map((transform) => transform.id));
  for (const transform of ['host_only', 'stable_hash', 'strip_query', 'error_code_only']) {
    assert(transforms.has(transform), `missing allowed transform ${transform}`);
  }
}

function assertCorrelationFields(fixture) {
  const fields = new Map(fixture.fields.map((field) => [field.id, field]));
  for (const field of manifest.required_correlation_fields) {
    assert(fields.has(field), `missing correlation field ${field}`);
  }
  assert.equal(fields.get('request_id').source, 'x-request-id response/request header');
  assert(fields.get('trace_id').source.includes('Problem Details'));
  assert(fixture.rules.some((rule) => /prefer IDs/i.test(rule)), 'missing prefer IDs correlation rule');
  assert(fixture.rules.some((rule) => /Authorization/i.test(rule)), 'missing authorization exclusion rule');
}

function assertPrivacyClassification(fixture) {
  const classes = new Map(fixture.classes.map((item) => [item.id, item]));
  for (const id of manifest.required_privacy_classes) {
    assert(classes.has(id), `missing privacy class ${id}`);
  }
  assert.equal(classes.get('secret').export_behavior, 'never_export');
  assert.equal(classes.get('consent_required').export_behavior, 'explicit_consent_only');
}

function assertPlatformChecklists(fixture) {
  const platforms = new Map(fixture.platforms.map((platform) => [platform.id, platform]));
  for (const platform of manifest.required_platforms) {
    assert(platforms.has(platform), `missing platform checklist ${platform}`);
  }
  for (const platform of fixture.platforms) {
    assert(platform.required_checks.length >= 4, `${platform.id} needs at least four checks`);
    assert(platform.required_checks.some((check) => /exclude|omit|strip|avoid/i.test(check)), `${platform.id} needs a redaction check`);
  }
}

function assertNoForbiddenFixtureLeaks() {
  const content = fs
    .readdirSync(fixtureDir)
    .filter((file) => file.endsWith('.json') && file !== 'redaction-rules.json')
    .map((file) => fs.readFileSync(path.join(fixtureDir, file), 'utf8'))
    .join('\n');
  const leakPatterns = [
    /Bearer\s+[A-Za-z0-9._-]+/,
    /Authorization:\s*[A-Za-z]/,
    /password=/i,
    /token=[A-Za-z0-9._-]+/,
    /signature=[A-Za-z0-9._-]+/,
    /X-Api-Key/i,
    /file:\/\//i,
    /C:\\Users\\/i,
    /\/Users\/[^/\s]+/,
    /\/home\/[^/\s]+/,
    /\/mnt\/media\//i
  ];
  for (const pattern of leakPatterns) {
    assert(!pattern.test(content), `diagnostics fixture content matched forbidden pattern ${pattern}`);
  }
}

function isUtc(value) {
  return /^\d{4}-\d{2}-\d{2}T\d{2}:\d{2}:\d{2}Z$/.test(value);
}
