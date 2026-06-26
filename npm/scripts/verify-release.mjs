#!/usr/bin/env node
import path from 'node:path';
import {
  npmArgsForPublish,
  packageDir,
  platforms,
  readCargoVersion,
  run,
  scriptsDir,
  wrapperPackage,
} from './lib/release-config.mjs';

const version = readCargoVersion();
const packages = [...platforms, wrapperPackage];

run(process.execPath, [path.join(scriptsDir, 'prepare-release.mjs')]);

console.log(`\nVerifying npm packages for ${version} with npm publish --dry-run...`);
for (const packageInfo of packages) {
  console.log(`\n==> ${packageInfo.name}`);
  run('npm', npmArgsForPublish(packageInfo, { dryRun: true }), {
    cwd: packageDir(packageInfo),
  });
}

console.log('\nNPM package verification completed. No packages were published.');
