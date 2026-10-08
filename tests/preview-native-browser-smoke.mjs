// Browser acceptance against the ignored Rust host: actual built WebUI -> generated
// client -> Core HTTP/runtime/SQLite -> production NeoDbClient -> synthetic upstream.
import assert from 'node:assert/strict';
import { spawn } from 'node:child_process';
import { mkdir, readFile } from 'node:fs/promises';
import { pathToFileURL } from 'node:url';
import Ajv2020 from 'ajv/dist/2020.js';
import { chromium } from '@playwright/test';

const HOST_TEST = 'client::preview_integration_tests::browser::browser_native_preview_host';
const FAILURE_HOST_TEST = 'client::preview_integration_tests::browser::failure::browser_native_failure_host';
const POLLING_HOST_TEST = 'client::preview_integration_tests::browser::polling::browser_native_polling_host';
const RESOLUTION_HOST_TEST = 'client::preview_integration_tests::browser::resolution::browser_native_resolution_host';
const HOST_PREFIX = 'E5F:';
const PROCESS_TIMEOUT_MS = 180_000;
const PHASE_TIMEOUT_MS = 30_000;
const EXIT_TIMEOUT_MS = 8_000;
const MAX_LINE_BYTES = 64 * 1024;
const FORBIDDEN_PUBLIC_VALUES = [
  'fixture-neodb-credential',
  'neodb-fixture-token-e5a',
  'DISCARDED_RAW_COLUMN_E5A',
  'DISCARDED_UPSTREAM_PAYLOAD_E5A',
  'DISCARDED_RAW_FAILURE_E5B',
  'DISCARDED_UPSTREAM_FAILURE_E5B',
  'DISCARDED_RAW_RESOLUTION_E5C',
  'DISCARDED_UPSTREAM_RESOLUTION_E5C',
];
const CSV = '\uFEFFConst,Your Rating,Title Type,Title,Raw Column\r\n'
  + 'tt1111111,8,movie,Equal,DISCARDED_RAW_COLUMN_E5A\r\n'
  + 'tt2222222,7,movie,中文标题,DISCARDED_RAW_COLUMN_E5A\r\n'
  + 'tt3333333,6,movie,Missing,DISCARDED_RAW_COLUMN_E5A\r\n'
  + 'tt3333333,6,movie,Duplicate,DISCARDED_RAW_COLUMN_E5A\r\n'
  + 'tt4444444,9,tvEpisode,Skipped,DISCARDED_RAW_COLUMN_E5A\r\n';
const FAILURE_CSV = '\uFEFFConst,Your Rating,Title Type,Title,Raw Column\r\n'
  + 'tt1111111,8,movie,Equal,DISCARDED_RAW_FAILURE_E5B\r\n'
  + 'tt2222222,7,movie,Fault,DISCARDED_RAW_FAILURE_E5B\r\n';
const POLLING_CSV = '\uFEFFConst,Your Rating,Title Type,Title,Raw Column\r\n'
  + 'tt4444444,8,movie,Polling,DISCARDED_RAW_RESOLUTION_E5C\r\n';
const RESOLUTION_CSV = '\uFEFFConst,Your Rating,Title Type,Title,Raw Column\r\n'
  + 'tt5555555,8,movie,中文候选,DISCARDED_RAW_RESOLUTION_E5C\r\n';
const FAILURE_CASES = [
  'fetch_422',
  'fetch_429',
  'fetch_foreign_redirect',
  'mark_401',
  'mark_404_item',
  'mark_hang',
];
const POLLING_CASES = ['poll_complete', 'poll_cancel'];
const RESOLUTION_CASES = ['manual_missing_id', 'manual_conflict_id'];

const ajv = new Ajv2020({ strict: true });
for (const format of ['uint32', 'int64', 'double']) ajv.addFormat(format, true);
const validators = Object.fromEntries(await Promise.all([
  'PreviewImportAccepted',
  'PreviewJobView',
  'PreviewResultPage',
  'PreviewMutationResult',
  'MappingProfile',
  'PreviewImdbResolutionRequest',
  'PreviewResolutionChoice',
].map(async name => [name, ajv.compile(JSON.parse(await readFile(
  new URL(`../contracts/json-schema/${name}.json`, import.meta.url), 'utf8',
)))])));

function validate(name, value) {
  const validator = validators[name];
  assert(validator(value), `${name} did not match its committed generated schema`);
  return value;
}

function assertPublic(value) {
  const encoded = JSON.stringify(value);
  for (const forbidden of FORBIDDEN_PUBLIC_VALUES) assert(!encoded.includes(forbidden));
}

function processExit(child) {
  return new Promise(resolve => {
    let error;
    child.once('error', cause => { error = cause; });
    child.once('close', (code, signal) => resolve({ code, signal, error }));
  });
}

async function waitForExit(child, exit, timeoutMs = EXIT_TIMEOUT_MS) {
  let timer;
  const timeout = new Promise(resolve => { timer = setTimeout(() => resolve(undefined), timeoutMs); });
  const result = await Promise.race([exit, timeout]);
  clearTimeout(timer);
  if (result) return { ...result, forced: false };
  if (child.exitCode === null && child.signalCode === null) child.kill('SIGKILL');
  let forceTimer;
  const forced = await Promise.race([
    exit,
    new Promise(resolve => { forceTimer = setTimeout(() => resolve(undefined), EXIT_TIMEOUT_MS); }),
  ]);
  clearTimeout(forceTimer);
  assert(forced, 'native preview host did not exit after bounded force termination');
  return { ...forced, forced: true };
}

function createLineReader(stream, onLine) {
  let buffered = Buffer.alloc(0);
  stream.on('data', chunk => {
    buffered = Buffer.concat([buffered, chunk]);
    while (true) {
      const newline = buffered.indexOf(0x0a);
      if (newline < 0) break;
      const line = buffered.subarray(0, Math.min(newline, MAX_LINE_BYTES)).toString('utf8').replace(/\r$/, '');
      buffered = buffered.subarray(newline + 1);
      onLine(line);
    }
    if (buffered.length > MAX_LINE_BYTES) buffered = Buffer.alloc(0);
  });
  stream.resume();
}

function startHost(executable, testName = HOST_TEST) {
  const child = spawn(executable, ['--ignored', '--exact', testName, '--nocapture'], {
    stdio: ['pipe', 'pipe', 'pipe'],
    windowsHide: true,
  });
  child.stdin.on('error', () => {});
  const exit = processExit(child);
  const queued = [];
  const waiters = [];
  createLineReader(child.stdout, line => {
    const marker = line.indexOf(HOST_PREFIX);
    if (marker < 0) return;
    const encoded = line.slice(marker + HOST_PREFIX.length);
    if (Buffer.byteLength(encoded) > MAX_LINE_BYTES) return;
    let value;
    try { value = JSON.parse(encoded); } catch { return; }
    const waiter = waiters.shift();
    if (waiter) waiter(value);
    else queued.push(value);
  });
  // Drain diagnostics without retaining or surfacing possible secrets.
  child.stderr.resume();
  return {
    child,
    exit,
    send(value) {
      assert(child.stdin.writable, 'native preview host stdin is closed');
      child.stdin.write(`${JSON.stringify(value)}\n`);
    },
    async receive(phase, timeoutMs = PHASE_TIMEOUT_MS) {
      const next = queued.length ? Promise.resolve(queued.shift()) : new Promise(resolve => waiters.push(resolve));
      let timer;
      const timeout = new Promise((_, reject) => {
        timer = setTimeout(() => reject(new Error(`native preview host ${phase} phase timed out`)), timeoutMs);
      });
      const earlyExit = exit.then(() => { throw new Error(`native preview host exited before ${phase}`); });
      let value;
      try {
        value = await Promise.race([next, earlyExit, timeout]);
      } finally {
        clearTimeout(timer);
      }
      assert.equal(value?.phase, phase, `native preview host returned an unexpected phase before ${phase}`);
      return value;
    },
    async closeInputAndDrain() {
      if (!child.stdin.destroyed) child.stdin.end();
      return waitForExit(child, exit);
    },
  };
}

async function cargoTestExecutable() {
  const child = spawn('cargo', [
    'test', '--no-run', '--locked', '-p', 'collector-cli', '--message-format=json',
  ], { stdio: ['ignore', 'pipe', 'pipe'], windowsHide: true });
  const exit = processExit(child);
  let executable;
  createLineReader(child.stdout, line => {
    let message;
    try { message = JSON.parse(line); } catch { return; }
    if (message.reason === 'compiler-artifact'
      && message.profile?.test === true
      && message.target?.name === 'collector'
      && message.target?.kind?.includes('bin')
      && typeof message.executable === 'string') executable = message.executable;
  });
  child.stderr.resume();
  let timer;
  const result = await Promise.race([
    exit,
    new Promise(resolve => { timer = setTimeout(() => resolve(undefined), PROCESS_TIMEOUT_MS); }),
  ]);
  clearTimeout(timer);
  if (!result) {
    if (child.exitCode === null && child.signalCode === null) child.kill('SIGKILL');
    await waitForExit(child, exit);
    assert.fail('Cargo test artifact discovery timed out');
  }
  assert.equal(result.error, undefined, 'could not start Cargo test artifact discovery');
  assert.equal(result.code, 0, 'Cargo test artifact discovery failed');
  assert(executable, 'Cargo did not report the collector test executable');
  return executable;
}

function coreRequestRecord(request, origin) {
  const url = new URL(request.url());
  if (url.origin !== origin || !url.pathname.startsWith('/api/v1/')) return undefined;
  return {
    method: request.method(),
    path: `${url.pathname}${url.search}`,
    // headers() omits security-related headers; retain the full browser trace.
    headers: request.allHeaders().catch(() => undefined),
  };
}

async function responseJson(response, schema) {
  const value = await response.json();
  validate(schema, value);
  assertPublic(value);
  return value;
}

