import assert from 'node:assert/strict';
import { existsSync, readFileSync, readdirSync } from 'node:fs';
import { execFileSync } from 'node:child_process';
import { dirname, resolve } from 'node:path';
import { fileURLToPath } from 'node:url';

const root = resolve(dirname(fileURLToPath(import.meta.url)), '..');
const required = [
  'README.md', 'LICENSE', '.gitignore', 'docs/research.md',
  'docs/architecture.md', 'docs/roadmap.md',
  'examples/workspace.example.json', 'scripts/Probe-Environment.ps1',
];
for (const path of required) assert.ok(existsSync(resolve(root, path)), 'Missing: ' + path);

const exampleText = readFileSync(resolve(root, 'examples/workspace.example.json'), 'utf8');
const example = JSON.parse(exampleText);
assert.equal(example.schemaVersion, 1);
const ids = new Set();
for (const item of [...example.installations, ...example.devices, ...example.accounts]) {
  assert.ok(!ids.has(item.id), 'Duplicate identifier: ' + item.id);
  ids.add(item.id);
}
const installationIds = new Set(example.installations.map(item => item.id));
const deviceIds = new Set(example.devices.map(item => item.id));
for (const item of example.devices) {
  assert.ok(installationIds.has(item.installationId), 'Unknown installation reference');
}
for (const item of example.accounts) {
  assert.equal(item.roleId, null, 'Public examples must use placeholder role IDs');
  if (item.deviceId) assert.ok(deviceIds.has(item.deviceId), 'Unknown device reference');
  if (item.toolInstallationId) assert.ok(installationIds.has(item.toolInstallationId), 'Unknown tool reference');
}
assert.deepEqual(new Set(example.accounts.map(item => item.game)), new Set([
  'arknights', 'reverse1999', 'genshin', 'starrail', 'wuthering-waves',
]));
assert.ok(!/[A-Z]:\\|\/Users\/|\/home\//i.test(exampleText), 'Public example includes a local path');
assert.ok(!/"(?:cookie|token|password|stoken|ltoken|did)"\s*:/i.test(exampleText), 'Credential field in public example');

function walk(directory) {
  return readdirSync(directory, { withFileTypes: true }).flatMap(entry => {
    if (['.git', '.local', 'node_modules', 'dist', 'target'].includes(entry.name)) return [];
    const path = resolve(directory, entry.name);
    return entry.isDirectory() ? walk(path) : [path];
  });
}
const files = walk(root);
for (const path of files.filter(file => file.endsWith('.md'))) {
  const markdown = readFileSync(path, 'utf8');
  for (const match of markdown.matchAll(/\[[^\]]*\]\(([^)]+)\)/g)) {
    const target = match[1];
    if (/^(?:https?:|mailto:|#)/.test(target)) continue;
    const local = decodeURIComponent(target.split('#')[0]);
    assert.ok(existsSync(resolve(dirname(path), local)), 'Broken local link in ' + path + ': ' + target);
  }
}

if (existsSync(resolve(root, '.git'))) {
  const tracked = execFileSync('git', ['ls-files', '-z'], { cwd: root, encoding: 'utf8' }).split('\0').filter(Boolean);
  for (const path of tracked) {
    assert.ok(!/(^|\/)(?:\.local|credentials|secrets)(\/|$)|(^|\/)\.env(?:\.|$)|\.(?:db|sqlite3?|log|dmp)$/i.test(path),
      'Private or generated file is tracked: ' + path);
    const content = readFileSync(resolve(root, path), 'utf8');
    assert.ok(!/\b(?:ghp_|github_pat_|gho_)[A-Za-z0-9_]{20,}|-----BEGIN (?:RSA |OPENSSH |EC )?PRIVATE KEY-----/.test(content),
      'Possible secret in tracked file: ' + path);
  }
}
console.log('Repository checks passed: references, placeholders, local links and tracked-file hygiene.');
