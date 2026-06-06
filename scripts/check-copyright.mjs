#!/usr/bin/env node
/*
 * Duskcue — Self-hosted media streaming server
 * Copyright (C) 2026-2026 Duskcue Contributors
 *
 * This program is free software: licensed under AGPL-3.0
 * See LICENSE file for details.
 */

import fs from 'node:fs';

const CURRENT_YEAR = new Date().getFullYear();
const EXPECTED_PATTERN = `2026-${CURRENT_YEAR}`;
const COPYRIGHT_RE = /Copyright \(C\) (\d{4}|\d{4}-\d{4}) Duskcue Contributors/g;

const FILE_PATTERNS = [
  'server/src/**/*.rs',
  'crates/types/src/**/*.rs',
  'crates/db/src/**/*.rs',
  'clients/desktop/src-tauri/src/**/*.rs',
  'server/migrations/**/*.sql',
  'clients/web/src/**/*.{js,svelte,html,css}',
  'clients/web/*.js',
  'clients/desktop/src/**/*.html',
  'clients/desktop/*.js',
  'clients/mobile/lib/**/*.dart',
  'scripts/**/*.{sh,mjs}',
  'docker/**/*.sh',
];

const IGNORE_SEGMENTS = ['node_modules', 'dist', 'build', 'coverage', 'target'];

const isIgnored = (path) => IGNORE_SEGMENTS.some(seg => path.includes(seg));

function checkFile(filePath) {
  const content = fs.readFileSync(filePath, 'utf8');
  const firstLines = content.split('\n').slice(0, 12).join('\n');

  const match = firstLines.match(COPYRIGHT_RE);
  if (!match) {
    return { valid: false, reason: 'No copyright header found' };
  }

  if (!firstLines.includes(EXPECTED_PATTERN)) {
    return { valid: false, reason: `Expected ${EXPECTED_PATTERN}, found ${match[0]}` };
  }

  if (!firstLines.includes('Duskcue Contributors')) {
    return { valid: false, reason: 'Expected owner "Duskcue Contributors"' };
  }

  return { valid: true };
}

function main() {
  const files = FILE_PATTERNS.flatMap(pattern =>
    fs.globSync(pattern, { exclude: isIgnored })
  );
  const errors = [];

  files.forEach(file => {
    const result = checkFile(file);
    if (!result.valid) {
      errors.push(`${file}: ${result.reason}`);
    }
  });

  if (errors.length > 0) {
    console.error(`\nCopyright compliance check FAILED\n`);
    console.error(`Found ${errors.length} file(s) with outdated/missing copyright headers:\n`);
    errors.forEach(err => console.error(`  - ${err}`));
    console.error(`\nRun: node scripts/update-copyright.mjs\n`);
    process.exit(1);
  }

  console.log(`Copyright compliance check PASSED (${files.length} files checked)`);
}

main();
