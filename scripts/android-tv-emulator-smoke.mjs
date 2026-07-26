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
import { spawnSync } from 'node:child_process';
import { fileURLToPath } from 'node:url';

const root = path.resolve(path.dirname(fileURLToPath(import.meta.url)), '..');
const args = process.argv.slice(2);
const apkPath = path.resolve(root, valueArg('--apk') ?? 'clients/tv/android/app/build/outputs/apk/debug/app-debug.apk');
const requestedSerial = valueArg('--serial') ?? process.env.ANDROID_SERIAL;
const adb = process.env.ADB ?? 'adb';

if (args.includes('--help')) {
  console.log(`Usage:
  node scripts/android-tv-emulator-smoke.mjs [--apk path/to/app-debug.apk] [--serial emulator-5554]

Installs the Android TV debug APK on one connected Android TV emulator or device, verifies
the Leanback launcher activity, and exercises a valid Duskcue playback deep-link handoff.`);
  process.exit(0);
}

assert(fs.existsSync(apkPath), `Android TV APK not found: ${apkPath}`);
const serial = selectSerial(requestedSerial);

runAdb(serial, ['wait-for-device']);
const features = runAdb(serial, ['shell', 'pm', 'list', 'features']);
assert.match(features, /android\.software\.leanback/, 'connected target is not an Android TV / Google TV runtime');

runAdb(serial, ['install', '-r', apkPath]);

const launcherOutput = runAdb(serial, [
  'shell',
  'am',
  'start',
  '-W',
  '-a',
  'android.intent.action.MAIN',
  '-c',
  'android.intent.category.LEANBACK_LAUNCHER',
  '-p',
  'com.duskcue.tv'
]);
assert.match(launcherOutput, /Status:\s*ok|Activity:/i, 'Android TV Leanback launcher activity did not start');

const deepLinkOutput = runAdb(serial, [
  'shell',
  'am',
  'start',
  '-W',
  '-a',
  'android.intent.action.VIEW',
  '-d',
  'duskcue://play/movie/123e4567-e89b-12d3-a456-426614174000',
  '-p',
  'com.duskcue.tv'
]);
assert.match(deepLinkOutput, /Status:\s*ok|Activity:/i, 'Android TV playback deep-link activity did not start');

const topActivity = runAdb(serial, ['shell', 'dumpsys', 'activity', 'top']);
assert.match(topActivity, /com\.duskcue\.tv/, 'Duskcue Android TV activity is not the active task after smoke launch');

console.log(`Android TV emulator smoke passed on ${serial}.`);

function selectSerial(requested) {
  const devices = runAdb(undefined, ['devices'])
    .split(/\r?\n/)
    .map((line) => line.trim().split(/\s+/))
    .filter(([serial, state]) => serial && state === 'device')
    .map(([serial]) => serial);

  if (requested) {
    assert(devices.includes(requested), `requested Android device is not ready: ${requested}`);
    return requested;
  }

  assert.equal(devices.length, 1, `expected exactly one ready Android TV target; found ${devices.length || 'none'}`);
  return devices[0];
}

function runAdb(serial, commandArgs) {
  const args = serial ? ['-s', serial, ...commandArgs] : commandArgs;
  const result = spawnSync(adb, args, {
    cwd: root,
    encoding: 'utf8',
    stdio: ['ignore', 'pipe', 'pipe']
  });
  const output = `${result.stdout ?? ''}${result.stderr ?? ''}`;
  assert.equal(result.status, 0, `${adb} ${args.join(' ')} failed:\n${output}`);
  return output;
}

function valueArg(name) {
  const index = args.indexOf(name);
  return index === -1 ? undefined : args[index + 1];
}
