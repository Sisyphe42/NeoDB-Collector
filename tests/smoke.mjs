import assert from 'node:assert/strict';
import { spawn, spawnSync } from 'node:child_process';
import { once } from 'node:events';
import { mkdtemp, mkdir, rm } from 'node:fs/promises';
import { tmpdir } from 'node:os';
import { join, resolve } from 'node:path';
import { chromium } from '@playwright/test';

const directory = await mkdtemp(join(tmpdir(), 'collector-smoke-'));
const binary = resolve(`target/debug/collector${process.platform === 'win32' ? '.exe' : ''}`);
const env = Object.fromEntries(Object.entries(process.env).filter(([key]) => !key.startsWith('NEODB_COLLECTOR_')));
const args = ['--data-dir', directory, '--assets-dir', resolve('apps/webui/dist'), '--port', '0'];
let child;
let browser;
try {
  const doctor = spawnSync(binary, ['doctor', '--format', 'json', ...args], { env, encoding: 'utf8' });
  assert.equal(doctor.status, 0, doctor.stderr || doctor.stdout);
  assert.equal(JSON.parse(doctor.stdout).ok, true);
  const invalid = spawnSync(binary, ['doctor', '--format', 'json', '--bind', '0.0.0.0', ...args], { env, encoding: 'utf8' });
  assert.equal(invalid.status, 1);
  assert.equal(JSON.parse(invalid.stdout).checks[0].code, 'config_invalid');
  child = spawn(binary, ['serve', ...args], { env, stdio: ['ignore', 'ignore', 'pipe'] });
  const address = await new Promise((accept, reject) => {
    let output = '';
    const timer = setTimeout(() => reject(new Error(`Core startup timed out: ${output}`)), 15_000);
    child.once('error', error => { clearTimeout(timer); reject(error); });
    child.once('exit', code => { clearTimeout(timer); reject(new Error(`Core exited: ${code}; ${output}`)); });
    child.stderr.on('data', chunk => {
      output += chunk;
      const match = output.match(/Core listening on (http:\/\/[^\s]+)/);
      if (match) { clearTimeout(timer); accept(match[1]); }
    });
  });
  const health = await fetch(`${address}/api/v1/health`).then(r => r.json());
  assert.equal(health.status, 'ok');
  const capabilities = await fetch(`${address}/api/v1/capabilities`).then(r => r.json());
  assert.deepEqual(capabilities.connectorIds, []);
  assert.equal((await fetch(`${address}/api/v1/imports`)).status, 404);
  assert.equal((await fetch(`${address}/api/v1/health`, { headers: { Origin: 'https://example.com' } })).status, 403);
  const conflict = spawnSync(binary, ['serve', ...args.slice(0, -2), '--port', new URL(address).port], { env, encoding: 'utf8', timeout: 5000 });
  assert.equal(conflict.status, 1);
  assert.match(conflict.stderr, /port_unavailable/);
  browser = await chromium.launch();
  const page = await browser.newPage({ viewport: { width: 1280, height: 900 } });
  const errors = [];
  page.on('pageerror', error => errors.push(error.message));
  await page.goto(address);
  await page.getByRole('heading', { name: '本地服务已连接' }).waitFor();
  assert.equal(await page.getByText('尚无可用的平台连接').count(), 1);
  await page.getByRole('button', { name: '重新检查' }).click();
  await page.getByRole('heading', { name: '本地服务已连接' }).waitFor();
  await mkdir('test-results', { recursive: true });
  await page.screenshot({ path: 'test-results/status-desktop.png', fullPage: true });
  await page.setViewportSize({ width: 390, height: 844 });
  assert(await page.evaluate(() => document.documentElement.scrollWidth <= innerWidth));
  await page.screenshot({ path: 'test-results/status-mobile.png', fullPage: true });
  assert.equal(await page.evaluate(() => localStorage.length), 0);
  assert.deepEqual(errors, []);
  console.log('Actual CLI, HTTP, desktop/mobile browser smoke passed.');
} finally {
  await browser?.close();
  if (child && child.exitCode === null) {
    const exited = once(child, 'exit');
    child.kill('SIGTERM');
    const timer = setTimeout(() => child.kill('SIGKILL'), 5000);
    await exited;
    clearTimeout(timer);
  }
  // Only remove the exact directory allocated by this test.
  assert(directory.startsWith(join(tmpdir(), 'collector-smoke-')));
  await rm(directory, { recursive: true, force: true });
}
