// Browser/client evidence only. Preview endpoints are mocked; Core session,
// status, assets and mapping APIs in smoke.mjs remain actual local services.
import assert from 'node:assert/strict';
import { readFile } from 'node:fs/promises';
import Ajv2020 from 'ajv/dist/2020.js';

// Browser standalone validators are bundled by Vite. Node smoke uses the same
// generated JSON schemas with the already installed AJV, not a second wire model.
const ajv = new Ajv2020({ strict: true });
for (const format of ['uint32', 'int64', 'double']) ajv.addFormat(format, true);
const validators = await Promise.all(['PreviewJobView', 'PreviewResultPage', 'PreviewImportAccepted'].map(async name =>
  ajv.compile(JSON.parse(await readFile(new URL(`../contracts/json-schema/${name}.json`, import.meta.url), 'utf8'))),
));
const [validatePreviewJobView, validatePreviewResultPage, validatePreviewImportAccepted] = validators;

export async function installPreviewUiFixture(page) {
  const now = Math.floor(Date.now() / 1000);
  const job = {
    id: 'preview_browser', status: 'running', attempt: 1,
    createdAt: now, updatedAt: now, expiresAt: now + 3600, observationCount: 6,
    profileId: 'imdb_ratings', profileVersion: 1, baseUrl: 'https://neodb.example',
    account: 'browser_fixture', bindingState: 'current', historical: true, executable: false,
  };
  const item = {
    uuid: 'browser-item', canonicalUrl: 'https://neodb.example/movie/browser-item',
    detailUrl: 'https://neodb.example/api/movie/browser-item', mediaType: 'movie',
    model: 'Movie', parentUuid: null,
    externalIds: [{ scheme: 'imdb:title', value: 'tt1234567' }],
  };
  const outcomes = [
    { kind: 'compared', item, observedAt: now, provenance: 'automatic', comparison: {
      profileId: job.profileId, profileVersion: job.profileVersion, fields: [
        { ruleId: 'shelf_type', field: 'shelf_type', suggestion: 'complete', sink: { kind: 'known', value: 'complete' }, status: 'skipped', reason: 'already_equal' },
        { ruleId: 'rating_grade', field: 'rating_grade', suggestion: 8, sink: { kind: 'empty' }, status: 'accepted', reason: 'sink_empty_accepts_suggestion' },
        { ruleId: 'unknown_rating', field: 'rating_grade', suggestion: 8, sink: { kind: 'unknown' }, status: 'needs_review', reason: 'sink_state_unknown' },
        { ruleId: 'rejected_rule', field: 'rating_grade', suggestion: null, sink: { kind: 'known', value: 6 }, status: 'rejected', reason: 'mapping_rule_rejected' },
      ],
    } },
    { kind: 'needs_review', item: { ...item, externalIds: [...item.externalIds, { scheme: 'imdb:title', value: 'tt7654321' }] }, reason: 'contradictory_identity' },
    { kind: 'search_candidates', candidates: [{ title: '待确认的电影候选', item }], moreAvailable: true },
    { kind: 'unsupported' }, { kind: 'rate_limited' }, { kind: 'unavailable' },
  ];
  let deleted = false;
  const mutations = [];
  const csv = '\uFEFFConst,Your Rating,Title Type\r\ntt1234567,8,movie\r\n';
  const reply = (route, body, status = 200) => route.fulfill({ status, contentType: 'application/json', body: JSON.stringify(body) });
  await page.route('**/api/v1/imports/imdb/ratings?*', async route => {
    const request = route.request();
    const url = new URL(request.url());
    assert.equal(request.method(), 'POST');
    assert.equal(url.searchParams.get('profileId'), job.profileId);
    assert.equal(url.searchParams.get('profileVersion'), '1'); // v2 already exists in real mapping storage.
    assert.equal(request.headers()['content-type'], 'text/csv');
    assert(request.headers()['x-csrf-token']);
    assert.deepEqual(request.postDataBuffer(), Buffer.from(csv));
    mutations.push('upload');
    const receipt = { preview: { jobId: job.id, attempt: 1, executable: false }, observationCount: 6, skippedRowCount: 1, skippedColumnCount: 2, duplicateRowCount: 1 };
    assert(validatePreviewImportAccepted(receipt));
    await reply(route, receipt, 202);
  });
  await page.route('**/api/v1/previews/**', async route => {
    const request = route.request();
    const url = new URL(request.url());
    assert(url.pathname.startsWith(`/api/v1/previews/${job.id}`));
    if (request.method() !== 'GET') {
      assert(request.headers()['x-csrf-token']);
      mutations.push(request.method() === 'DELETE' ? 'delete' : url.pathname.split('/').at(-1));
    }
    if (url.pathname.endsWith('/cancel')) {
      assert.equal(request.method(), 'POST');
      assert.deepEqual(request.postDataJSON(), { expectedAttempt: 1 });
      job.status = 'cancelled';
      return reply(route, { applied: true });
    }
    if (url.pathname.endsWith('/resume')) {
      assert.equal(request.method(), 'POST');
      assert.equal(job.status, 'cancelled');
      assert.deepEqual(request.postDataJSON(), { expectedAttempt: 1 });
      job.attempt = 2;
      job.status = 'completed';
      job.updatedAt = now + 10;
      return reply(route, { jobId: job.id, attempt: 2, executable: false }, 202);
    }
    if (request.method() === 'DELETE') {
      deleted = true;
      return reply(route, { applied: true });
    }
    assert.equal(request.method(), 'GET');
    if (deleted) return reply(route, { code: 'preview_job_not_found', message: 'Preview not found.', field: null, retryable: false }, 404);
    if (url.pathname.endsWith('/results')) {
      assert.equal(Number(url.searchParams.get('attempt')), job.attempt);
      const offset = Number(url.searchParams.get('offset'));
      const limit = Number(url.searchParams.get('limit'));
      const items = outcomes.map((outcome, ordinal) => ({
        ordinal, outcome, attempt: job.attempt, committedAt: now + job.attempt,
        sourceConnectorId: 'imdb:ratings_csv', sourceRecordId: `tt123456${ordinal}`,
        sourceFingerprint: `sha256-browser-${ordinal}`, profileId: job.profileId,
        profileVersion: job.profileVersion, baseUrl: job.baseUrl, account: job.account,
        historical: true, executable: false,
      })).filter(row => row.ordinal >= offset).slice(0, limit);
      const result = { jobId: job.id, attempt: job.attempt, offset, limit, historical: true, executable: false, items };
      assert(validatePreviewResultPage(result));
      return reply(route, result);
    }
    assert(validatePreviewJobView(job));
    return reply(route, job);
  });
  return { csv, mutations, job, cleanup: async () => {
    await page.unroute('**/api/v1/imports/imdb/ratings?*');
    await page.unroute('**/api/v1/previews/**');
  } };
}

