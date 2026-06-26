#!/usr/bin/env node
import fs from 'node:fs';
import path from 'node:path';
import {
  generatedRoot,
  packageDir,
  platforms,
  readCargoVersion,
  repoRoot,
  wrapperPackage,
} from './lib/release-config.mjs';

const version = readCargoVersion();

function writeJson(filePath, value) {
  fs.writeFileSync(filePath, `${JSON.stringify(value, null, 2)}\n`);
}

function ensureFile(filePath, help) {
  if (!fs.existsSync(filePath)) {
    throw new Error(`Missing ${filePath}\n${help}`);
  }
}

function copyExecutable(source, destination) {
  fs.copyFileSync(source, destination);
  fs.chmodSync(destination, 0o755);
}

function repositoryMetadata() {
  return {
    type: 'git',
    url: 'git+https://github.com/owenqwenstarsky/cassady.git',
  };
}

function packageHomepage() {
  return 'https://github.com/owenqwenstarsky/cassady#readme';
}

function writePlatformPackage(platform) {
  const dir = packageDir(platform);
  const binDir = path.join(dir, 'bin');
  fs.mkdirSync(binDir, { recursive: true });

  for (const command of ['cass', 'cassady']) {
    const fileName = `${command}${platform.exeSuffix}`;
    const source = path.join(repoRoot, 'target', platform.triple, 'release', fileName);
    const destination = path.join(binDir, fileName);
    ensureFile(
      source,
      `Build release binaries first. For ${platform.triple}, run the release build command from AGENTS.md.`
    );
    copyExecutable(source, destination);
  }

  writeJson(path.join(dir, 'package.json'), {
    name: platform.name,
    version,
    description: platform.description,
    license: 'MIT',
    homepage: packageHomepage(),
    repository: repositoryMetadata(),
    os: [platform.os],
    cpu: [platform.cpu],
    files: ['bin'],
  });

  fs.writeFileSync(
    path.join(dir, 'README.md'),
    `# ${platform.name}\n\n${platform.description}\n\nThis package is installed automatically by the \`cassady\` npm package on supported platforms. It contains the Rust-built \`cass\` and \`cassady\` executables for \`${platform.triple}\`.\n`
  );
}

function writeWrapperPackage() {
  const dir = packageDir(wrapperPackage);
  fs.mkdirSync(path.join(dir, 'bin'), { recursive: true });
  fs.mkdirSync(path.join(dir, 'lib'), { recursive: true });

  const optionalDependencies = Object.fromEntries(
    platforms.map((platform) => [platform.name, version])
  );

  writeJson(path.join(dir, 'package.json'), {
    name: wrapperPackage.name,
    version,
    description: 'Cassady/Cass minimal terminal coding agent',
    license: 'MIT',
    homepage: packageHomepage(),
    repository: repositoryMetadata(),
    bin: {
      cass: 'bin/cass.js',
      cassady: 'bin/cassady.js',
    },
    optionalDependencies,
    engines: {
      node: '>=16',
    },
    files: ['bin', 'lib'],
  });

  fs.writeFileSync(
    path.join(dir, 'README.md'),
    `# Cassady / Cass\n\nCassady (\`cass\`) is a terminal coding agent written in Rust. This npm package is a tiny launcher that installs the matching platform-specific binary package and exposes both commands:\n\n\`\`\`sh\nnpm install -g cassady\ncass --version\ncassady --version\n\`\`\`\n\nThe platform-specific packages contain the Rust-built executables. Supported npm platforms are macOS Apple Silicon, Linux x86_64, Linux ARM64, and Windows x86_64.\n`
  );

  const launcher = `'use strict';

const fs = require('node:fs');
const path = require('node:path');
const { spawnSync } = require('node:child_process');

const packagesByPlatform = {
  'darwin-arm64': '@cassady/cli-darwin-arm64',
  'linux-x64': '@cassady/cli-linux-x64',
  'linux-arm64': '@cassady/cli-linux-arm64',
  'win32-x64': '@cassady/cli-win32-x64',
};

function packageNameForCurrentPlatform() {
  const key = process.platform + '-' + process.arch;
  const packageName = packagesByPlatform[key];
  if (!packageName) {
    throw new Error(
      'Unsupported platform for Cassady: ' + process.platform + ' ' + process.arch +
      '. Supported platforms: ' + Object.keys(packagesByPlatform).join(', ')
    );
  }
  return packageName;
}

function binaryPath(command) {
  const packageName = packageNameForCurrentPlatform();
  let packageJsonPath;
  try {
    packageJsonPath = require.resolve(packageName + '/package.json');
  } catch (error) {
    throw new Error(
      'Cassady binary package was not installed: ' + packageName + '.\\n' +
      'Try reinstalling with npm install -g cassady and make sure optional dependencies are enabled.\\n' +
      'Original error: ' + error.message
    );
  }

  const executable = process.platform === 'win32' ? command + '.exe' : command;
  const resolved = path.join(path.dirname(packageJsonPath), 'bin', executable);
  if (!fs.existsSync(resolved)) {
    throw new Error('Cassady executable is missing from ' + packageName + ': ' + resolved);
  }
  return resolved;
}

function run(command) {
  let executable;
  try {
    executable = binaryPath(command);
  } catch (error) {
    console.error(error.message);
    process.exit(1);
  }

  const result = spawnSync(executable, process.argv.slice(2), { stdio: 'inherit' });
  if (result.error) {
    console.error(result.error.message);
    process.exit(1);
  }
  if (result.signal) {
    process.kill(process.pid, result.signal);
    return;
  }
  process.exit(result.status == null ? 1 : result.status);
}

module.exports = { run };
`;

  fs.writeFileSync(path.join(dir, 'lib', 'run.js'), launcher);

  for (const command of ['cass', 'cassady']) {
    const scriptPath = path.join(dir, 'bin', `${command}.js`);
    fs.writeFileSync(scriptPath, `#!/usr/bin/env node\nrequire('../lib/run').run('${command}');\n`);
    fs.chmodSync(scriptPath, 0o755);
  }
}

fs.rmSync(generatedRoot, { recursive: true, force: true });
fs.mkdirSync(generatedRoot, { recursive: true });

for (const platform of platforms) {
  writePlatformPackage(platform);
}
writeWrapperPackage();

console.log(`Prepared npm packages in ${generatedRoot}`);
console.log(`Version: ${version}`);
console.log('Packages:');
for (const platform of platforms) {
  console.log(`- ${platform.name}`);
}
console.log(`- ${wrapperPackage.name}`);
