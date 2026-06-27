import fs from 'node:fs';
import os from 'node:os';
import path from 'node:path';
import { spawnSync } from 'node:child_process';
import { fileURLToPath } from 'node:url';

export const scriptsDir = path.resolve(path.dirname(fileURLToPath(import.meta.url)), '..');
export const repoRoot = path.resolve(scriptsDir, '..', '..');
export const generatedRoot = path.join(repoRoot, 'dist', 'npm');

export const wrapperPackage = {
  name: 'cassady',
  dirName: 'cassady',
};

export const platforms = [
  {
    name: '@cassady/cli-darwin-arm64',
    dirName: 'cassady-cli-darwin-arm64',
    description: 'Cassady CLI and desktop binaries for macOS Apple Silicon.',
    triple: 'aarch64-apple-darwin',
    os: 'darwin',
    cpu: 'arm64',
    exeSuffix: '',
  },
  {
    name: '@cassady/cli-linux-x64',
    dirName: 'cassady-cli-linux-x64',
    description: 'Cassady CLI and desktop binaries for Linux x86_64.',
    triple: 'x86_64-unknown-linux-gnu',
    os: 'linux',
    cpu: 'x64',
    exeSuffix: '',
  },
  {
    name: '@cassady/cli-linux-arm64',
    dirName: 'cassady-cli-linux-arm64',
    description: 'Cassady CLI and desktop binaries for Linux ARM64.',
    triple: 'aarch64-unknown-linux-gnu',
    os: 'linux',
    cpu: 'arm64',
    exeSuffix: '',
  },
  {
    name: '@cassady/cli-win32-x64',
    dirName: 'cassady-cli-win32-x64',
    description: 'Cassady CLI binaries for Windows x86_64.',
    triple: 'x86_64-pc-windows-gnu',
    os: 'win32',
    cpu: 'x64',
    exeSuffix: '.exe',
  },
];

export function readCargoVersion() {
  const cargoToml = fs.readFileSync(path.join(repoRoot, 'Cargo.toml'), 'utf8');
  const match = cargoToml.match(/^version\s*=\s*"([^"]+)"/m);
  if (!match) {
    throw new Error('Could not read package.version from Cargo.toml');
  }
  return match[1];
}

export function packageDir(packageInfo) {
  return path.join(generatedRoot, packageInfo.dirName);
}

export function run(command, args, options = {}) {
  const result = spawnSync(command, args, {
    cwd: options.cwd ?? repoRoot,
    stdio: options.stdio ?? 'inherit',
    env: options.env ?? process.env,
    shell: false,
  });

  if (result.error) {
    throw result.error;
  }

  if (result.status !== 0) {
    const rendered = [command, ...args].join(' ');
    throw new Error(`${rendered} exited with status ${result.status}`);
  }

  return result;
}

export function npmArgsForPublish(packageInfo, { dryRun = false, tag = 'latest' } = {}) {
  const args = ['publish', '--tag', tag];
  if (packageInfo.name.startsWith('@')) {
    args.push('--access', 'public');
  }
  if (dryRun) {
    args.push('--dry-run');
  }
  return args;
}

export function npmPackVersion(packageName, version) {
  const tempDir = fs.mkdtempSync(path.join(os.tmpdir(), 'cassady-npm-pack-'));
  const cacheDir = fs.mkdtempSync(path.join(os.tmpdir(), 'cassady-npm-cache-'));
  try {
    return spawnSync(
      'npm',
      [
        'pack',
        `${packageName}@${version}`,
        '--dry-run',
        '--json',
        '--registry=https://registry.npmjs.org/',
        '--prefer-online',
        '--cache',
        cacheDir,
      ],
      {
        cwd: tempDir,
        encoding: 'utf8',
        stdio: ['ignore', 'pipe', 'pipe'],
        shell: false,
      }
    );
  } finally {
    fs.rmSync(tempDir, { recursive: true, force: true });
    fs.rmSync(cacheDir, { recursive: true, force: true });
  }
}
