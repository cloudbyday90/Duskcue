/*
 * Duskcue — Self-hosted media streaming server
 * Copyright (C) 2026-2026 Duskcue Contributors
 *
 * This program is free software: licensed under AGPL-3.0
 * See LICENSE file for details.
 */

import assert from 'node:assert/strict';
import { execFileSync } from 'node:child_process';
import fs from 'node:fs';
import path from 'node:path';
import { fileURLToPath } from 'node:url';

const root = path.resolve(path.dirname(fileURLToPath(import.meta.url)), '..');
const fixture = JSON.parse(fs.readFileSync(path.join(root, 'docs/api/fixtures/device-lab/v1/nvidia-shield-validation.json'), 'utf8'));
const options = parseArgs(process.argv.slice(2));

if (options.help) {
  printUsage();
  process.exit(0);
}

if (options.plan) {
  printPlan();
  process.exit(0);
}

assert(options.serial, 'A physical SHIELD ADB serial is required. Run with --plan to inspect the checklist.');
assert(['ethernet', 'wifi'].includes(options.network), '--network must be ethernet or wifi for a captured physical run.');
if (options.apk) {
  assert(fs.existsSync(path.resolve(options.apk)), `APK does not exist: ${options.apk}`);
}

const adb = process.env.ADB ?? 'adb';
assertConnected(adb, options.serial);
const manufacturer = getProp(adb, options.serial, 'ro.product.manufacturer');
const model = getProp(adb, options.serial, 'ro.product.model');
assert(manufacturer.toLowerCase().includes('nvidia'), `ADB target manufacturer is not NVIDIA: ${safeLabel(manufacturer)}`);
assert(model.toLowerCase().includes('shield'), `ADB target model is not a SHIELD: ${safeLabel(model)}`);
assert.notEqual(getProp(adb, options.serial, 'ro.kernel.qemu'), '1', 'ADB target is an emulator, not physical SHIELD hardware.');

const features = shell(adb, options.serial, ['pm', 'list', 'features']);
assert(features.includes('feature:android.software.leanback'), 'SHIELD target lacks android.software.leanback.');

if (options.apk) {
  run(adb, ['-s', options.serial, 'install', '-r', path.resolve(options.apk)]);
}

const packagePath = shell(adb, options.serial, ['pm', 'path', options.package]);
assert(packagePath.includes('package:'), `Duskcue package ${options.package} is not installed. Supply --apk or install it first.`);
const launcher = shell(adb, options.serial, [
  'am', 'start', '-W', '-n', `${options.package}/.MainActivity`, '-a', 'android.intent.action.MAIN', '-c', 'android.intent.category.LEANBACK_LAUNCHER'
]);
assertLaunchSucceeded(launcher, 'Leanback launcher');
const deepLink = shell(adb, options.serial, [
  'am', 'start', '-W', '-a', 'android.intent.action.VIEW', '-c', 'android.intent.category.DEFAULT', '-c', 'android.intent.category.BROWSABLE',
  '-d', 'duskcue://play/movie/123e4567-e89b-42d3-a456-426614174000', options.package
]);
assertLaunchSucceeded(deepLink, 'Duskcue deep link');

const displayDump = shell(adb, options.serial, ['dumpsys', 'display']);
const packageDump = shell(adb, options.serial, ['dumpsys', 'package', options.package]);
const report = {
  fixture: fixture.fixture,
  status: 'preflight_complete_manual_evidence_pending',
  captured_at: new Date().toISOString(),
  device: {
    family: 'nvidia_shield',
    model: safeLabel(model),
    android_release: safeLabel(getProp(adb, options.serial, 'ro.build.version.release')),
    android_api_level: safeLabel(getProp(adb, options.serial, 'ro.build.version.sdk')),
    firmware_version: safeLabel(getProp(adb, options.serial, 'ro.build.display.id')),
    display_size: parseDisplaySize(shell(adb, options.serial, ['wm', 'size'])),
    advertised_hdr_types: parseHdrTypes(displayDump),
  },
  app: {
    package: options.package,
    version_name: parsePackageValue(packageDump, 'versionName'),
    version_code: parsePackageValue(packageDump, 'versionCode'),
    leanback_launcher: 'passed',
    valid_deep_link_handoff: 'passed',
  },
  network_transport: options.network,
  manual_required_test_cases: fixture.test_cases.map((testCase) => testCase.id),
  scope_limit: fixture.machine_preflight.scope_limit,
};
console.log(JSON.stringify(report, null, 2));

function parseArgs(args) {
  const parsed = { package: 'com.duskcue.tv' };
  for (let index = 0; index < args.length; index += 1) {
    const value = args[index];
    if (value === '--plan') parsed.plan = true;
    else if (value === '--help' || value === '-h') parsed.help = true;
    else if (value === '--serial' || value === '--network' || value === '--apk' || value === '--package') {
      const next = args[index + 1];
      assert(next && !next.startsWith('--'), `${value} requires a value.`);
      parsed[value.slice(2)] = next;
      index += 1;
    } else {
      throw new Error(`Unknown argument: ${value}`);
    }
  }
  return parsed;
}

function printUsage() {
  console.log('Usage: node scripts/nvidia-shield-validation.mjs --plan');
  console.log('       node scripts/nvidia-shield-validation.mjs --serial <adb-serial> --network <ethernet|wifi> [--apk <path>] [--package com.duskcue.tv]');
}

function printPlan() {
  console.log(`NVIDIA SHIELD validation plan: ${fixture.status}`);
  console.log('Physical targets:');
  for (const target of fixture.device_targets) console.log(`- ${target.name}: ${target.required_for}`);
  console.log('Manual evidence cases:');
  for (const testCase of fixture.test_cases) console.log(`- ${testCase.id}: ${testCase.observation}`);
  console.log('No physical result is recorded by --plan. Use a connected SHIELD with --serial and retain evidence outside the repository.');
}

function assertConnected(adb, serial) {
  const devices = run(adb, ['devices', '-l'])
    .split(/\r?\n/)
    .filter((line) => line.startsWith(`${serial}\t`));
  assert.equal(devices.length, 1, `ADB serial ${serial} is not exactly one connected device.`);
  assert(devices[0].includes('\tdevice'), `ADB serial ${serial} is not ready for device commands.`);
}

function getProp(adb, serial, property) {
  return shell(adb, serial, ['getprop', property]).trim();
}

function shell(adb, serial, args) {
  return run(adb, ['-s', serial, 'shell', ...args]);
}

function run(command, args) {
  return execFileSync(command, args, { encoding: 'utf8', stdio: ['ignore', 'pipe', 'pipe'] });
}

function assertLaunchSucceeded(output, label) {
  assert(!/Error:|Exception occurred/i.test(output), `${label} failed: ${safeLabel(output)}`);
  assert(/Status: ok|Activity:/i.test(output), `${label} did not report a successful activity start.`);
}

function parseDisplaySize(value) {
  const match = value.match(/Physical size:\s*([^\r\n]+)/i);
  return match ? safeLabel(match[1]) : 'unknown';
}

function parseHdrTypes(value) {
  return [...new Set((value.match(/DOLBY_VISION|HDR10_PLUS|HDR10|HLG/gi) ?? []).map((item) => item.toLowerCase()))].sort();
}

function parsePackageValue(value, key) {
  const match = value.match(new RegExp(`${key}=([^\\s]+)`));
  return match ? safeLabel(match[1]) : 'unknown';
}

function safeLabel(value) {
  return value.trim().replace(/[^A-Za-z0-9._@x-]/g, '_').slice(0, 96) || 'unknown';
}
