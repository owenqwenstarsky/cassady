#!/usr/bin/env node
import path from 'node:path';
import { spawnSync } from 'node:child_process';
import {
  npmArgsForPublish,
  npmPackVersion,
  packageDir,
  platforms,
  readCargoVersion,
  run,
  scriptsDir,
  wrapperPackage,
} from './lib/release-config.mjs';

const args = new Set(process.argv.slice(2));
const dryRun = args.has('--dry-run');
const skipPrepare = args.has('--skip-prepare');
const tag = process.env.NPM_TAG || 'latest';
const version = readCargoVersion();
const packages = [...platforms, wrapperPackage];

function npmWhoami() {
  return spawnSync('npm', ['whoami'], {
    encoding: 'utf8',
    stdio: ['ignore', 'pipe', 'pipe'],
    shell: false,
  });
}

function ensureNpmAuth() {
  const whoami = npmWhoami();
  if (whoami.status === 0) {
    console.log(`Authenticated to npm as ${whoami.stdout.trim()}`);
    return;
  }

  console.log('Not authenticated to npm. Starting `npm login`...');
  run('npm', ['login']);

  const afterLogin = npmWhoami();
  if (afterLogin.status !== 0) {
    throw new Error('npm login completed, but `npm whoami` still failed.');
  }
  console.log(`Authenticated to npm as ${afterLogin.stdout.trim()}`);
}

function isNotFound(result) {
  const output = `${result.stdout ?? ''}\n${result.stderr ?? ''}`;
  return result.status !== 0 && /E404|404 Not Found|not found/i.test(output);
}

function publishedVersionExists(packageName) {
  const result = npmPackVersion(packageName, version);
  if (result.status === 0) {
    return true;
  }
  if (isNotFound(result)) {
    return false;
  }
  const output = `${result.stdout ?? ''}${result.stderr ?? ''}`.trim();
  throw new Error(`Could not query npm for ${packageName}@${version}:\n${output}`);
}

if (!skipPrepare) {
  run(process.execPath, [path.join(scriptsDir, 'prepare-release.mjs')]);
}

ensureNpmAuth();

console.log(`\nChecking npm registry for ${version}...`);
for (const packageInfo of packages) {
  if (publishedVersionExists(packageInfo.name)) {
    console.log(`- ${packageInfo.name}@${version} already exists; it will be skipped.`);
  } else {
    console.log(`- ${packageInfo.name}@${version} is available.`);
  }
}

console.log(`\n${dryRun ? 'Dry-run publishing' : 'Publishing'} npm packages with tag "${tag}"...`);
for (const packageInfo of packages) {
  if (!dryRun && publishedVersionExists(packageInfo.name)) {
    console.log(`\n==> Skipping ${packageInfo.name}@${version}; already published.`);
    continue;
  }

  console.log(`\n==> ${packageInfo.name}`);
  run('npm', npmArgsForPublish(packageInfo, { dryRun, tag }), {
    cwd: packageDir(packageInfo),
  });
}

if (dryRun) {
  console.log('\nDry run completed. No packages were published.');
} else {
  console.log('\nVerifying published npm packages...');
  for (const packageInfo of packages) {
    const result = npmPackVersion(packageInfo.name, version);
    if (result.status !== 0) {
      const output = `${result.stdout ?? ''}${result.stderr ?? ''}`.trim();
      throw new Error(`Published package not visible on npm yet: ${packageInfo.name}@${version}\n${output}`);
    }
    console.log(`- ${packageInfo.name}@${version}`);
  }
  console.log(`\nPublished Cassady ${version} to npm.`);
}