export async function runPreviewUiSmoke(page) {
  const fixture = await installPreviewUiFixture(page);
  try {
    const panel = page.getByRole('region', { name: '历史预览' });
    await panel.getByLabel('保存的配置 ID', { exact: true }).fill('imdb_ratings');
    await panel.getByLabel('保存的配置版本', { exact: true }).fill('1');
    await panel.getByLabel('CSV 文件（UTF-8，最大 10 MiB）').setInputFiles({
      name: 'ratings.csv', mimeType: 'text/csv', buffer: Buffer.from(fixture.csv),
    });
    await panel.getByText(/尚未.*提交/).waitFor();
    assert.deepEqual(fixture.mutations, []);
    await panel.getByRole('button', { name: '提交只读预览' }).click();
    await panel.getByRole('heading', { name: '任务 preview_browser', exact: true }).waitFor();
    assert.deepEqual(fixture.mutations, ['upload']);
    await panel.getByRole('button', { name: '读取该尝试的一页结果' }).click();
    const results = panel.getByRole('region', { name: '历史结果页' });
    await results.getByText('Unknown · 无法确认', { exact: true }).waitFor();
    for (const evidence of [
      'Known · complete', 'Empty · 已确认为空', 'sink_state_unknown',
      'mapping_rule_rejected', 'tt7654321', '待确认的电影候选',
    ]) assert(await results.getByText(evidence, { exact: true }).count());
    for (const outcome of ['不支持', '目录服务限流', '目录服务不可用']) {
      assert(await results.getByText(new RegExp(outcome)).count());
    }
    assert.equal(await results.locator('.preview-result').count(), 6);
    await page.setViewportSize({ width: 1280, height: 900 });
    await results.locator('.preview-result').first().screenshot({ path: 'test-results/preview-desktop.png' });
    assert(await page.evaluate(() => document.documentElement.scrollWidth <= innerWidth));
    await page.setViewportSize({ width: 390, height: 844 });
    await results.locator('.preview-result').first().screenshot({ path: 'test-results/preview-mobile.png' });
    assert(await page.evaluate(() => document.documentElement.scrollWidth <= innerWidth));
    assert.equal(await page.evaluate(() => localStorage.length), 0);
    assert.equal(await page.evaluate(() => sessionStorage.length), 0);
    await panel.getByRole('button', { name: '取消尝试 1', exact: true }).click();
    await panel.getByText('cancelled', { exact: true }).waitFor();
    await panel.getByRole('button', { name: '显式恢复尝试 1', exact: true }).click();
    await panel.getByText('completed', { exact: true }).waitFor();
    assert.equal(fixture.job.attempt, 2);
    await panel.getByRole('button', { name: '读取该尝试的一页结果' }).click();
    await results.getByText(/尝试 2.*起始序号/).waitFor();
    page.once('dialog', dialog => dialog.accept());
    await panel.getByRole('button', { name: '删除运行历史', exact: true }).click();
    await panel.getByText('服务器确认运行历史已删除；公共目录条目不受影响。', { exact: true }).waitFor();
    assert.equal(await panel.getByRole('heading', { name: '任务 preview_browser', exact: true }).count(), 0);
    assert.deepEqual(fixture.mutations, ['upload', 'cancel', 'resume', 'delete']);
    const capabilities = await page.request.get(new URL('/api/v1/capabilities', page.url()).href).then(response => response.json());
    delete capabilities.routes.previewResume;
    await page.route('**/api/v1/capabilities', route => route.fulfill({ contentType: 'application/json', body: JSON.stringify(capabilities) }));
    await page.reload();
    await page.getByText('已授权', { exact: true }).waitFor();
    assert.equal(await page.getByRole('region', { name: '历史预览' }).count(), 0);
    assert.deepEqual(fixture.mutations, ['upload', 'cancel', 'resume', 'delete']);
    console.log('Mock preview browser/client flow and desktop/mobile evidence passed (not backend restart acceptance).');
  } finally {
    await page.unroute('**/api/v1/capabilities');
    await fixture.cleanup();
  }
}