function assertComparedResultItem(item, job, spec) {
  assert.equal(item.ordinal, spec.ordinal);
  assert.equal(item.attempt, 1);
  assert.equal(item.committedAt, 1_800_000_000);
  assert.equal(item.sourceConnectorId, 'imdb:ratings_csv');
  assert.equal(item.sourceRecordId, spec.sourceId);
  assert.match(item.sourceFingerprint, /^imdb:ratings_csv:[0-9a-f]{64}$/);
  assert.equal(item.profileId, 'imdb_ratings');
  assert.equal(item.profileVersion, 1);
  assert.equal(item.baseUrl, job.baseUrl);
  assert.equal(item.account, 'alice');
  assert.equal(item.historical, true);
  assert.equal(item.executable, false);
  assert.equal(item.outcome.kind, 'compared');
  assert.equal(item.outcome.provenance, 'automatic');
  assert.equal(item.outcome.observedAt, 1_800_000_000);
  assert.equal(item.outcome.item.uuid, spec.uuid);
  assert.equal(item.outcome.item.canonicalUrl, `${job.baseUrl}/movie/${spec.uuid}`);
  assert.equal(item.outcome.item.detailUrl, `${job.baseUrl}/api/movie/${spec.uuid}`);
  assert.equal(item.outcome.item.mediaType, 'movie');
  assert.equal(item.outcome.item.model, 'Movie');
  assert.equal(item.outcome.item.parentUuid, null);
  assert.deepEqual(item.outcome.item.externalIds, [{ scheme: 'imdb:title', value: spec.sourceId }]);
  assert.equal(item.outcome.comparison.profileId, 'imdb_ratings');
  assert.equal(item.outcome.comparison.profileVersion, 1);
  const rating = item.outcome.comparison.fields.find(field => field.ruleId === 'rating');
  assert(rating);
  assert.equal(rating.field, 'rating_grade');
  assert.equal(rating.suggestion, spec.rating);
  assert.deepEqual(rating.sink, spec.sink);
  assert.equal(rating.status, spec.status);
  assert.equal(rating.reason, spec.reason);
}

function assertResultPage(page, job, offset, expected) {
  validate('PreviewResultPage', page);
  assertPublic(page);
  assert.equal(page.jobId, job.id);
  assert.equal(page.attempt, 1);
  assert.equal(page.offset, offset);
  assert.equal(page.limit, 2);
  assert.equal(page.historical, true);
  assert.equal(page.executable, false);
  assert.equal(page.items.length, expected.length);
  for (let index = 0; index < page.items.length; index += 1) {
    assertComparedResultItem(page.items[index], job, expected[index]);
  }
}

async function clickForResponse(page, button, predicate) {
  const pending = page.waitForResponse(response => predicate(new URL(response.url()), response.request().method()));
  const [response] = await Promise.all([pending, button.click()]);
  return response;
}

async function waitForPolledPage(page, jobId, offset, initialPage, count, maxReads = 3) {
  let current = initialPage;
  for (let read = 0; read < maxReads && current.items.length !== count; read += 1) {
    const response = await page.waitForResponse(candidate => {
      const url = new URL(candidate.url());
      return url.pathname === `/api/v1/previews/${jobId}/results`
        && url.searchParams.get('offset') === String(offset)
        && url.searchParams.get('limit') === '2';
    }, { timeout: 12_000 });
    current = await responseJson(response, 'PreviewResultPage');
  }
  assert.equal(current.items.length, count, `preview page at ordinal ${offset} did not converge to ${count} items`);
  return current;
}

