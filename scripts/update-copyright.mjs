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
const COPYRIGHT_YEAR = `2026-${CURRENT_YEAR}`;

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

const FULL_HEADER_LICENSE = `GNU Affero General Public License as published by
 * the Free Software Foundation, either version 3 of the License, or
 * (at your option) any later version.`;

const HEADERS = {
  rs: `// Duskcue — Self-hosted media streaming server
// Copyright (C) ${COPYRIGHT_YEAR} Duskcue Contributors
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

`,
  js: `/*
 * Duskcue — Self-hosted media streaming server
 * Copyright (C) ${COPYRIGHT_YEAR} Duskcue Contributors
 *
 * This program is free software: you can redistribute it and/or modify
 * it under the terms of the GNU Affero General Public License as published by
 * the Free Software Foundation, either version 3 of the License, or
 * (at your option) any later version.
 *
 * This program is distributed in the hope that it will be useful,
 * but WITHOUT ANY WARRANTY; without even the implied warranty of
 * MERCHANTABILITY or FITNESS FOR A PARTICULAR PURPOSE. See the
 * GNU Affero General Public License for more details.
 *
 * You should have received a copy of the GNU Affero General Public License
 * along with this program. If not, see <https://www.gnu.org/licenses/>.
 */

`,
  svelte: `<!--
  Duskcue — Self-hosted media streaming server
  Copyright (C) ${COPYRIGHT_YEAR} Duskcue Contributors

  This program is free software: licensed under AGPL-3.0
  See LICENSE file for details.
-->

`,
  html: `<!--
  Duskcue — Self-hosted media streaming server
  Copyright (C) ${COPYRIGHT_YEAR} Duskcue Contributors

  This program is free software: licensed under AGPL-3.0
  See LICENSE file for details.
-->

`,
  css: `/*
 * Duskcue — Self-hosted media streaming server
 * Copyright (C) ${COPYRIGHT_YEAR} Duskcue Contributors
 *
 * This program is free software: licensed under AGPL-3.0
 * See LICENSE file for details.
 */

`,
  sql: `-- Duskcue — Self-hosted media streaming server
-- Copyright (C) ${COPYRIGHT_YEAR} Duskcue Contributors
--
-- This program is free software: you can redistribute it and/or modify
-- it under the terms of the GNU Affero General Public License as published by
-- the Free Software Foundation, either version 3 of the License, or
-- (at your option) any later version.
--
-- This program is distributed in the hope that it will be useful,
-- but WITHOUT ANY WARRANTY; without even the implied warranty of
-- MERCHANTABILITY or FITNESS FOR A PARTICULAR PURPOSE. See the
-- GNU Affero General Public License for more details.
--
-- You should have received a copy of the GNU Affero General Public License
-- along with this program. If not, see <https://www.gnu.org/licenses/>.

`,
  sh: `#!/usr/bin/env bash
# Duskcue — Self-hosted media streaming server
# Copyright (C) ${COPYRIGHT_YEAR} Duskcue Contributors
#
# This program is free software: you can redistribute it and/or modify
# it under the terms of the GNU Affero General Public License as published by
# the Free Software Foundation, either version 3 of the License, or
# (at your option) any later version.
#
# This program is distributed in the hope that it will be useful,
# but WITHOUT ANY WARRANTY; without even the implied warranty of
# MERCHANTABILITY or FITNESS FOR A PARTICULAR PURPOSE. See the
# GNU Affero General Public License for more details.
#
# You should have received a copy of the GNU Affero General Public License
# along with this program. If not, see <https://www.gnu.org/licenses/>.

`,
  dart: `// Duskcue — Self-hosted media streaming server
// Copyright (C) ${COPYRIGHT_YEAR} Duskcue Contributors
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

`,
  mjs: `/*
 * Duskcue — Self-hosted media streaming server
 * Copyright (C) ${COPYRIGHT_YEAR} Duskcue Contributors
 *
 * This program is free software: licensed under AGPL-3.0
 * See LICENSE file for details.
 */

`,
};

const COPYRIGHT_RE = /Copyright \(C\) (\d{4}|\d{4}-\d{4}) Duskcue Contributors/g;

function getExt(filePath) {
  return filePath.split('.').pop();
}

function processFile(filePath) {
  const content = fs.readFileSync(filePath, 'utf8');

  if (!content.includes('Copyright (C)')) {
    const ext = getExt(filePath);
    const header = HEADERS[ext] || HEADERS.js;

    let newContent;
    if (content.startsWith('#!')) {
      if (ext === 'sh') {
        newContent = HEADERS.sh;
        const withoutShebang = content.replace(/^#!\/[^\n]*\n\n?/, '');
        newContent += withoutShebang;
      } else {
        const eol = content.indexOf('\n');
        if (eol !== -1) {
          newContent = content.substring(0, eol + 1) + header + content.substring(eol + 1);
        } else {
          newContent = content + '\n' + header;
        }
      }
    } else {
      newContent = header + content;
    }

    fs.writeFileSync(filePath, newContent, 'utf8');
    return 'added';
  }

  const updated = content.replace(COPYRIGHT_RE, `Copyright (C) ${COPYRIGHT_YEAR} Duskcue Contributors`);
  if (updated !== content) {
    fs.writeFileSync(filePath, updated, 'utf8');
    return 'updated';
  }

  return null;
}

function main() {
  console.log(`\nUpdating copyright headers to ${COPYRIGHT_YEAR}...\n`);

  const files = FILE_PATTERNS.flatMap(pattern =>
    fs.globSync(pattern, { exclude: isIgnored })
  );

  let added = 0;
  let updated = 0;

  files.forEach(file => {
    const result = processFile(file);
    if (result === 'added') {
      console.log(`  + ${file}`);
      added++;
    } else if (result === 'updated') {
      console.log(`  ~ ${file}`);
      updated++;
    }
  });

  console.log(`\nDone. Added headers to ${added} file(s), updated year/owner in ${updated} file(s)\n`);
}

main();
