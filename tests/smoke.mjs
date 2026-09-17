import assert from 'node:assert/strict';
import { spawn, spawnSync } from 'node:child_process';
import { once } from 'node:events';
import { mkdtemp, mkdir, readFile, rm } from 'node:fs/promises';
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
  const badPair = spawnSync(binary, ['pair', '--format', 'json'], { env, input: 'not-a-pairing-link', encoding: 'utf8' });
  assert.equal(badPair.status, 1);
  assert.equal(JSON.parse(badPair.stdout).code, 'core_url_invalid');
  assert(!badPair.stdout.includes('not-a-pairing-link'));
  const badStatus = spawnSync(binary, ['status', '--core-url', 'https://example.com', '--format', 'json'], { env, encoding: 'utf8' });
  assert.equal(badStatus.status, 1);
  assert.equal(JSON.parse(badStatus.stdout).code, 'core_url_invalid');
  const doctor = spawnSync(binary, ['doctor', '--format', 'json', ...args], { env, encoding: 'utf8' });
  assert.equal(doctor.status, 0, doctor.stderr || doctor.stdout);
  assert.equal(JSON.parse(doctor.stdout).ok, true);
  const invalid = spawnSync(binary, ['doctor', '--format', 'json', '--bind', '0.0.0.0', ...args], { env, encoding: 'utf8' });
  assert.equal(invalid.status, 1);
  assert.equal(JSON.parse(invalid.stdout).checks[0].code, 'config_invalid');
  child = spawn(binary, ['serve', ...args], { env, stdio: ['ignore', 'ignore', 'pipe'] });
  const bootstrapUrl = await new Promise((accept, reject) => {
    let output = '';
    const timer = setTimeout(() => reject(new Error('Core startup timed out.')), 15_000);
    child.once('error', error => { clearTimeout(timer); reject(error); });
    child.once('exit', code => { clearTimeout(timer); reject(new Error(`Core exited: ${code}`)); });
    child.stderr.on('data', chunk => {
      output += chunk;
      // stderr can split a single formatted line into multiple chunks.
      const match = output.match(/Open WebUI \(one use, 10 minutes\): (http:\/\/[^\s]+)\r?\n/);
      if (match) { clearTimeout(timer); accept(match[1]); }
    });
  });
  const address = new URL(bootstrapUrl).origin;
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
  // OAuth UI fixture only: never invoke the user's native keychain in smoke.
  await page.route('**/api/v1/settings/neodb', route => route.fulfill({
    contentType: 'application/json', body: JSON.stringify({ connected: false, baseUrl: null, account: null, pending: false }),
  }));
  page.on('pageerror', error => errors.push(error.message));
  await page.goto(bootstrapUrl);
  await page.getByRole('heading', { name: '本地服务已连接' }).waitFor();
  assert.equal(await page.getByText('尚无可用的平台连接').count(), 1);
  await page.getByText('已授权', { exact: true }).waitFor();
  assert.equal(new URL(page.url()).hash, '');
  assert.equal(await page.evaluate(() => document.cookie), '');
  const cookies = await page.context().cookies();
  assert(cookies.some(cookie => cookie.name === 'collector_session' && cookie.httpOnly && cookie.sameSite === 'Strict'));
  // Real local mapping API/UI and SQLite only; no remote catalog requests.
  await page.getByRole('button', { name: '使用 IMDb 默认规则' }).click();
  const complete = page.getByLabel('评分即看完（需主动开启）');
  await complete.waitFor();
  assert.equal(await complete.isChecked(), false);
  await complete.check();
  await page.getByRole('button', { name: '保存禁用版本' }).click();
  await page.getByText('已保存 v1（禁用）；未执行任何媒体同步。', { exact: true }).waitFor();
  await page.getByRole('button', { name: '导出表单为 YAML' }).click();
  await page.getByText('表单已导出到下方 YAML 文本，可复制或下载。', { exact: true }).waitFor();
  await page.locator('.mapping-document summary').click();
  const exportedYaml = await page.getByLabel('配置文本', { exact: true }).inputValue();
  assert.match(exportedYaml, /schemaVersion: 1/);
  const downloaded = page.waitForEvent('download');
  await page.getByRole('button', { name: '下载文本' }).click();
  const download = await downloaded;
  assert.equal(download.suggestedFilename(), 'mapping.yaml');
  assert.equal(await readFile(await download.path(), 'utf8'), exportedYaml);
  await page.getByLabel('从文件读取').setInputFiles({ name: 'mapping.yaml', mimeType: 'text/yaml', buffer: Buffer.from(exportedYaml) });
  await page.getByText('文件已读入文本区；验证并导入后才会替换表单。', { exact: true }).waitFor();
  await page.getByRole('button', { name: '验证并导入文本' }).click();
  await page.getByText('已验证并导入到表单，尚未保存；保存版本由已读取的版本决定。', { exact: true }).waitFor();
  assert.equal(await complete.isChecked(), true);
  await page.getByRole('button', { name: '保存禁用版本' }).click();
  await page.getByText('已保存 v2（禁用）；未执行任何媒体同步。', { exact: true }).waitFor();
  await page.getByRole('button', { name: '生成配对链接' }).click();
  const linkField = page.getByLabel('一次性链接 · 10 分钟内有效');
  await linkField.waitFor();
  const pairingUrl = await linkField.inputValue();
  const oneTimeKey = new URLSearchParams(new URL(pairingUrl).hash.slice(1)).get('key');
  const paired = await fetch(`${address}/api/v1/pairings/exchange`, { method: 'POST', headers: { 'Content-Type': 'application/json' }, body: JSON.stringify({ one_time_key: oneTimeKey, client_name: 'Collector CLI', client_kind: 'cli' }) });
  assert.equal(paired.status, 200);
  const pairedToken = await paired.json();
  const identity = await fetch(`${address}/api/v1/client`, { headers: { Authorization: `Bearer ${pairedToken.token}` } });
  assert.equal(identity.status, 200);
  assert.equal((await identity.json()).id, pairedToken.client_id);
  assert.equal((await fetch(`${address}/api/v1/clients`, { headers: { Authorization: `Bearer ${pairedToken.token}` } })).status, 403);
  await page.getByRole('button', { name: '隐藏链接' }).click();
  await page.getByRole('button', { name: '刷新列表' }).click();
  await page.getByRole('button', { name: '撤销 Collector CLI' }).click();
  await page.getByText('cli · 已撤销', { exact: true }).waitFor();
  assert.equal((await fetch(`${address}/api/v1/client`, { headers: { Authorization: `Bearer ${pairedToken.token}` } })).status, 401);
  await page.getByRole('button', { name: '重新检查' }).click();
  await page.getByRole('heading', { name: '本地服务已连接' }).waitFor();
  await page.getByRole('button', { name: '读取并替换表单' }).click();
  await page.getByText('已读取 v2；替换了上方表单，配置文本仍保留。', { exact: true }).waitFor();
  await mkdir('test-results', { recursive: true });
  await page.screenshot({ path: 'test-results/status-desktop.png', fullPage: true });
  await page.setViewportSize({ width: 390, height: 844 });
  assert(await page.evaluate(() => document.documentElement.scrollWidth <= innerWidth));
  await page.screenshot({ path: 'test-results/status-mobile.png', fullPage: true });
  assert.equal(await page.evaluate(() => localStorage.length), 0);
  assert.equal(await page.evaluate(() => sessionStorage.length), 0);
  await page.reload();
  await page.getByText('已授权', { exact: true }).waitFor();
  await page.getByRole('button', { name: '读取并替换表单' }).click();
  await page.getByText('已读取 v2；替换了上方表单，配置文本仍保留。', { exact: true }).waitFor();
  assert.equal(await page.getByLabel('评分即看完（需主动开启）').isChecked(), true);
  await page.getByRole('button', { name: '退出本地授权' }).click();
  await page.getByText('未授权', { exact: true }).waitFor();
  assert.equal((await page.request.get(`${address}/api/v1/session`)).status(), 401);
  assert.deepEqual(errors, []);
  console.log('Actual CLI, HTTP, session/pairing/revocation and desktop/mobile browser smoke passed.');
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