async function runHappyPath(browser, executable) {
  const host = startHost(executable);
  let context;
  let bootstrapKey;
  try {
    const ready = await host.receive('ready');
    assert.equal(typeof ready.bootstrapUrl, 'string');
    const bootstrapUrl = new URL(ready.bootstrapUrl);
    const origin = bootstrapUrl.origin;
    bootstrapKey = new URLSearchParams(bootstrapUrl.hash.slice(1)).get('key');
    assert(bootstrapKey);

    context = await browser.newContext({ viewport: { width: 1280, height: 900 } });
    const page = await context.newPage();
    const pageErrors = [];
    const requests = [];
    page.on('pageerror', error => pageErrors.push(error.message));
    page.on('request', request => {
      const record = coreRequestRecord(request, origin);
      if (record) requests.push(record);
    });

    await page.goto(bootstrapUrl.href);
    await page.getByRole('heading', { name: '本地服务已连接' }).waitFor();
    await page.getByText('已授权', { exact: true }).waitFor();
    assert.equal(new URL(page.url()).hash, '');
    assert.equal(await page.evaluate(() => document.cookie), '');
    const cookies = await context.cookies(origin);
    assert(cookies.some(cookie => cookie.name === 'collector_session'
      && cookie.httpOnly && cookie.sameSite === 'Strict'));
    await page.getByText(/alice · http:\/\/127\.0\.0\.1:/).waitFor();

    const panel = page.getByRole('region', { name: '历史预览' });
    await panel.getByLabel('保存的配置 ID', { exact: true }).fill('imdb_ratings');
    await panel.getByLabel('保存的配置版本', { exact: true }).fill('1');
    await panel.getByLabel('CSV 文件（UTF-8，最大 10 MiB）').setInputFiles({
      name: 'ratings-native.csv', mimeType: 'text/csv', buffer: Buffer.from(CSV),
    });
    const uploadResponse = await clickForResponse(
      page,
      panel.getByRole('button', { name: '提交只读预览', exact: true }),
      (url, method) => method === 'POST' && url.pathname === '/api/v1/imports/imdb/ratings',
    );
    assert.equal(uploadResponse.status(), 202);
    const receipt = await responseJson(uploadResponse, 'PreviewImportAccepted');
    assert.deepEqual({
      observations: receipt.observationCount,
      skippedRows: receipt.skippedRowCount,
      skippedColumns: receipt.skippedColumnCount,
      duplicates: receipt.duplicateRowCount,
    }, { observations: 3, skippedRows: 1, skippedColumns: 1, duplicates: 1 });
    assert.equal(receipt.preview.attempt, 1);
    assert.equal(receipt.preview.executable, false);
    const jobId = receipt.preview.jobId;

    await panel.getByRole('heading', { name: `任务 ${jobId}`, exact: true }).waitFor();
    await panel.getByText('running', { exact: true }).waitFor();
    const runningJobResponse = await clickForResponse(
      page,
      panel.getByRole('button', { name: '读取状态', exact: true }),
      url => url.pathname === `/api/v1/previews/${jobId}`,
    );
    const runningJob = await responseJson(runningJobResponse, 'PreviewJobView');
    assert.equal(runningJob.status, 'running');
    assert.equal(runningJob.profileVersion, 1);
    assert.equal(runningJob.observationCount, 3);
    await panel.getByLabel('起始记录序号').fill('0');
    await panel.getByLabel('单页数量').fill('2');
    const partialResponse = await clickForResponse(
      page,
      panel.getByRole('button', { name: '读取该尝试的一页结果', exact: true }),
      url => url.pathname === `/api/v1/previews/${jobId}/results` && url.searchParams.get('offset') === '0',
    );
    const initialPartialPage = await responseJson(partialResponse, 'PreviewResultPage');
    const firstSpecs = [
      { ordinal: 0, sourceId: 'tt1111111', uuid: 'Movie1111111', rating: 8, sink: { kind: 'known', value: 8 }, status: 'skipped', reason: 'already_equal' },
      { ordinal: 1, sourceId: 'tt2222222', uuid: 'Movie2222222', rating: 7, sink: { kind: 'known', value: 5 }, status: 'needs_review', reason: 'sink_conflict_needs_review' },
    ];
    const partialPage = await waitForPolledPage(page, jobId, 0, initialPartialPage, 2);
    assertResultPage(partialPage, runningJob, 0, firstSpecs);
    const partialResults = panel.getByRole('region', { name: '历史结果页' });
    await partialResults.locator('.preview-result').nth(1).waitFor();
    for (const text of ['Known · 5', 'sink_conflict_needs_review', '自动', '来源指纹']) {
      assert(await partialResults.getByText(text, { exact: false }).count());
    }

    host.send({ phase: 'partial', jobId });
    await host.receive('released');

    await panel.getByText('completed', { exact: true }).waitFor({ timeout: 12_000 });
    const completedResponse = await clickForResponse(
      page,
      panel.getByRole('button', { name: '读取状态', exact: true }),
      url => url.pathname === `/api/v1/previews/${jobId}`,
    );
    const completedJob = await responseJson(completedResponse, 'PreviewJobView');
    assert.equal(completedJob.status, 'completed');
    assert.deepEqual({
      attempt: completedJob.attempt,
      observations: completedJob.observationCount,
      profile: `${completedJob.profileId}:${completedJob.profileVersion}`,
      account: completedJob.account,
      binding: completedJob.bindingState,
      created: completedJob.createdAt,
      updated: completedJob.updatedAt,
    }, {
      attempt: 1, observations: 3, profile: 'imdb_ratings:1', account: 'alice',
      binding: 'current', created: 1_800_000_000, updated: 1_800_000_000,
    });
    assert.equal(completedJob.expiresAt, 1_802_592_000);
    assert.equal(completedJob.historical, true);
    assert.equal(completedJob.executable, false);

    const firstResponse = await clickForResponse(
      page,
      panel.getByRole('button', { name: '读取该尝试的一页结果', exact: true }),
      url => url.pathname === `/api/v1/previews/${jobId}/results` && url.searchParams.get('offset') === '0',
    );
    const firstPage = await responseJson(firstResponse, 'PreviewResultPage');
    assertResultPage(firstPage, completedJob, 0, firstSpecs);
    await panel.getByLabel('起始记录序号').fill('2');
    const secondResponse = await clickForResponse(
      page,
      panel.getByRole('button', { name: '读取该尝试的一页结果', exact: true }),
      url => url.pathname === `/api/v1/previews/${jobId}/results` && url.searchParams.get('offset') === '2',
    );
    const secondPage = await responseJson(secondResponse, 'PreviewResultPage');
    assertResultPage(secondPage, completedJob, 2, [{
      ordinal: 2, sourceId: 'tt3333333', uuid: 'Movie3333333', rating: 6,
      sink: { kind: 'unknown' }, status: 'needs_review', reason: 'rating_requires_shelf',
    }]);

    for (const text of [
      '来源连接器', '来源记录 ID', '来源指纹', '身份声明（逐条保留）',
      '自动', '历史证据 · 不可执行', 'Unknown · 无法确认', 'rating_requires_shelf',
    ]) assert(await panel.getByText(text, { exact: false }).count());
    assert(await panel.getByText(/不会写入 NeoDB 个人标记/).count());
    assert.equal(await panel.getByLabel('解析选择草稿').count(), 1);

    host.send({ phase: 'retained', jobId });
    await host.receive('reopened');
    await page.reload();
    await page.getByText('已授权', { exact: true }).waitFor();
    assert.equal(new URL(page.url()).hash, '');
    assert.equal(await panel.getByLabel('已有任务 ID', { exact: true }).inputValue(), '');
    assert.equal(await panel.getByLabel('保存的配置 ID', { exact: true }).inputValue(), '');
    assert.equal(await panel.getByLabel('CSV 文件（UTF-8，最大 10 MiB）').inputValue(), '');
    await panel.getByLabel('已有任务 ID', { exact: true }).fill(jobId);
    const reopenedResponse = await clickForResponse(
      page,
      panel.getByRole('button', { name: '打开任务', exact: true }),
      url => url.pathname === `/api/v1/previews/${jobId}`,
    );
    const reopenedJob = await responseJson(reopenedResponse, 'PreviewJobView');
    assert.deepEqual(reopenedJob, completedJob);
    await panel.getByRole('heading', { name: `任务 ${jobId}`, exact: true }).waitFor();
    assert.equal(await panel.getByLabel('解析选择草稿').count(), 0);
    assert(await panel.getByRole('button', { name: '取消尝试 1', exact: true }).isDisabled());
    assert(await panel.getByRole('button', { name: '显式恢复尝试 1', exact: true }).isDisabled());

    await panel.getByLabel('起始记录序号').fill('0');
    await panel.getByLabel('单页数量').fill('2');
    const reopenedFirstResponse = await clickForResponse(
      page,
      panel.getByRole('button', { name: '读取该尝试的一页结果', exact: true }),
      url => url.pathname === `/api/v1/previews/${jobId}/results`,
    );
    const reopenedFirstPage = await responseJson(reopenedFirstResponse, 'PreviewResultPage');
    assert.deepEqual(reopenedFirstPage, firstPage);
    await panel.getByRole('region', { name: '历史结果页' }).waitFor();
    await panel.getByLabel('起始记录序号').fill('2');
    const reopenedSecondResponse = await clickForResponse(
      page,
      panel.getByRole('button', { name: '读取该尝试的一页结果', exact: true }),
      url => url.pathname === `/api/v1/previews/${jobId}/results` && url.searchParams.get('offset') === '2',
    );
    const reopenedSecondPage = await responseJson(reopenedSecondResponse, 'PreviewResultPage');
    assert.deepEqual(reopenedSecondPage, secondPage);
    assert.equal(requests.filter(request => request.method === 'POST'
      && request.path.startsWith('/api/v1/imports/imdb/ratings?')).length, 1);
    assert.equal(requests.filter(request => request.path.endsWith('/cancel') || request.path.endsWith('/resume')).length, 0);

    const visibleText = await page.locator('body').innerText();
    for (const forbidden of [...FORBIDDEN_PUBLIC_VALUES, bootstrapKey]) {
      assert(!visibleText.includes(forbidden));
    }

    await page.setViewportSize({ width: 1280, height: 900 });
    await page.screenshot({ path: 'test-results/preview-native-desktop.png', fullPage: true });
    assert(await page.evaluate(() => document.documentElement.scrollWidth <= innerWidth));
    await page.setViewportSize({ width: 390, height: 844 });
    await page.screenshot({ path: 'test-results/preview-native-mobile.png', fullPage: true });
    assert(await page.evaluate(() => document.documentElement.scrollWidth <= innerWidth));
    assert.equal(await page.evaluate(() => localStorage.length), 0);
    assert.equal(await page.evaluate(() => sessionStorage.length), 0);
    assert.equal(await page.getByText(/#key=/).count(), 0);
    assert.equal(await page.getByLabel('一次性链接 · 10 分钟内有效').count(), 0);

    page.once('dialog', dialog => dialog.accept());
    const deleteResponse = await clickForResponse(
      page,
      panel.getByRole('button', { name: '删除运行历史', exact: true }),
      (url, method) => method === 'DELETE' && url.pathname === `/api/v1/previews/${jobId}`,
    );
    assert.deepEqual(await responseJson(deleteResponse, 'PreviewMutationResult'), { applied: true });
    await panel.getByText('服务器确认运行历史已删除；公共目录条目不受影响。', { exact: true }).waitFor();
    const missing = await page.evaluate(async id => {
      const response = await fetch(`/api/v1/previews/${encodeURIComponent(id)}`);
      return { status: response.status, body: await response.json() };
    }, jobId);
    assert.equal(missing.status, 404);
    assert.equal(missing.body.code, 'preview_job_not_found');
    assertPublic(missing.body);
    const profileResponse = await clickForResponse(
      page,
      page.getByRole('button', { name: '读取并替换表单' }),
      url => url.pathname === '/api/v1/mapping-profiles/imdb_ratings',
    );
    const profile = await responseJson(profileResponse, 'MappingProfile');
    assert.equal(profile.version, 2);
    assert.equal(profile.enabled, false);
    assert.equal(profile.rules[0]?.operation.kind, 'clamp');
    assert.equal(profile.rules[0]?.operation.max, 5);
    await page.getByText('已读取 v2；替换了上方表单，配置文本仍保留。', { exact: true }).waitFor();
    await page.getByText(new RegExp(`alice.*${completedJob.baseUrl.replace(/[.*+?^${}()|[\]\\]/g, '\\$&')}`)).waitFor();

    const mutations = requests.filter(request => ['POST', 'PUT', 'PATCH', 'DELETE'].includes(request.method));
    const previewMutations = mutations.filter(request => request.path.startsWith('/api/v1/imports/imdb/ratings?')
      || request.path.startsWith(`/api/v1/previews/${jobId}`));
    assert.deepEqual(previewMutations.map(request => request.method), ['POST', 'DELETE']);
    assert.deepEqual(mutations.map(request => `${request.method} ${request.path}`), [
      'POST /api/v1/session/bootstrap',
      'POST /api/v1/imports/imdb/ratings?profileId=imdb_ratings&profileVersion=1',
      `DELETE /api/v1/previews/${jobId}`,
    ]);
    const headers = await Promise.all(requests.map(request => request.headers));
    for (const value of headers) {
      assert(value, 'browser request headers could not be read');
      assert.equal(value.authorization, undefined);
    }
    for (const request of previewMutations) {
      const value = await request.headers;
      assert.equal(value.origin, origin);
      assert(value['x-csrf-token']);
    }
    assert.deepEqual(pageErrors, []);

    host.send({ phase: 'finish', jobId });
    await host.receive('done');
    const result = await host.closeInputAndDrain();
    assert.equal(result.error, undefined, 'native preview host process failed to start');
    assert.equal(result.forced, false, 'native preview host exceeded its exit bound');
    assert.equal(result.signal, null, 'native preview host terminated by signal');
    assert.equal(result.code, 0, 'native preview host did not exit successfully');
    return { origin };
  } catch (error) {
    const result = await host.closeInputAndDrain();
    void result;
    const message = error instanceof Error ? error.message : 'native preview browser smoke failed';
    const safe = bootstrapKey ? message.replaceAll(bootstrapKey, '[redacted]') : message;
    // The original error may contain the private bootstrap URL, so it must not
    // be attached as Error.cause after constructing the redacted diagnostic.
    // eslint-disable-next-line preserve-caught-error
    throw new Error(safe.replace(/#key=[^\s"']+/g, '#key=[redacted]'));
  } finally {
    await context?.close();
  }
}

function assertFailureJob(job, expectedStatus, observationCount = 2) {
  validate('PreviewJobView', job);
  assertPublic(job);
  assert.equal(job.status, expectedStatus);
  assert.deepEqual({
    attempt: job.attempt,
    observations: job.observationCount,
    profile: `${job.profileId}:${job.profileVersion}`,
    account: job.account,
    binding: job.bindingState,
    created: job.createdAt,
    updated: job.updatedAt,
    expires: job.expiresAt,
    historical: job.historical,
    executable: job.executable,
  }, {
    attempt: 1,
    observations: observationCount,
    profile: 'imdb_ratings:1',
    account: 'alice',
    binding: 'current',
    created: 1_800_000_000,
    updated: 1_800_000_000,
    expires: 1_802_592_000,
    historical: true,
    executable: false,
  });
}

function assertFailureOutcomeItem(item, job, ordinal, sourceId) {
  assert.equal(item.ordinal, ordinal);
  assert.equal(item.attempt, 1);
  assert.equal(item.committedAt, 1_800_000_000);
  assert.equal(item.sourceConnectorId, 'imdb:ratings_csv');
  assert.equal(item.sourceRecordId, sourceId);
  assert.match(item.sourceFingerprint, /^imdb:ratings_csv:[0-9a-f]{64}$/);
  assert.equal(item.profileId, 'imdb_ratings');
  assert.equal(item.profileVersion, 1);
  assert.equal(item.baseUrl, job.baseUrl);
  assert.equal(item.account, 'alice');
  assert.equal(item.historical, true);
  assert.equal(item.executable, false);
}

function assertFailureTerminalPage(resultPage, job, failureCase) {
  validate('PreviewResultPage', resultPage);
  assertPublic(resultPage);
  assert.equal(resultPage.jobId, job.id);
  assert.equal(resultPage.attempt, 1);
  assert.equal(resultPage.offset, 0);
  assert.equal(resultPage.limit, 2);
  assert.equal(resultPage.historical, true);
  assert.equal(resultPage.executable, false);
  const completes = failureCase === 'fetch_422' || failureCase === 'fetch_429';
  assert.equal(resultPage.items.length, completes ? 2 : 1);
  const first = resultPage.items[0];
  assertComparedResultItem(first, job, {
    ordinal: 0,
    sourceId: 'tt1111111',
    uuid: 'Movie1111111',
    rating: 8,
    sink: { kind: 'known', value: 8 },
    status: 'skipped',
    reason: 'already_equal',
  });
  if (completes) {
    const second = resultPage.items[1];
    assertFailureOutcomeItem(second, job, 1, 'tt2222222');
    assert.equal(second.outcome.kind, failureCase === 'fetch_422' ? 'unsupported' : 'rate_limited');
  }
}

async function runFailureCase(browser, host, failureCase, bootstrapKeys) {
  let context;
  let bootstrapKey;
  try {
    const ready = await host.receive('ready');
    assert.equal(ready.case, failureCase);
    assert.equal(typeof ready.bootstrapUrl, 'string');
    const bootstrapUrl = new URL(ready.bootstrapUrl);
    const origin = bootstrapUrl.origin;
    bootstrapKey = new URLSearchParams(bootstrapUrl.hash.slice(1)).get('key');
    assert(bootstrapKey);
    bootstrapKeys.push(bootstrapKey);

    context = await browser.newContext({ viewport: { width: 1280, height: 900 } });
    const page = await context.newPage();
    const pageErrors = [];
    const requests = [];
    page.on('pageerror', error => pageErrors.push(error.message));
    page.on('request', request => {
      const record = coreRequestRecord(request, origin);
      if (record) requests.push(record);
    });

    await page.goto(bootstrapUrl.href);
    await page.getByRole('heading', { name: '本地服务已连接' }).waitFor();
    await page.getByText('已授权', { exact: true }).waitFor();
    assert.equal(new URL(page.url()).hash, '');
    assert.equal(await page.evaluate(() => document.cookie), '');
    const cookies = await context.cookies(origin);
    assert(cookies.some(cookie => cookie.name === 'collector_session'
      && cookie.httpOnly && cookie.sameSite === 'Strict'));
    await page.getByText(/alice · http:\/\/127\.0\.0\.1:/).waitFor();

    const panel = page.getByRole('region', { name: '历史预览' });
    await panel.getByLabel('保存的配置 ID', { exact: true }).fill('imdb_ratings');
    await panel.getByLabel('保存的配置版本', { exact: true }).fill('1');
    await panel.getByLabel('CSV 文件（UTF-8，最大 10 MiB）').setInputFiles({
      name: `ratings-${failureCase}.csv`, mimeType: 'text/csv', buffer: Buffer.from(FAILURE_CSV),
    });
    const preUpload = performance.now();
    const uploadResponse = await clickForResponse(
      page,
      panel.getByRole('button', { name: '提交只读预览', exact: true }),
      (url, method) => method === 'POST' && url.pathname === '/api/v1/imports/imdb/ratings',
    );
    assert.equal(uploadResponse.status(), 202);
    const receipt = await responseJson(uploadResponse, 'PreviewImportAccepted');
    assert.deepEqual({
      observations: receipt.observationCount,
      skippedRows: receipt.skippedRowCount,
      skippedColumns: receipt.skippedColumnCount,
      duplicates: receipt.duplicateRowCount,
      attempt: receipt.preview.attempt,
      executable: receipt.preview.executable,
    }, {
      observations: 2, skippedRows: 0, skippedColumns: 1, duplicates: 0,
      attempt: 1, executable: false,
    });
    const jobId = receipt.preview.jobId;
    await panel.getByRole('heading', { name: `任务 ${jobId}`, exact: true }).waitFor();

    if (failureCase === 'mark_hang') {
      await panel.getByText('running', { exact: true }).waitFor();
      const runningResponse = await clickForResponse(
        page,
        panel.getByRole('button', { name: '读取状态', exact: true }),
        url => url.pathname === `/api/v1/previews/${jobId}`,
      );
      const runningJob = await responseJson(runningResponse, 'PreviewJobView');
      assertFailureJob(runningJob, 'running');
      await panel.getByLabel('起始记录序号').fill('0');
      await panel.getByLabel('单页数量').fill('2');
      const partialResponse = await clickForResponse(
        page,
        panel.getByRole('button', { name: '读取该尝试的一页结果', exact: true }),
        url => url.pathname === `/api/v1/previews/${jobId}/results`
          && url.searchParams.get('offset') === '0',
      );
      const initialPartialPage = await responseJson(partialResponse, 'PreviewResultPage');
      const partialPage = await waitForPolledPage(page, jobId, 0, initialPartialPage, 1);
      assertFailureTerminalPage(partialPage, runningJob, failureCase);
      host.send({ phase: 'partial', jobId });
      const held = await host.receive('held');
      assert.equal(held.case, failureCase);
    }

    const expectedStatus = ['fetch_422', 'fetch_429'].includes(failureCase) ? 'completed' : 'failed';
    await panel.getByText(expectedStatus, { exact: true }).waitFor({ timeout: 20_000 });
    if (failureCase === 'mark_hang') {
      assert(performance.now() - preUpload >= 9_000, 'real ten-second native timeout did not drive mark_hang');
    }
    const terminalResponse = await clickForResponse(
      page,
      panel.getByRole('button', { name: '读取状态', exact: true }),
      url => url.pathname === `/api/v1/previews/${jobId}`,
    );
    const terminalJob = await responseJson(terminalResponse, 'PreviewJobView');
    assertFailureJob(terminalJob, expectedStatus);
    await page.getByText('已授权', { exact: true }).waitFor();
    assert.equal(await page.getByText(/本地授权已失效/).count(), 0);
    const resumeButton = panel.getByRole('button', { name: '显式恢复尝试 1', exact: true });
    if (expectedStatus === 'failed') assert(await resumeButton.isEnabled());
    else assert(await resumeButton.isDisabled());

    await panel.getByLabel('起始记录序号').fill('0');
    await panel.getByLabel('单页数量').fill('2');
    const terminalPageResponse = await clickForResponse(
      page,
      panel.getByRole('button', { name: '读取该尝试的一页结果', exact: true }),
      url => url.pathname === `/api/v1/previews/${jobId}/results`
        && url.searchParams.get('offset') === '0',
    );
    const terminalPage = await responseJson(terminalPageResponse, 'PreviewResultPage');
    assertFailureTerminalPage(terminalPage, terminalJob, failureCase);
    await panel.getByLabel('起始记录序号').fill('1');
    const ordinalOneResponse = await clickForResponse(
      page,
      panel.getByRole('button', { name: '读取该尝试的一页结果', exact: true }),
      url => url.pathname === `/api/v1/previews/${jobId}/results`
        && url.searchParams.get('offset') === '1',
    );
    const ordinalOnePage = await responseJson(ordinalOneResponse, 'PreviewResultPage');
    if (expectedStatus === 'failed') {
      assert.deepEqual({
        jobId: ordinalOnePage.jobId,
        attempt: ordinalOnePage.attempt,
        offset: ordinalOnePage.offset,
        limit: ordinalOnePage.limit,
        items: ordinalOnePage.items,
        historical: ordinalOnePage.historical,
        executable: ordinalOnePage.executable,
      }, { jobId, attempt: 1, offset: 1, limit: 2, items: [], historical: true, executable: false });
    } else {
      assert.equal(ordinalOnePage.jobId, jobId);
      assert.equal(ordinalOnePage.attempt, 1);
      assert.equal(ordinalOnePage.offset, 1);
      assert.equal(ordinalOnePage.limit, 2);
      assert.equal(ordinalOnePage.historical, true);
      assert.equal(ordinalOnePage.executable, false);
      assert.equal(ordinalOnePage.items.length, 1);
      assertFailureOutcomeItem(ordinalOnePage.items[0], terminalJob, 1, 'tt2222222');
      assert.equal(ordinalOnePage.items[0].outcome.kind,
        failureCase === 'fetch_422' ? 'unsupported' : 'rate_limited');
    }

    const results = panel.getByRole('region', { name: '历史结果页' });
    await panel.getByLabel('起始记录序号').fill('0');
    const visibleOutcomeResponse = await clickForResponse(
      page,
      panel.getByRole('button', { name: '读取该尝试的一页结果', exact: true }),
      url => url.pathname === `/api/v1/previews/${jobId}/results`
        && url.searchParams.get('offset') === '0',
    );
    assert.deepEqual(await responseJson(visibleOutcomeResponse, 'PreviewResultPage'), terminalPage);
    await results.locator('.preview-result').first().waitFor();
    if (expectedStatus === 'completed') {
      const secondResult = results.locator('.preview-result').nth(1);
      await secondResult.waitFor();
      await secondResult.getByText(
        failureCase === 'fetch_422' ? '结果：不支持' : '结果：目录服务限流',
        { exact: true },
      ).waitFor();
    }

    const visibleText = await page.locator('body').innerText();
    for (const forbidden of [...FORBIDDEN_PUBLIC_VALUES, bootstrapKey]) assert(!visibleText.includes(forbidden));
    await page.setViewportSize({ width: 1280, height: 900 });
    assert(await page.evaluate(() => document.documentElement.scrollWidth <= innerWidth));
    if (failureCase === 'fetch_422') {
      await page.screenshot({ path: 'test-results/preview-native-failure-terminal.png', fullPage: true });
    }
    if (failureCase === 'mark_401') {
      await page.screenshot({ path: 'test-results/preview-native-failure-desktop.png', fullPage: true });
    }
    await page.setViewportSize({ width: 390, height: 844 });
    assert(await page.evaluate(() => document.documentElement.scrollWidth <= innerWidth));
    if (failureCase === 'mark_404_item') {
      await page.screenshot({ path: 'test-results/preview-native-failure-mobile.png', fullPage: true });
    }
    assert.equal(await page.evaluate(() => localStorage.length), 0);
    assert.equal(await page.evaluate(() => sessionStorage.length), 0);
    assert.equal(await page.getByText(/#key=/).count(), 0);

    host.send({ phase: 'retained', jobId });
    const reopened = await host.receive('reopened');
    assert.equal(reopened.case, failureCase);
    await page.reload();
    await page.getByText('已授权', { exact: true }).waitFor();
    assert.equal(new URL(page.url()).hash, '');
    assert.equal(await panel.getByLabel('已有任务 ID', { exact: true }).inputValue(), '');
    assert.equal(await panel.getByLabel('保存的配置 ID', { exact: true }).inputValue(), '');
    assert.equal(await panel.getByLabel('CSV 文件（UTF-8，最大 10 MiB）').inputValue(), '');
    assert.equal(await panel.getByLabel('解析选择草稿').count(), 0);
    await panel.getByLabel('已有任务 ID', { exact: true }).fill(jobId);
    const reopenedResponse = await clickForResponse(
      page,
      panel.getByRole('button', { name: '打开任务', exact: true }),
      url => url.pathname === `/api/v1/previews/${jobId}`,
    );
    const reopenedJob = await responseJson(reopenedResponse, 'PreviewJobView');
    assert.deepEqual(reopenedJob, terminalJob);
    if (expectedStatus === 'failed') assert(await panel.getByRole('button', { name: '显式恢复尝试 1', exact: true }).isEnabled());
    await panel.getByLabel('起始记录序号').fill('0');
    await panel.getByLabel('单页数量').fill('2');
    const reopenedPageResponse = await clickForResponse(
      page,
      panel.getByRole('button', { name: '读取该尝试的一页结果', exact: true }),
      url => url.pathname === `/api/v1/previews/${jobId}/results`
        && url.searchParams.get('offset') === '0',
    );
    assert.deepEqual(await responseJson(reopenedPageResponse, 'PreviewResultPage'), terminalPage);
    await panel.getByLabel('起始记录序号').fill('1');
    const reopenedOrdinalOneResponse = await clickForResponse(
      page,
      panel.getByRole('button', { name: '读取该尝试的一页结果', exact: true }),
      url => url.pathname === `/api/v1/previews/${jobId}/results`
        && url.searchParams.get('offset') === '1',
    );
    assert.deepEqual(await responseJson(reopenedOrdinalOneResponse, 'PreviewResultPage'), ordinalOnePage);

    page.once('dialog', dialog => dialog.accept());
    const deleteResponse = await clickForResponse(
      page,
      panel.getByRole('button', { name: '删除运行历史', exact: true }),
      (url, method) => method === 'DELETE' && url.pathname === `/api/v1/previews/${jobId}`,
    );
    assert.deepEqual(await responseJson(deleteResponse, 'PreviewMutationResult'), { applied: true });
    await panel.getByText('服务器确认运行历史已删除；公共目录条目不受影响。', { exact: true }).waitFor();
    const missing = await page.evaluate(async id => {
      const response = await fetch(`/api/v1/previews/${encodeURIComponent(id)}`);
      return { status: response.status, body: await response.json() };
    }, jobId);
    assert.equal(missing.status, 404);
    assert.equal(missing.body.code, 'preview_job_not_found');
    assertPublic(missing.body);

    const mutations = requests.filter(request => ['POST', 'PUT', 'PATCH', 'DELETE'].includes(request.method));
    assert.deepEqual(mutations.map(request => `${request.method} ${request.path}`), [
      'POST /api/v1/session/bootstrap',
      'POST /api/v1/imports/imdb/ratings?profileId=imdb_ratings&profileVersion=1',
      `DELETE /api/v1/previews/${jobId}`,
    ]);
    const headers = await Promise.all(requests.map(request => request.headers));
    for (const value of headers) {
      assert(value, 'browser request headers could not be read');
      assert.equal(value.authorization, undefined);
    }
    const mutationHeaders = await Promise.all(mutations.map(request => request.headers));
    assert.equal(mutationHeaders[0].origin, origin);
    assert.equal(mutationHeaders[0]['x-csrf-token'], undefined);
    assert.equal(mutationHeaders[1].origin, origin);
    assert(mutationHeaders[1]['x-csrf-token']);
    assert.equal(mutationHeaders[2].origin, origin);
    assert.equal(mutationHeaders[2]['x-csrf-token'], mutationHeaders[1]['x-csrf-token']);
    assert.equal(requests.filter(request => request.path.endsWith('/resume')
      || request.path.endsWith('/cancel')).length, 0);
    assert.deepEqual(pageErrors, []);

    host.send({ phase: 'finish', jobId });
    const done = await host.receive('done');
    assert.equal(done.case, failureCase);
  } catch (error) {
    const message = error instanceof Error ? error.message : `native failure case ${failureCase} failed`;
    const safe = bootstrapKeys.reduce((value, key) => value.replaceAll(key, '[redacted]'), message);
    // eslint-disable-next-line preserve-caught-error
    throw new Error(safe.replace(/#key=[^\s"']+/g, '#key=[redacted]'));
  } finally {
    await context?.close();
  }
}

async function runFailurePath(browser, executable) {
  const host = startHost(executable, FAILURE_HOST_TEST);
  const bootstrapKeys = [];
  try {
    for (const failureCase of FAILURE_CASES) {
      await runFailureCase(browser, host, failureCase, bootstrapKeys);
    }
    await host.receive('all_done');
    const result = await waitForExit(host.child, host.exit);
    assert.equal(result.error, undefined, 'native failure host process failed to start');
    assert.equal(result.forced, false, 'native failure host exceeded its natural exit bound');
    assert.equal(result.signal, null, 'native failure host terminated by signal');
    assert.equal(result.code, 0, 'native failure host did not exit successfully');
  } catch (error) {
    if (host.child.exitCode === null && host.child.signalCode === null) await host.closeInputAndDrain();
    throw error;
  }
}

function assertEmptyPollingPage(resultPage, job, offset) {
  validate('PreviewResultPage', resultPage);
  assertPublic(resultPage);
  assert.deepEqual({
    jobId: resultPage.jobId,
    attempt: resultPage.attempt,
    offset: resultPage.offset,
    limit: resultPage.limit,
    items: resultPage.items,
    historical: resultPage.historical,
    executable: resultPage.executable,
  }, {
    jobId: job.id,
    attempt: 1,
    offset,
    limit: 2,
    items: [],
    historical: true,
    executable: false,
  });
}

async function readPollingPage(page, panel, jobId, offset) {
  await panel.getByLabel('起始记录序号').fill(String(offset));
  await panel.getByLabel('单页数量').fill('2');
  const response = await clickForResponse(
    page,
    panel.getByRole('button', { name: '读取该尝试的一页结果', exact: true }),
    url => url.pathname === `/api/v1/previews/${jobId}/results`
      && url.searchParams.get('offset') === String(offset)
      && url.searchParams.get('limit') === '2',
  );
  return responseJson(response, 'PreviewResultPage');
}

async function runPollingCase(browser, host, pollingCase, bootstrapKeys) {
  let context;
  let bootstrapKey;
  try {
    const ready = await host.receive('ready');
    assert.equal(ready.case, pollingCase);
    assert.equal(typeof ready.bootstrapUrl, 'string');
    const bootstrapUrl = new URL(ready.bootstrapUrl);
    const origin = bootstrapUrl.origin;
    bootstrapKey = new URLSearchParams(bootstrapUrl.hash.slice(1)).get('key');
    assert(bootstrapKey);
    bootstrapKeys.push(bootstrapKey);

    context = await browser.newContext({ viewport: { width: 1280, height: 900 } });
    const page = await context.newPage();
    const pageErrors = [];
    const requests = [];
    page.on('pageerror', error => pageErrors.push(error.message));
    page.on('request', request => {
      const record = coreRequestRecord(request, origin);
      if (record) requests.push(record);
    });

    await page.goto(bootstrapUrl.href);
    await page.getByRole('heading', { name: '本地服务已连接' }).waitFor();
    await page.getByText('已授权', { exact: true }).waitFor();
    assert.equal(new URL(page.url()).hash, '');
    assert.equal(await page.evaluate(() => document.cookie), '');
    const cookies = await context.cookies(origin);
    assert(cookies.some(cookie => cookie.name === 'collector_session'
      && cookie.httpOnly && cookie.sameSite === 'Strict'));
    await page.getByText(/alice · http:\/\/127\.0\.0\.1:/).waitFor();

    const panel = page.getByRole('region', { name: '历史预览' });
    await panel.getByText(/预览可能让 NeoDB 创建公共目录条目，但不会写入 NeoDB 个人标记/).waitFor();
    await panel.getByLabel('保存的配置 ID', { exact: true }).fill('imdb_ratings');
    await panel.getByLabel('保存的配置版本', { exact: true }).fill('1');
    await panel.getByLabel('CSV 文件（UTF-8，最大 10 MiB）').setInputFiles({
      name: `ratings-${pollingCase}.csv`, mimeType: 'text/csv', buffer: Buffer.from(POLLING_CSV),
    });
    const uploadStartedAt = performance.now();
    const uploadResponse = await clickForResponse(
      page,
      panel.getByRole('button', { name: '提交只读预览', exact: true }),
      (url, method) => method === 'POST' && url.pathname === '/api/v1/imports/imdb/ratings',
    );
    assert.equal(uploadResponse.status(), 202);
    const receipt = await responseJson(uploadResponse, 'PreviewImportAccepted');
    assert.deepEqual({
      observations: receipt.observationCount,
      skippedRows: receipt.skippedRowCount,
      skippedColumns: receipt.skippedColumnCount,
      duplicates: receipt.duplicateRowCount,
      attempt: receipt.preview.attempt,
      executable: receipt.preview.executable,
    }, {
      observations: 1, skippedRows: 0, skippedColumns: 1, duplicates: 0,
      attempt: 1, executable: false,
    });
    const jobId = receipt.preview.jobId;
    await panel.getByRole('heading', { name: `任务 ${jobId}`, exact: true }).waitFor();
    await panel.getByText('running', { exact: true }).waitFor();

    const runningResponse = await clickForResponse(
      page,
      panel.getByRole('button', { name: '读取状态', exact: true }),
      url => url.pathname === `/api/v1/previews/${jobId}`,
    );
    const runningJob = await responseJson(runningResponse, 'PreviewJobView');
    assert.equal(runningJob.id, jobId);
    assertFailureJob(runningJob, 'running', 1);
    assert(await panel.getByRole('button', { name: '取消尝试 1', exact: true }).isEnabled());
    assert(await panel.getByRole('button', { name: '显式恢复尝试 1', exact: true }).isDisabled());
    const pendingPage = await readPollingPage(page, panel, jobId, 0);
    assertEmptyPollingPage(pendingPage, runningJob, 0);
    await panel.getByText('本页为空；这不表示任务完成，也不表示后续没有稀疏结果。', { exact: true }).waitFor();

    host.send({ phase: 'pending', jobId });
    const pending = await host.receive('pending');
    assert.equal(pending.case, pollingCase);

    let polledTerminalPage;
    if (pollingCase === 'poll_complete') {
      [polledTerminalPage] = await Promise.all([
        waitForPolledPage(page, jobId, 0, pendingPage, 1, 6),
        (async () => {
          const pendingText = await page.locator('body').innerText();
          for (const forbidden of [...FORBIDDEN_PUBLIC_VALUES, bootstrapKey]) assert(!pendingText.includes(forbidden));
          await page.screenshot({ path: 'test-results/preview-native-polling-pending-desktop.png', fullPage: true });
          assert(await page.evaluate(() => document.documentElement.scrollWidth <= innerWidth));
          await page.setViewportSize({ width: 390, height: 844 });
          await page.screenshot({ path: 'test-results/preview-native-polling-pending-mobile.png', fullPage: true });
          assert(await page.evaluate(() => document.documentElement.scrollWidth <= innerWidth));
          await page.setViewportSize({ width: 1280, height: 900 });
          await panel.getByText('completed', { exact: true }).waitFor({ timeout: 25_000 });
          assert(performance.now() - uploadStartedAt >= 15_000,
            'browser observed completion before the native pending interval elapsed');
        })(),
      ]);
    } else {
      const cancelButton = panel.getByRole('button', { name: '取消尝试 1', exact: true });
      const cancelResponse = await clickForResponse(
        page,
        cancelButton,
        (url, method) => method === 'POST' && url.pathname === `/api/v1/previews/${jobId}/cancel`,
      );
      assert.deepEqual(cancelResponse.request().postDataJSON(), { expectedAttempt: 1 });
      assert.deepEqual(await responseJson(cancelResponse, 'PreviewMutationResult'), { applied: true });
      await panel.getByText('服务器确认取消已应用；部分历史证据仍保留。', { exact: true }).waitFor();
      await panel.getByText('cancelled', { exact: true }).waitFor({ timeout: 10_000 });
    }

    const terminalResponse = await clickForResponse(
      page,
      panel.getByRole('button', { name: '读取状态', exact: true }),
      url => url.pathname === `/api/v1/previews/${jobId}`,
    );
    const expectedStatus = pollingCase === 'poll_complete' ? 'completed' : 'cancelled';
    const terminalJob = await responseJson(terminalResponse, 'PreviewJobView');
    assert.equal(terminalJob.id, jobId);
    assert.equal(terminalJob.baseUrl, runningJob.baseUrl);
    assertFailureJob(terminalJob, expectedStatus, 1);
    const terminalFirstPage = polledTerminalPage ?? await readPollingPage(page, panel, jobId, 0);
    if (pollingCase === 'poll_complete') {
      assertResultPage(terminalFirstPage, terminalJob, 0, [{
        ordinal: 0,
        sourceId: 'tt4444444',
        uuid: 'Movie4444444',
        rating: 8,
        sink: { kind: 'known', value: 8 },
        status: 'skipped',
        reason: 'already_equal',
      }]);
      const result = panel.getByRole('region', { name: '历史结果页' }).locator('.preview-result').first();
      await result.getByText(/结果：已比较 · 证据来源：自动 · 快照时间：/).waitFor();
      assert(await panel.getByRole('button', { name: '取消尝试 1', exact: true }).isDisabled());
      assert(await panel.getByRole('button', { name: '显式恢复尝试 1', exact: true }).isDisabled());
    } else {
      assertEmptyPollingPage(terminalFirstPage, terminalJob, 0);
      assert(await panel.getByRole('button', { name: '显式恢复尝试 1', exact: true }).isEnabled());
    }
    const terminalOrdinalOnePage = await readPollingPage(page, panel, jobId, 1);
    assertEmptyPollingPage(terminalOrdinalOnePage, terminalJob, 1);

    const visibleText = await page.locator('body').innerText();
    for (const forbidden of [...FORBIDDEN_PUBLIC_VALUES, bootstrapKey]) assert(!visibleText.includes(forbidden));
    if (pollingCase === 'poll_cancel') {
      await page.setViewportSize({ width: 1280, height: 900 });
      await page.screenshot({ path: 'test-results/preview-native-polling-cancelled-desktop.png', fullPage: true });
      assert(await page.evaluate(() => document.documentElement.scrollWidth <= innerWidth));
      await page.setViewportSize({ width: 390, height: 844 });
      await page.screenshot({ path: 'test-results/preview-native-polling-cancelled-mobile.png', fullPage: true });
      assert(await page.evaluate(() => document.documentElement.scrollWidth <= innerWidth));
    }
    assert.equal(await page.evaluate(() => localStorage.length), 0);
    assert.equal(await page.evaluate(() => sessionStorage.length), 0);
    assert.equal(await page.getByText(/#key=/).count(), 0);
    assert.equal(await page.getByLabel('一次性链接 · 10 分钟内有效').count(), 0);

    host.send({ phase: 'retained', jobId });
    const reopened = await host.receive('reopened');
    assert.equal(reopened.case, pollingCase);
    await page.reload();
    await page.getByText('已授权', { exact: true }).waitFor();
    assert.equal(new URL(page.url()).hash, '');
    assert.equal(await panel.getByLabel('已有任务 ID', { exact: true }).inputValue(), '');
    assert.equal(await panel.getByLabel('保存的配置 ID', { exact: true }).inputValue(), '');
    assert.equal(await panel.getByLabel('CSV 文件（UTF-8，最大 10 MiB）').inputValue(), '');
    assert.equal(await panel.getByLabel('解析选择草稿').count(), 0);
    await panel.getByLabel('已有任务 ID', { exact: true }).fill(jobId);
    const reopenedResponse = await clickForResponse(
      page,
      panel.getByRole('button', { name: '打开任务', exact: true }),
      url => url.pathname === `/api/v1/previews/${jobId}`,
    );
    assert.deepEqual(await responseJson(reopenedResponse, 'PreviewJobView'), terminalJob);
    if (pollingCase === 'poll_cancel') {
      assert(await panel.getByRole('button', { name: '显式恢复尝试 1', exact: true }).isEnabled());
    }
    assert.deepEqual(await readPollingPage(page, panel, jobId, 0), terminalFirstPage);
    assert.deepEqual(await readPollingPage(page, panel, jobId, 1), terminalOrdinalOnePage);

    page.once('dialog', dialog => dialog.accept());
    const deleteResponse = await clickForResponse(
      page,
      panel.getByRole('button', { name: '删除运行历史', exact: true }),
      (url, method) => method === 'DELETE' && url.pathname === `/api/v1/previews/${jobId}`,
    );
    assert.deepEqual(await responseJson(deleteResponse, 'PreviewMutationResult'), { applied: true });
    await panel.getByText('服务器确认运行历史已删除；公共目录条目不受影响。', { exact: true }).waitFor();
    const missing = await page.evaluate(async id => {
      const response = await fetch(`/api/v1/previews/${encodeURIComponent(id)}`);
      return { status: response.status, body: await response.json() };
    }, jobId);
    assert.equal(missing.status, 404);
    assert.equal(missing.body.code, 'preview_job_not_found');
    assertPublic(missing.body);

    const mutations = requests.filter(request => ['POST', 'PUT', 'PATCH', 'DELETE'].includes(request.method));
    const expectedMutations = [
      'POST /api/v1/session/bootstrap',
      'POST /api/v1/imports/imdb/ratings?profileId=imdb_ratings&profileVersion=1',
      ...(pollingCase === 'poll_cancel' ? [`POST /api/v1/previews/${jobId}/cancel`] : []),
      `DELETE /api/v1/previews/${jobId}`,
    ];
    assert.deepEqual(mutations.map(request => `${request.method} ${request.path}`), expectedMutations);
    const headers = await Promise.all(requests.map(request => request.headers));
    for (const value of headers) {
      assert(value, 'browser request headers could not be read');
      assert.equal(value.authorization, undefined);
    }
    const mutationHeaders = await Promise.all(mutations.map(request => request.headers));
    assert.equal(mutationHeaders[0].origin, origin);
    assert.equal(mutationHeaders[0]['x-csrf-token'], undefined);
    for (const value of mutationHeaders.slice(1)) {
      assert.equal(value.origin, origin);
      assert(value['x-csrf-token']);
      assert.equal(value['x-csrf-token'], mutationHeaders[1]['x-csrf-token']);
    }
    assert.equal(requests.filter(request => request.path.endsWith('/resume')).length, 0);
    assert.equal(requests.filter(request => request.path.includes('resolution')).length, 0);
    assert.equal(requests.filter(request => request.method === 'POST'
      && request.path.startsWith('/api/v1/imports/imdb/ratings?')).length, 1);
    assert.deepEqual(pageErrors, []);

    host.send({ phase: 'finish', jobId });
    const done = await host.receive('done');
    assert.equal(done.case, pollingCase);
  } catch (error) {
    const message = error instanceof Error ? error.message : `native polling case ${pollingCase} failed`;
    const safe = bootstrapKeys.reduce((value, key) => value.replaceAll(key, '[redacted]'), message);
    // eslint-disable-next-line preserve-caught-error
    throw new Error(safe.replace(/#key=[^\s"']+/g, '#key=[redacted]'));
  } finally {
    await context?.close();
  }
}

async function runPollingPath(browser, executable) {
  const host = startHost(executable, POLLING_HOST_TEST);
  const bootstrapKeys = [];
  try {
    for (const pollingCase of POLLING_CASES) {
      await runPollingCase(browser, host, pollingCase, bootstrapKeys);
    }
    await host.receive('all_done');
    const result = await waitForExit(host.child, host.exit);
    assert.equal(result.error, undefined, 'native polling host process failed to start');
    assert.equal(result.forced, false, 'native polling host exceeded its natural exit bound');
    assert.equal(result.signal, null, 'native polling host terminated by signal');
    assert.equal(result.code, 0, 'native polling host did not exit successfully');
  } catch (error) {
    if (host.child.exitCode === null && host.child.signalCode === null) await host.closeInputAndDrain();
    throw error;
  }
}

function assertResolutionItemBase(item, job) {
  assertFailureOutcomeItem(item, job, 0, 'tt5555555');
}

function assertResolutionCatalogItem(item, job, externalIds) {
  assert.deepEqual(item, {
    uuid: 'Movie5555555',
    canonicalUrl: `${job.baseUrl}/movie/Movie5555555`,
    detailUrl: `${job.baseUrl}/api/movie/Movie5555555`,
    mediaType: 'movie',
    model: 'Movie',
    parentUuid: null,
    externalIds,
  });
}

function assertResolutionPage(resultPage, job, offset, outcome) {
  validate('PreviewResultPage', resultPage);
  assertPublic(resultPage);
  assert.deepEqual({
    jobId: resultPage.jobId,
    attempt: resultPage.attempt,
    offset: resultPage.offset,
    limit: resultPage.limit,
    historical: resultPage.historical,
    executable: resultPage.executable,
  }, {
    jobId: job.id,
    attempt: 1,
    offset,
    limit: 2,
    historical: true,
    executable: false,
  });
  if (offset === 1) {
    assert.deepEqual(resultPage.items, []);
    return;
  }
  assert.equal(resultPage.items.length, 1);
  const item = resultPage.items[0];
  assertResolutionItemBase(item, job);
  if (outcome === 'native') {
    assert.deepEqual(item.outcome, { kind: 'unsupported' });
    return;
  }
  if (outcome === 'search') {
    assert.equal(item.outcome.kind, 'search_candidates');
    assert.equal(item.outcome.moreAvailable, false);
    assert.equal(item.outcome.candidates.length, 1);
    const candidate = item.outcome.candidates[0];
    assert.equal(candidate.title, 'Exact review candidate');
    assertResolutionCatalogItem(candidate.item, job, [{ scheme: 'imdb:title', value: 'tt5555555' }]);
    return;
  }
  if (outcome === 'manual_missing_id') {
    assert.equal(item.outcome.kind, 'compared');
    assert.equal(item.outcome.provenance, 'manual');
    assert.equal(item.outcome.observedAt, 1_800_000_000);
    assertResolutionCatalogItem(item.outcome.item, job, []);
    assert.equal(item.outcome.comparison.profileId, 'imdb_ratings');
    assert.equal(item.outcome.comparison.profileVersion, 1);
    assert.deepEqual(item.outcome.comparison.fields, [
      {
        ruleId: 'rating',
        field: 'rating_grade',
        suggestion: 8,
        sink: { kind: 'known', value: 8 },
        status: 'skipped',
        reason: 'already_equal',
      },
      {
        ruleId: 'rating_implies_complete',
        field: 'shelf_type',
        suggestion: null,
        sink: { kind: 'known', value: 'complete' },
        status: 'skipped',
        reason: 'rule_disabled',
      },
    ]);
    return;
  }
  assert.equal(outcome, 'manual_conflict_id');
  assert.equal(item.outcome.kind, 'needs_review');
  assert.equal(item.outcome.reason, 'item_identity_conflict');
  assertResolutionCatalogItem(item.outcome.item, job, [{ scheme: 'imdb:title', value: 'tt9999999' }]);
  assert.equal(item.outcome.comparison, undefined);
}

async function readResolutionHistory(page, panel, jobId, expectedOutcome) {
  const statusResponse = await clickForResponse(
    page,
    panel.getByRole('button', { name: '读取状态', exact: true }),
    url => url.pathname === `/api/v1/previews/${jobId}`,
  );
  const job = await responseJson(statusResponse, 'PreviewJobView');
  assert.equal(job.id, jobId);
  assertFailureJob(job, 'completed', 1);
  const firstPage = await readPollingPage(page, panel, jobId, 0);
  assertResolutionPage(firstPage, job, 0, expectedOutcome);
  const ordinalOnePage = await readPollingPage(page, panel, jobId, 1);
  assertResolutionPage(ordinalOnePage, job, 1, expectedOutcome);
  return { job, firstPage, ordinalOnePage };
}

async function openResolutionHistory(page, panel, expected) {
  await panel.getByLabel('已有任务 ID', { exact: true }).fill(expected.job.id);
  const response = await clickForResponse(
    page,
    panel.getByRole('button', { name: '打开任务', exact: true }),
    url => url.pathname === `/api/v1/previews/${expected.job.id}`,
  );
  assert.deepEqual(await responseJson(response, 'PreviewJobView'), expected.job);
  assert.deepEqual(await readPollingPage(page, panel, expected.job.id, 0), expected.firstPage);
  assert.deepEqual(await readPollingPage(page, panel, expected.job.id, 1), expected.ordinalOnePage);
  assert.equal(await panel.getByLabel('解析选择草稿').count(), 0);
  assert.equal(await panel.getByRole('button', { name: /暂存记录 0/ }).count(), 0);
  assert.equal(await panel.getByLabel('记录 0 的搜索词', { exact: true }).count(), 0);
  assert.equal(await panel.getByRole('button', { name: '按选择生成新预览', exact: true }).count(), 0);
}

async function deleteResolutionHistory(page, panel, expected) {
  await panel.getByLabel('已有任务 ID', { exact: true }).fill(expected.job.id);
  const openResponse = await clickForResponse(
    page,
    panel.getByRole('button', { name: '打开任务', exact: true }),
    url => url.pathname === `/api/v1/previews/${expected.job.id}`,
  );
  assert.deepEqual(await responseJson(openResponse, 'PreviewJobView'), expected.job);
  page.once('dialog', dialog => dialog.accept());
  const deleteResponse = await clickForResponse(
    page,
    panel.getByRole('button', { name: '删除运行历史', exact: true }),
    (url, method) => method === 'DELETE' && url.pathname === `/api/v1/previews/${expected.job.id}`,
  );
  assert.deepEqual(await responseJson(deleteResponse, 'PreviewMutationResult'), { applied: true });
  await panel.getByText('服务器确认运行历史已删除；公共目录条目不受影响。', { exact: true }).waitFor();
  const missing = await page.evaluate(async id => {
    const response = await fetch(`/api/v1/previews/${encodeURIComponent(id)}`);
    return { status: response.status, body: await response.json() };
  }, expected.job.id);
  assert.equal(missing.status, 404);
  assert.equal(missing.body.code, 'preview_job_not_found');
  assertPublic(missing.body);
}

async function runResolutionCase(browser, host, resolutionCase, bootstrapKeys) {
  let context;
  let bootstrapKey;
  try {
    const ready = await host.receive('ready');
    assert.equal(ready.case, resolutionCase);
    assert.equal(typeof ready.bootstrapUrl, 'string');
    const bootstrapUrl = new URL(ready.bootstrapUrl);
    const origin = bootstrapUrl.origin;
    bootstrapKey = new URLSearchParams(bootstrapUrl.hash.slice(1)).get('key');
    assert(bootstrapKey);
    bootstrapKeys.push(bootstrapKey);

    context = await browser.newContext({ viewport: { width: 1280, height: 900 } });
    const page = await context.newPage();
    const pageErrors = [];
    const requests = [];
    page.on('pageerror', error => pageErrors.push(error.message));
    page.on('request', request => {
      const record = coreRequestRecord(request, origin);
      if (record) requests.push(record);
    });

    await page.goto(bootstrapUrl.href);
    await page.getByRole('heading', { name: '本地服务已连接' }).waitFor();
    await page.getByText('已授权', { exact: true }).waitFor();
    assert.equal(new URL(page.url()).hash, '');
    assert.equal(await page.evaluate(() => document.cookie), '');
    const cookies = await context.cookies(origin);
    assert(cookies.some(cookie => cookie.name === 'collector_session'
      && cookie.httpOnly && cookie.sameSite === 'Strict'));
    await page.getByText(/alice · http:\/\/127\.0\.0\.1:/).waitFor();

    const panel = page.getByRole('region', { name: '历史预览' });
    await panel.getByLabel('保存的配置 ID', { exact: true }).fill('imdb_ratings');
    await panel.getByLabel('保存的配置版本', { exact: true }).fill('1');
    await panel.getByLabel('CSV 文件（UTF-8，最大 10 MiB）').setInputFiles({
      name: `ratings-${resolutionCase}.csv`, mimeType: 'text/csv', buffer: Buffer.from(RESOLUTION_CSV),
    });
    const rawResponse = await clickForResponse(
      page,
      panel.getByRole('button', { name: '提交只读预览', exact: true }),
      (url, method) => method === 'POST' && url.pathname === '/api/v1/imports/imdb/ratings',
    );
    assert.equal(rawResponse.status(), 202);
    const rawReceipt = await responseJson(rawResponse, 'PreviewImportAccepted');
    assert.deepEqual({
      observations: rawReceipt.observationCount,
      skippedRows: rawReceipt.skippedRowCount,
      skippedColumns: rawReceipt.skippedColumnCount,
      duplicates: rawReceipt.duplicateRowCount,
      attempt: rawReceipt.preview.attempt,
      executable: rawReceipt.preview.executable,
    }, {
      observations: 1, skippedRows: 0, skippedColumns: 1, duplicates: 0,
      attempt: 1, executable: false,
    });
    const nativeJobId = rawReceipt.preview.jobId;
    assert.equal(rawResponse.request().postData(), RESOLUTION_CSV);
    await panel.getByRole('heading', { name: `任务 ${nativeJobId}`, exact: true }).waitFor();
    await panel.getByText('completed', { exact: true }).waitFor({ timeout: 12_000 });
    const nativeHistory = await readResolutionHistory(page, panel, nativeJobId, 'native');
    host.send({ phase: 'native', jobId: nativeJobId });
    const native = await host.receive('native');
    assert.equal(native.case, resolutionCase);

    await panel.getByLabel('起始记录序号').fill('0');
    await panel.getByLabel('单页数量').fill('2');
    const visibleNativePage = await clickForResponse(
      page,
      panel.getByRole('button', { name: '读取该尝试的一页结果', exact: true }),
      url => url.pathname === `/api/v1/previews/${nativeJobId}/results`
        && url.searchParams.get('offset') === '0',
    );
    assert.deepEqual(await responseJson(visibleNativePage, 'PreviewResultPage'), nativeHistory.firstPage);
    const query = panel.getByLabel('记录 0 的搜索词', { exact: true });
    await query.fill('  Exact candidate  ');
    const mutationsBeforeSearchStage = requests.filter(request => ['POST', 'PUT', 'PATCH', 'DELETE'].includes(request.method)).length;
    const stageSearch = panel.getByRole('button', { name: '暂存记录 0 的搜索', exact: true });
    await stageSearch.focus();
    assert(await stageSearch.evaluate(element => element === document.activeElement));
    await stageSearch.press('Enter');
    const draft = panel.getByLabel('解析选择草稿', { exact: true });
    await draft.getByText('Exact candidate', { exact: false }).waitFor();
    assert.equal(requests.filter(request => ['POST', 'PUT', 'PATCH', 'DELETE'].includes(request.method)).length,
      mutationsBeforeSearchStage, 'search staging dispatched a mutation');
    host.send({ phase: 'search_staged', jobId: nativeJobId });
    const searchStaged = await host.receive('staged');
    assert.equal(searchStaged.case, resolutionCase);
    assert.equal(searchStaged.kind, 'search');

    const searchResponse = await clickForResponse(
      page,
      panel.getByRole('button', { name: '按选择生成新预览', exact: true }),
      (url, method) => method === 'POST' && url.pathname === '/api/v1/imports/imdb/ratings',
    );
    assert.equal(searchResponse.status(), 202);
    const searchRequest = searchResponse.request().postDataJSON();
    validate('PreviewImdbResolutionRequest', searchRequest);
    assert.deepEqual(searchRequest, {
      csv: RESOLUTION_CSV,
      resolutionInputs: [{
        kind: 'search', ordinal: 0, sourceRecordId: 'tt5555555', query: '  Exact candidate  ',
      }],
    });
    validate('PreviewResolutionChoice', searchRequest.resolutionInputs[0]);
    const searchReceipt = await responseJson(searchResponse, 'PreviewImportAccepted');
    assert.deepEqual({
      observations: searchReceipt.observationCount,
      skippedRows: searchReceipt.skippedRowCount,
      skippedColumns: searchReceipt.skippedColumnCount,
      duplicates: searchReceipt.duplicateRowCount,
      attempt: searchReceipt.preview.attempt,
      executable: searchReceipt.preview.executable,
    }, {
      observations: 1, skippedRows: 0, skippedColumns: 1, duplicates: 0,
      attempt: 1, executable: false,
    });
    const searchJobId = searchReceipt.preview.jobId;
    assert.notEqual(searchJobId, nativeJobId);
    await panel.getByRole('heading', { name: `任务 ${searchJobId}`, exact: true }).waitFor();
    await panel.getByText('completed', { exact: true }).waitFor({ timeout: 12_000 });
    const searchHistory = await readResolutionHistory(page, panel, searchJobId, 'search');
    const results = panel.getByRole('region', { name: '历史结果页' });
    await panel.getByLabel('起始记录序号').fill('0');
    const visibleSearchPage = await clickForResponse(
      page,
      panel.getByRole('button', { name: '读取该尝试的一页结果', exact: true }),
      url => url.pathname === `/api/v1/previews/${searchJobId}/results`
        && url.searchParams.get('offset') === '0',
    );
    assert.deepEqual(await responseJson(visibleSearchPage, 'PreviewResultPage'), searchHistory.firstPage);
    await results.getByText('Exact review candidate', { exact: true }).waitFor();
    await results.getByText(/结果：搜索候选 · 服务端未说明还有候选/).waitFor();
    await draft.getByText('Exact candidate', { exact: false }).waitFor();
    assert.equal(await draft.getByText('人工目录条目 Movie5555555', { exact: false }).count(), 0,
      'singleton candidate was selected automatically');
    host.send({ phase: 'searched', jobId: searchJobId });
    const searched = await host.receive('searched');
    assert.equal(searched.case, resolutionCase);

    const mutationsBeforeManualStage = requests.filter(request => ['POST', 'PUT', 'PATCH', 'DELETE'].includes(request.method)).length;
    const stageManual = panel.getByRole('button', { name: '暂存记录 0 的目录条目 Movie5555555', exact: true });
    await stageManual.focus();
    assert(await stageManual.evaluate(element => element === document.activeElement));
    await stageManual.press('Enter');
    await draft.getByText('人工目录条目 Movie5555555', { exact: false }).waitFor();
    assert.equal(await draft.getByText('Exact candidate', { exact: false }).count(), 0);
    assert.equal(requests.filter(request => ['POST', 'PUT', 'PATCH', 'DELETE'].includes(request.method)).length,
      mutationsBeforeManualStage, 'manual staging dispatched a mutation');
    host.send({ phase: 'manual_staged', jobId: searchJobId });
    const manualStaged = await host.receive('staged');
    assert.equal(manualStaged.case, resolutionCase);
    assert.equal(manualStaged.kind, 'manual');

    const visibleCandidateText = await page.locator('body').innerText();
    for (const forbidden of [...FORBIDDEN_PUBLIC_VALUES, bootstrapKey]) assert(!visibleCandidateText.includes(forbidden));
    await page.setViewportSize({ width: 1280, height: 900 });
    await page.screenshot({ path: `test-results/preview-native-resolution-${resolutionCase}-candidate-desktop.png`, fullPage: true });
    assert(await page.evaluate(() => document.documentElement.scrollWidth <= innerWidth));
    await page.setViewportSize({ width: 390, height: 844 });
    await page.screenshot({ path: `test-results/preview-native-resolution-${resolutionCase}-candidate-mobile.png`, fullPage: true });
    assert(await page.evaluate(() => document.documentElement.scrollWidth <= innerWidth));
    await page.setViewportSize({ width: 1280, height: 900 });

    const manualResponse = await clickForResponse(
      page,
      panel.getByRole('button', { name: '按选择生成新预览', exact: true }),
      (url, method) => method === 'POST' && url.pathname === '/api/v1/imports/imdb/ratings',
    );
    assert.equal(manualResponse.status(), 202);
    const manualRequest = manualResponse.request().postDataJSON();
    validate('PreviewImdbResolutionRequest', manualRequest);
    assert.deepEqual(manualRequest, {
      csv: RESOLUTION_CSV,
      resolutionInputs: [{
        kind: 'manual',
        ordinal: 0,
        sourceRecordId: 'tt5555555',
        item: { uuid: 'Movie5555555', mediaType: 'movie', model: 'Movie' },
      }],
    });
    validate('PreviewResolutionChoice', manualRequest.resolutionInputs[0]);
    const manualReceipt = await responseJson(manualResponse, 'PreviewImportAccepted');
    assert.deepEqual({
      observations: manualReceipt.observationCount,
      skippedRows: manualReceipt.skippedRowCount,
      skippedColumns: manualReceipt.skippedColumnCount,
      duplicates: manualReceipt.duplicateRowCount,
      attempt: manualReceipt.preview.attempt,
      executable: manualReceipt.preview.executable,
    }, {
      observations: 1, skippedRows: 0, skippedColumns: 1, duplicates: 0,
      attempt: 1, executable: false,
    });
    const manualJobId = manualReceipt.preview.jobId;
    assert.notEqual(manualJobId, nativeJobId);
    assert.notEqual(manualJobId, searchJobId);
    await panel.getByRole('heading', { name: `任务 ${manualJobId}`, exact: true }).waitFor();
    await panel.getByText('completed', { exact: true }).waitFor({ timeout: 12_000 });
    const manualHistory = await readResolutionHistory(page, panel, manualJobId, resolutionCase);
    await panel.getByLabel('起始记录序号').fill('0');
    const visibleManualPage = await clickForResponse(
      page,
      panel.getByRole('button', { name: '读取该尝试的一页结果', exact: true }),
      url => url.pathname === `/api/v1/previews/${manualJobId}/results`
        && url.searchParams.get('offset') === '0',
    );
    assert.deepEqual(await responseJson(visibleManualPage, 'PreviewResultPage'), manualHistory.firstPage);
    if (resolutionCase === 'manual_missing_id') {
      await results.getByText(/结果：已比较 · 证据来源：人工 · 快照时间：/).waitFor();
      await results.getByText('already_equal', { exact: true }).waitFor();
      await results.getByText('无外部身份声明。', { exact: true }).waitFor();
    } else {
      await results.getByText(/结果：需要检查 · item_identity_conflict/).waitFor();
      await results.getByText('tt9999999', { exact: true }).waitFor();
    }
    const visibleManualText = await page.locator('body').innerText();
    for (const forbidden of [...FORBIDDEN_PUBLIC_VALUES, bootstrapKey]) assert(!visibleManualText.includes(forbidden));
    await page.setViewportSize({ width: 1280, height: 900 });
    await page.screenshot({ path: `test-results/preview-native-resolution-${resolutionCase}-manual-desktop.png`, fullPage: true });
    assert(await page.evaluate(() => document.documentElement.scrollWidth <= innerWidth));
    await page.setViewportSize({ width: 390, height: 844 });
    await page.screenshot({ path: `test-results/preview-native-resolution-${resolutionCase}-manual-mobile.png`, fullPage: true });
    assert(await page.evaluate(() => document.documentElement.scrollWidth <= innerWidth));
    assert.equal(await page.evaluate(() => localStorage.length), 0);
    assert.equal(await page.evaluate(() => sessionStorage.length), 0);
    assert.equal(await page.getByText(/#key=/).count(), 0);

    host.send({ phase: 'retained', jobId: manualJobId });
    const reopened = await host.receive('reopened');
    assert.equal(reopened.case, resolutionCase);
    await page.reload();
    await page.getByText('已授权', { exact: true }).waitFor();
    assert.equal(new URL(page.url()).hash, '');
    assert.equal(await panel.getByLabel('已有任务 ID', { exact: true }).inputValue(), '');
    assert.equal(await panel.getByLabel('保存的配置 ID', { exact: true }).inputValue(), '');
    assert.equal(await panel.getByLabel('保存的配置版本', { exact: true }).inputValue(), '');
    assert.equal(await panel.getByLabel('CSV 文件（UTF-8，最大 10 MiB）').inputValue(), '');
    assert.equal(await panel.getByLabel('解析选择草稿').count(), 0);
    const histories = [nativeHistory, searchHistory, manualHistory];
    for (const history of histories) await openResolutionHistory(page, panel, history);
    for (const history of histories) await deleteResolutionHistory(page, panel, history);

    const mutations = requests.filter(request => ['POST', 'PUT', 'PATCH', 'DELETE'].includes(request.method));
    assert.deepEqual(mutations.map(request => `${request.method} ${request.path}`), [
      'POST /api/v1/session/bootstrap',
      'POST /api/v1/imports/imdb/ratings?profileId=imdb_ratings&profileVersion=1',
      'POST /api/v1/imports/imdb/ratings?profileId=imdb_ratings&profileVersion=1',
      'POST /api/v1/imports/imdb/ratings?profileId=imdb_ratings&profileVersion=1',
      `DELETE /api/v1/previews/${nativeJobId}`,
      `DELETE /api/v1/previews/${searchJobId}`,
      `DELETE /api/v1/previews/${manualJobId}`,
    ]);
    const headers = await Promise.all(requests.map(request => request.headers));
    for (const value of headers) {
      assert(value, 'browser request headers could not be read');
      assert.equal(value.authorization, undefined);
    }
    const mutationHeaders = await Promise.all(mutations.map(request => request.headers));
    assert.equal(mutationHeaders[0].origin, origin);
    assert.equal(mutationHeaders[0]['x-csrf-token'], undefined);
    for (const value of mutationHeaders.slice(1)) {
      assert.equal(value.origin, origin);
      assert(value['x-csrf-token']);
      assert.equal(value['x-csrf-token'], mutationHeaders[1]['x-csrf-token']);
    }
    assert.match(mutationHeaders[1]['content-type'], /^text\/csv(?:;|$)/);
    for (const value of mutationHeaders.slice(2, 4)) assert.match(value['content-type'], /^application\/json(?:;|$)/);
    assert.equal(requests.filter(request => request.path.endsWith('/resume')
      || request.path.endsWith('/cancel')).length, 0);
    assert.deepEqual(pageErrors, []);

    host.send({ phase: 'finish', jobId: manualJobId });
    const done = await host.receive('done');
    assert.equal(done.case, resolutionCase);
  } catch (error) {
    const message = error instanceof Error ? error.message : `native resolution case ${resolutionCase} failed`;
    const safe = bootstrapKeys.reduce((value, key) => value.replaceAll(key, '[redacted]'), message);
    // eslint-disable-next-line preserve-caught-error
    throw new Error(safe.replace(/#key=[^\s"']+/g, '#key=[redacted]'));
  } finally {
    await context?.close();
  }
}

async function runResolutionPath(browser, executable) {
  const host = startHost(executable, RESOLUTION_HOST_TEST);
  const bootstrapKeys = [];
  try {
    for (const resolutionCase of RESOLUTION_CASES) {
      await runResolutionCase(browser, host, resolutionCase, bootstrapKeys);
    }
    await host.receive('all_done');
    const result = await waitForExit(host.child, host.exit);
    assert.equal(result.error, undefined, 'native resolution host process failed to start');
    assert.equal(result.forced, false, 'native resolution host exceeded its natural exit bound');
    assert.equal(result.signal, null, 'native resolution host terminated by signal');
    assert.equal(result.code, 0, 'native resolution host did not exit successfully');
  } catch (error) {
    if (host.child.exitCode === null && host.child.signalCode === null) await host.closeInputAndDrain();
    throw error;
  }
}

async function runEofProbe(executable, testName = HOST_TEST) {
  const host = startHost(executable, testName);
  try {
    const ready = await host.receive('ready');
    if (testName === FAILURE_HOST_TEST) assert.equal(ready.case, FAILURE_CASES[0]);
    if (testName === POLLING_HOST_TEST) assert.equal(ready.case, POLLING_CASES[0]);
    if (testName === RESOLUTION_HOST_TEST) assert.equal(ready.case, RESOLUTION_CASES[0]);
    const origin = new URL(ready.bootstrapUrl).origin;
    const result = await host.closeInputAndDrain();
    assert.equal(result.error, undefined, 'EOF probe host process failed to start');
    assert.equal(result.forced, false, 'EOF probe required forced termination');
    assert.equal(result.signal, null, 'EOF probe host terminated by signal');
    assert(Number.isInteger(result.code) && result.code !== 0, 'EOF probe host did not fail naturally');
    let reachable = true;
    try {
      await fetch(`${origin}/api/v1/health`, { signal: AbortSignal.timeout(2_000) });
    } catch {
      reachable = false;
    }
    assert.equal(reachable, false, 'EOF probe left the Core port reachable');
  } finally {
    if (host.child.exitCode === null && host.child.signalCode === null) await host.closeInputAndDrain();
  }
}

export async function runPreviewNativeBrowserSmoke(existingBrowser) {
  await mkdir(new URL('../test-results/', import.meta.url), { recursive: true });
  const executable = await cargoTestExecutable();
  const browser = existingBrowser ?? await chromium.launch();
  try {
    await runHappyPath(browser, executable);
    await runEofProbe(executable);
    await runFailurePath(browser, executable);
    await runEofProbe(executable, FAILURE_HOST_TEST);
    await runPollingPath(browser, executable);
    await runEofProbe(executable, POLLING_HOST_TEST);
    await runResolutionPath(browser, executable);
    await runEofProbe(executable, RESOLUTION_HOST_TEST);
    console.log('Actual native browser/Core/SQLite/NeoDB-adapter preview acceptance passed, including failure, polling, and explicit resolution retention.');
  } finally {
    if (!existingBrowser) await browser.close();
  }
}

if (process.argv[1] && import.meta.url === pathToFileURL(process.argv[1]).href) {
  await runPreviewNativeBrowserSmoke();
}
