import fs from 'node:fs';
import path from 'node:path';
import { spawnSync } from 'node:child_process';
import { fileURLToPath } from 'node:url';

const root = path.resolve(path.dirname(fileURLToPath(import.meta.url)), '..');
const fixtureDir = path.join(root, 'docs', 'api', 'fixtures', 'client-ci', 'v1');
const manifest = readJson(path.join(fixtureDir, 'manifest.json'));
const plan = readJson(path.join(fixtureDir, 'harness-plan.json'));
const seedProfile = readJson(path.join(fixtureDir, 'seed-data-profile.json'));
const args = new Set(process.argv.slice(2));

const runMode = args.has('--run');
const planMode = args.has('--plan') || !runMode;
const keepDeployment = args.has('--keep');
const baseUrl = valueArg('--base-url') ?? manifest.docker_target.public_base_url;
const timeoutSeconds = Number(valueArg('--timeout-seconds') ?? 180);

if (args.has('--help')) {
  console.log(`Usage:
  node scripts/client-smoke-harness.mjs --plan
  node scripts/client-smoke-harness.mjs --run [--keep] [--base-url http://127.0.0.1:48027] [--timeout-seconds 180]

The --plan mode validates and prints the reusable client CI smoke plan.
The --run mode starts docker compose, seeds representative media, probes :48027, runs contract verifiers, and tears down.`);
  process.exit(0);
}

if (planMode) {
  printPlan();
  process.exit(0);
}

let composeStarted = false;
try {
  seedRepresentativeData();
  runCommand('docker', ['compose', 'version']);
  runCommand('docker', ['compose', 'up', '-d'], {
    ...process.env,
    DUSKCUE_HOST_BIND: '127.0.0.1',
    DUSKCUE_PORT: String(manifest.docker_target.default_port)
  });
  composeStarted = true;

  await waitForReadiness(`${baseUrl}/health/ready`, timeoutSeconds);
  await probePublicSurface(baseUrl);
  runContractVerifiers();
  console.log(`Client smoke harness passed against ${baseUrl}.`);
} finally {
  if (composeStarted && !keepDeployment) {
    runCommand('docker', ['compose', 'down'], process.env, { allowFailure: true });
  } else if (composeStarted) {
    console.log('Keeping docker compose deployment for debugging.');
  }
}

function printPlan() {
  console.log(`Client CI smoke harness plan: ${manifest.harness.plan_command}`);
  console.log(`Docker target: ${manifest.docker_target.public_base_url}`);
  for (const step of plan.steps) {
    console.log(`- ${step.id}: ${step.command}`);
  }
  console.log('Verifier commands:');
  for (const command of plan.contract_verifier_commands) {
    console.log(`- ${command}`);
  }
}

function seedRepresentativeData() {
  for (const mediaRoot of seedProfile.media_roots) {
    const targetDir = path.join(root, mediaRoot.relative_path);
    fs.mkdirSync(targetDir, { recursive: true });
    for (const file of mediaRoot.files) {
      const targetFile = path.join(targetDir, file.name);
      if (!fs.existsSync(targetFile)) {
        fs.writeFileSync(targetFile, `${file.content}\n`);
      }
    }
  }
}

async function waitForReadiness(url, timeout) {
  const deadline = Date.now() + timeout * 1000;
  let lastError = 'not attempted';
  while (Date.now() < deadline) {
    try {
      const response = await fetchWithTimeout(url, 5000);
      if (response.ok) {
        return;
      }
      lastError = `HTTP ${response.status}`;
    } catch (error) {
      lastError = error.message;
    }
    await new Promise((resolve) => setTimeout(resolve, 2000));
  }
  throw new Error(`Timed out waiting for readiness at ${url}: ${lastError}`);
}

async function probePublicSurface(targetBaseUrl) {
  for (const check of plan.public_surface_checks) {
    const url = `${targetBaseUrl}${check.path}`;
    const response = await fetchWithTimeout(url, 5000);
    if (check.allowed_status && !check.allowed_status.includes(response.status)) {
      throw new Error(`${check.id} returned HTTP ${response.status}, expected ${check.allowed_status.join(', ')}`);
    }
    if (check.forbidden_status_range === '500-599' && response.status >= 500) {
      throw new Error(`${check.id} returned server error HTTP ${response.status}`);
    }
  }
}

function runContractVerifiers() {
  for (const command of plan.contract_verifier_commands) {
    const [bin, ...commandArgs] = command.split(' ');
    runCommand(bin, commandArgs);
  }
}

async function fetchWithTimeout(url, timeoutMs) {
  const controller = new AbortController();
  const timer = setTimeout(() => controller.abort(), timeoutMs);
  try {
    return await fetch(url, { signal: controller.signal });
  } finally {
    clearTimeout(timer);
  }
}

function runCommand(command, commandArgs, env = process.env, options = {}) {
  const result = spawnSync(command, commandArgs, {
    cwd: root,
    env,
    stdio: 'inherit',
    shell: process.platform === 'win32'
  });
  if (result.status !== 0 && !options.allowFailure) {
    throw new Error(`${command} ${commandArgs.join(' ')} failed with exit code ${result.status}`);
  }
}

function valueArg(name) {
  const index = process.argv.indexOf(name);
  if (index === -1) {
    return undefined;
  }
  return process.argv[index + 1];
}

function readJson(filePath) {
  return JSON.parse(fs.readFileSync(filePath, 'utf8'));
}
