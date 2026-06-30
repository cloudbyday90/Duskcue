import fs from 'node:fs';
import path from 'node:path';
import { fileURLToPath } from 'node:url';

const root = path.resolve(path.dirname(fileURLToPath(import.meta.url)), '..');
const manifestPath = path.join(root, 'docs', 'api', 'client-contracts.v1.json');
const manifest = JSON.parse(fs.readFileSync(manifestPath, 'utf8'));

function readFiles(dir, extensions) {
  const results = [];
  for (const entry of fs.readdirSync(dir, { withFileTypes: true })) {
    const fullPath = path.join(dir, entry.name);
    if (entry.isDirectory()) {
      results.push(...readFiles(fullPath, extensions));
    } else if (extensions.includes(path.extname(entry.name))) {
      results.push(fs.readFileSync(fullPath, 'utf8'));
    }
  }
  return results;
}

const serverSource = readFiles(path.join(root, 'server', 'src'), ['.rs']).join('\n');
const webApiSource = readFiles(path.join(root, 'clients', 'web', 'src', 'lib', 'api'), ['.js']).join('\n');
const seenRoutes = new Set();
const failures = [];

for (const domain of manifest.domains ?? []) {
  for (const route of domain.routes ?? []) {
    const key = `${route.method} ${route.path}`;
    if (seenRoutes.has(key)) {
      failures.push(`Duplicate route in manifest: ${key}`);
    }
    seenRoutes.add(key);

    const serverPath = route.server_path ?? route.path;
    if (!serverSource.includes(`"${serverPath}"`)) {
      failures.push(`Missing server route for ${key}`);
    }

    for (const helper of route.web_helpers ?? []) {
      if (!webApiSource.includes(`function ${helper}`)) {
        failures.push(`Missing web API helper ${helper} for ${key}`);
      }
    }
  }
}

if (failures.length > 0) {
  for (const failure of failures) {
    console.error(failure);
  }
  process.exit(1);
}

console.log(`Verified ${seenRoutes.size} client contract routes.`);
