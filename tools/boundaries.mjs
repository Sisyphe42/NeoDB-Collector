import assert from 'node:assert/strict';
import { execFileSync } from 'node:child_process';
import { readdir, readFile } from 'node:fs/promises';
import { join } from 'node:path';

const metadata = JSON.parse(execFileSync('cargo', ['metadata', '--format-version', '1', '--no-deps', '--locked'], { encoding: 'utf8' }));
const allowed = {
  'collector-domain': ['serde', 'schemars', 'utoipa'],
  'collector-core': ['collector-domain'],
};
for (const [name, dependencies] of Object.entries(allowed)) {
  const pkg = metadata.packages.find(p => p.name === name);
  assert(pkg, `Missing ${name}`);
  for (const dependency of pkg.dependencies.filter(d => d.kind !== 'dev')) {
    assert(dependencies.includes(dependency.name), `${name} cannot depend on ${dependency.name}`);
  }
}
async function scan(directory, visit) {
  for (const entry of await readdir(directory, { withFileTypes: true })) {
    if (['node_modules', 'dist', 'generated'].includes(entry.name)) continue;
    const path = join(directory, entry.name);
    if (entry.isDirectory()) await scan(path, visit);
    else await visit(path);
  }
}
for (const root of ['apps', 'packages']) {
  await scan(root, async path => {
    if (path.endsWith('package.json')) {
      const pkg = JSON.parse(await readFile(path, 'utf8'));
      assert.equal(pkg.collectorCoreApi, 'v1', `${path}: missing API compatibility`);
      for (const dependency of Object.keys(pkg.dependencies ?? {})) {
        assert(!/collector-(core|server|storage|domain)/.test(dependency), `${path}: Rust internals dependency`);
      }
    }
    if (/\.[cm]?[jt]sx?$/.test(path)) {
      const source = await readFile(path, 'utf8');
      assert(!/(?:from\s*|import\s*\(|require\s*\()\s*['"][^'"]*(?:crates\/|collector-(?:core|server|storage|domain))/.test(source), `${path}: cross-process import`);
    }
  });
}
console.log('Dependency boundaries verified.');
