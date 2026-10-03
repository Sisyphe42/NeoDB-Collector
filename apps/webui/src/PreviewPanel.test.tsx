import { StrictMode } from 'react';
import { act, cleanup, fireEvent, render, screen, waitFor } from '@testing-library/react';
import { afterEach, describe, expect, it, vi } from 'vitest';
import {
  AccessClientError,
  PreviewAdmissionClientError,
  type AccessClient,
  type PreviewJobView,
  type PreviewResultPage,
} from '@neodb-collector/core-client';
import { PreviewPanel } from './PreviewPanel.js';

afterEach(() => { cleanup(); vi.useRealTimers(); vi.restoreAllMocks(); });

const job = (overrides: Partial<PreviewJobView> = {}): PreviewJobView => ({
  id: 'preview_1', status: 'completed', attempt: 2,
  createdAt: 1_700_000_000, updatedAt: 1_700_000_005, expiresAt: 1_702_592_000,
  observationCount: 7, profileId: 'imdb_ratings', profileVersion: 1,
  baseUrl: 'https://neodb.example', account: 'alice', bindingState: 'current',
  historical: true, executable: false, ...overrides,
});

const item = {
  uuid: 'item-1', canonicalUrl: 'https://neodb.example/item/item-1',
  detailUrl: 'https://neodb.example/api/items/item-1', mediaType: 'movie', model: 'Movie',
  parentUuid: null, externalIds: [{ scheme: 'imdb:title', value: 'tt1234567' }],
};

const resultPage: PreviewResultPage = {
  jobId: 'preview_1', attempt: 2, offset: 0, limit: 25, historical: true, executable: false,
  items: [
    {
      ordinal: 0, attempt: 2, committedAt: 1_700_000_010,
      sourceConnectorId: 'imdb:ratings_csv', sourceRecordId: 'record-0', sourceFingerprint: 'sha256-0',
      profileId: 'imdb_ratings', profileVersion: 1, baseUrl: 'https://neodb.example', account: 'alice', historical: true, executable: false,
      outcome: {
        kind: 'compared', item, observedAt: 1_700_000_001, provenance: 'manual',
        comparison: { profileId: 'imdb_ratings', profileVersion: 1, fields: [
          { ruleId: 'known', field: 'rating_known', suggestion: 8, sink: { kind: 'known', value: 8 }, status: 'skipped', reason: 'already_equal' },
          { ruleId: 'empty', field: 'rating_empty', suggestion: 8, sink: { kind: 'empty' }, status: 'accepted', reason: 'sink_empty_accepts_suggestion' },
          { ruleId: 'unknown', field: 'rating_unknown', suggestion: 8, sink: { kind: 'unknown' }, status: 'needs_review', reason: 'sink_state_unknown' },
        ] },
      },
    },
    ...([
      { kind: 'needs_review', item: { ...item, canonicalUrl: 'javascript:alert(1)' }, reason: 'multiple identities' },
      { kind: 'search_candidates', candidates: [{ title: 'Candidate', item }], moreAvailable: true },
      { kind: 'unsupported' }, { kind: 'rate_limited' }, { kind: 'unavailable' },
    ] satisfies PreviewResultPage['items'][number]['outcome'][]).map((outcome, index) => ({
      ordinal: index + 1, attempt: 2, committedAt: 1_700_000_011 + index,
      sourceConnectorId: 'imdb:ratings_csv', sourceRecordId: `record-${index + 1}`, sourceFingerprint: `sha256-${index + 1}`,
      profileId: 'imdb_ratings', profileVersion: 1, baseUrl: 'https://neodb.example', account: 'alice', historical: true as const, executable: false as const, outcome,
    })),
  ],
};

function mockClient(overrides: Partial<Record<keyof AccessClient, unknown>> = {}) {
  return {
    previewJob: vi.fn().mockResolvedValue(job()),
    previewResults: vi.fn().mockResolvedValue(resultPage),
    submitImdbRatings: vi.fn().mockResolvedValue({ preview: { jobId: 'preview_1', attempt: 1, executable: false }, observationCount: 7, skippedRowCount: 1, skippedColumnCount: 2, duplicateRowCount: 3 }),
    submitImdbRatingsWithResolution: vi.fn().mockResolvedValue({ preview: { jobId: 'preview_2', attempt: 1, executable: false }, observationCount: 7, skippedRowCount: 0, skippedColumnCount: 0, duplicateRowCount: 0 }),
    resumePreview: vi.fn().mockResolvedValue({ jobId: 'preview_1', attempt: 3, executable: false }),
    cancelPreview: vi.fn().mockResolvedValue({ applied: true }),
    deletePreview: vi.fn().mockResolvedValue({ applied: true }),
    ...overrides,
  } as unknown as AccessClient;
}

function selectFile(file: File) {
  fireEvent.change(screen.getByLabelText('CSV 文件（UTF-8，最大 10 MiB）'), { target: { files: [file] } });
}

async function mountEligibleResolution(options: { outcome?: PreviewResultPage['items'][number]['outcome']; file?: File } = {}) {
  const state = {
    job: job({ attempt: 1, observationCount: 2 }),
    page: { ...resultPage, attempt: 1, items: [{
      ...resultPage.items[3]!, ordinal: 0, attempt: 1, sourceRecordId: 'tt1234567', outcome: options.outcome ?? { kind: 'unsupported' },
    }] } as PreviewResultPage,
  };
  const previewJob = vi.fn(() => Promise.resolve(state.job));
  const previewResults = vi.fn(() => Promise.resolve(state.page));
  const submitImdbRatingsWithResolution = vi.fn().mockResolvedValue({ preview: { jobId: 'preview_2', attempt: 1, executable: false }, observationCount: 2, skippedRowCount: 0, skippedColumnCount: 0, duplicateRowCount: 0 });
  const api = mockClient({
    previewJob,
    previewResults,
    submitImdbRatings: vi.fn().mockResolvedValue({ preview: { jobId: 'preview_1', attempt: 1, executable: false }, observationCount: 2, skippedRowCount: 0, skippedColumnCount: 0, duplicateRowCount: 0 }),
    submitImdbRatingsWithResolution,
  });
  const bytes = new TextEncoder().encode('Const,Your Rating,Title,Title Type\ntt1234567,8,Example,movie\n');
  const file = options.file ?? ({ name: 'ratings.csv', size: bytes.byteLength, arrayBuffer: vi.fn().mockResolvedValue(bytes.buffer) } as unknown as File);
  render(<PreviewPanel client={api} />);
  selectFile(file);
  fireEvent.change(screen.getByLabelText('保存的配置 ID'), { target: { value: 'imdb_ratings' } });
  fireEvent.change(screen.getByLabelText('保存的配置版本'), { target: { value: '1' } });
  fireEvent.click(screen.getByRole('button', { name: '提交只读预览' }));
  await waitFor(() => expect(screen.getByText('任务 preview_1')).toBeTruthy());
  fireEvent.click(screen.getByRole('button', { name: '读取该尝试的一页结果' }));
  await screen.findByLabelText('记录 0 的解析选择');
  return { api, state, file, previewJob, previewResults, submitImdbRatingsWithResolution };
}

describe('PreviewPanel', () => {
  it('creates new jobs from explicit search then returned manual evidence without staging I/O or editing old history', async () => {
    const selectableItem = { ...item, uuid: 'BrowserItem123' };
    const jobs: Record<string, PreviewJobView> = {
      preview_1: job({ id: 'preview_1', attempt: 1, observationCount: 1 }),
      preview_2: job({ id: 'preview_2', attempt: 1, observationCount: 1 }),
      preview_3: job({ id: 'preview_3', attempt: 1, observationCount: 1 }),
    };
    const pages: Record<string, PreviewResultPage> = {
      preview_1: { ...resultPage, jobId: 'preview_1', attempt: 1, items: [{
        ...resultPage.items[3]!, ordinal: 0, attempt: 1, sourceRecordId: 'tt1234567', outcome: { kind: 'unsupported' },
      }] },
      preview_2: { ...resultPage, jobId: 'preview_2', attempt: 1, items: [{
        ...resultPage.items[2]!, ordinal: 0, attempt: 1, sourceRecordId: 'tt1234567', outcome: { kind: 'search_candidates', candidates: [{ title: 'Legacy unsafe', item }, { title: 'Candidate', item: selectableItem }], moreAvailable: false },
      }] },
      preview_3: { ...resultPage, jobId: 'preview_3', attempt: 1, items: [{
        ...resultPage.items[0]!, ordinal: 0, attempt: 1, sourceRecordId: 'tt1234567', outcome: resultPage.items[0]!.outcome,
      }] },
    };
    const previewJob = vi.fn((id: string) => Promise.resolve(jobs[id]!));
    const previewResults = vi.fn((id: string) => Promise.resolve(pages[id]!));
    const submitImdbRatings = vi.fn().mockResolvedValue({ preview: { jobId: 'preview_1', attempt: 1, executable: false }, observationCount: 1, skippedRowCount: 0, skippedColumnCount: 0, duplicateRowCount: 0 });
    const submitImdbRatingsWithResolution = vi.fn()
      .mockResolvedValueOnce({ preview: { jobId: 'preview_2', attempt: 1, executable: false }, observationCount: 1, skippedRowCount: 0, skippedColumnCount: 0, duplicateRowCount: 0 })
      .mockResolvedValueOnce({ preview: { jobId: 'preview_3', attempt: 1, executable: false }, observationCount: 1, skippedRowCount: 0, skippedColumnCount: 0, duplicateRowCount: 0 });
    const bytes = new TextEncoder().encode('Const,Your Rating,Title,Title Type\ntt1234567,8,Example,movie\n');
    const arrayBuffer = vi.fn().mockResolvedValue(bytes.buffer);
    const api = mockClient({ previewJob, previewResults, submitImdbRatings, submitImdbRatingsWithResolution });
    render(<PreviewPanel client={api} />);
    selectFile({ name: 'ratings.csv', size: bytes.byteLength, arrayBuffer } as unknown as File);
    fireEvent.change(screen.getByLabelText('保存的配置 ID'), { target: { value: 'imdb_ratings' } });
    fireEvent.change(screen.getByLabelText('保存的配置版本'), { target: { value: '1' } });
    fireEvent.click(screen.getByRole('button', { name: '提交只读预览' }));
    await screen.findByText(/已接收任务 preview_1/);
    await waitFor(() => expect(screen.getByText('任务 preview_1')).toBeTruthy());
    fireEvent.click(screen.getByRole('button', { name: '读取该尝试的一页结果' }));
    await screen.findByLabelText('记录 0 的搜索词');

    const readsBeforeStage = arrayBuffer.mock.calls.length;
    const requestsBeforeStage = previewJob.mock.calls.length + previewResults.mock.calls.length;
    fireEvent.change(screen.getByLabelText('记录 0 的搜索词'), { target: { value: 'Example' } });
    fireEvent.click(screen.getByRole('button', { name: '暂存记录 0 的搜索' }));
    expect(arrayBuffer).toHaveBeenCalledTimes(readsBeforeStage);
    expect(previewJob.mock.calls.length + previewResults.mock.calls.length).toBe(requestsBeforeStage);
    expect(screen.getByLabelText('解析选择草稿').textContent).toContain('1 条已暂存');
    fireEvent.change(screen.getByLabelText('记录 0 的搜索词'), { target: { value: 'Bad\u0085Query' } });
    fireEvent.click(screen.getByRole('button', { name: '暂存记录 0 的搜索' }));
    expect(screen.getByRole('alert').textContent).toContain('控制字符');
    expect(screen.getByLabelText('解析选择草稿').textContent).toContain('搜索“Example”');
    fireEvent.change(screen.getByLabelText('记录 0 的搜索词'), { target: { value: 'Replacement' } });
    fireEvent.click(screen.getByRole('button', { name: '暂存记录 0 的搜索' }));
    expect(screen.getByLabelText('解析选择草稿').textContent).toContain('搜索“Replacement”');
    expect(screen.getByLabelText('解析选择草稿').textContent).not.toContain('搜索“Example”');
    fireEvent.click(screen.getByRole('button', { name: '清除记录 0 的选择' }));
    expect(screen.getByLabelText('解析选择草稿').textContent).toContain('0 条已暂存');
    fireEvent.change(screen.getByLabelText('记录 0 的搜索词'), { target: { value: 'Example' } });
    fireEvent.click(screen.getByRole('button', { name: '暂存记录 0 的搜索' }));

    fireEvent.click(screen.getByRole('button', { name: '按选择生成新预览' }));
    await waitFor(() => expect(submitImdbRatingsWithResolution).toHaveBeenCalledTimes(1));
    expect(submitImdbRatingsWithResolution.mock.calls[0]![0]).toEqual({ csv: expect.any(String), resolutionInputs: [{ kind: 'search', ordinal: 0, sourceRecordId: 'tt1234567', query: 'Example' }] });
    await screen.findByText(/新任务 preview_2/);
    await waitFor(() => expect(screen.getByText('任务 preview_2')).toBeTruthy());
    fireEvent.click(screen.getByRole('button', { name: '读取该尝试的一页结果' }));
    await screen.findByRole('button', { name: '暂存记录 0 的目录条目 BrowserItem123' });
    fireEvent.click(screen.getByRole('button', { name: '暂存记录 0 的目录条目 item-1' }));
    expect(screen.getByRole('alert').textContent).toContain('安全、完整');
    expect(screen.getByLabelText('解析选择草稿').textContent).toContain('搜索“Example”');
    fireEvent.click(screen.getByRole('button', { name: '暂存记录 0 的目录条目 BrowserItem123' }));
    expect(screen.getByLabelText('解析选择草稿').textContent).toContain('人工目录条目 BrowserItem123');
    expect(screen.getByLabelText('解析选择草稿').textContent).not.toContain('搜索“Example”');

    fireEvent.click(screen.getByRole('button', { name: '按选择生成新预览' }));
    await waitFor(() => expect(submitImdbRatingsWithResolution).toHaveBeenCalledTimes(2));
    expect(submitImdbRatingsWithResolution.mock.calls[1]![0]).toEqual({ csv: expect.any(String), resolutionInputs: [{ kind: 'manual', ordinal: 0, sourceRecordId: 'tt1234567', item: { uuid: 'BrowserItem123', mediaType: 'movie', model: 'Movie' } }] });
    expect(submitImdbRatings).toHaveBeenCalledTimes(1);
    await waitFor(() => expect(screen.getByText('任务 preview_3')).toBeTruthy());
    fireEvent.click(screen.getByRole('button', { name: '读取该尝试的一页结果' }));
    await screen.findByRole('button', { name: '清除记录 0 的选择' });
    fireEvent.click(screen.getByRole('button', { name: '清除记录 0 的选择' }));
    expect(screen.getByLabelText('解析选择草稿').textContent).toContain('0 条已暂存');
  });

  it('invalidates a staged frame when the just-in-time job preflight changes attempt', async () => {
    let remoteJob = job({ attempt: 1, observationCount: 1 });
    const page: PreviewResultPage = { ...resultPage, attempt: 1, items: [{
      ...resultPage.items[3]!, ordinal: 0, attempt: 1, sourceRecordId: 'tt1234567', outcome: { kind: 'unsupported' },
    }] };
    const previewJob = vi.fn(() => Promise.resolve(remoteJob));
    const submitImdbRatingsWithResolution = vi.fn();
    const bytes = new TextEncoder().encode('Const,Your Rating,Title,Title Type\ntt1234567,8,Example,movie\n');
    const api = mockClient({
      previewJob,
      previewResults: vi.fn().mockResolvedValue(page),
      submitImdbRatings: vi.fn().mockResolvedValue({ preview: { jobId: 'preview_1', attempt: 1, executable: false }, observationCount: 1, skippedRowCount: 0, skippedColumnCount: 0, duplicateRowCount: 0 }),
      submitImdbRatingsWithResolution,
    });
    render(<PreviewPanel client={api} />);
    selectFile({ name: 'ratings.csv', size: bytes.byteLength, arrayBuffer: vi.fn().mockResolvedValue(bytes.buffer) } as unknown as File);
    fireEvent.change(screen.getByLabelText('保存的配置 ID'), { target: { value: 'imdb_ratings' } });
    fireEvent.change(screen.getByLabelText('保存的配置版本'), { target: { value: '1' } });
    fireEvent.click(screen.getByRole('button', { name: '提交只读预览' }));
    await waitFor(() => expect(screen.getByText('任务 preview_1')).toBeTruthy());
    fireEvent.click(screen.getByRole('button', { name: '读取该尝试的一页结果' }));
    await screen.findByLabelText('记录 0 的搜索词');
    fireEvent.change(screen.getByLabelText('记录 0 的搜索词'), { target: { value: 'Example' } });
    fireEvent.click(screen.getByRole('button', { name: '暂存记录 0 的搜索' }));
    remoteJob = job({ attempt: 2, observationCount: 1 });
    fireEvent.click(screen.getByRole('button', { name: '按选择生成新预览' }));
    await waitFor(() => expect(screen.getByRole('alert').textContent).toContain('已变化'));
    expect(submitImdbRatingsWithResolution).not.toHaveBeenCalled();
    expect(screen.queryByLabelText('解析选择草稿')).toBeNull();
  });

  it('clears the old selected job and page synchronously for a raw known ambiguous receipt', async () => {
    const fixture = await mountEligibleResolution();
    expect(screen.getByText('任务 preview_1')).toBeTruthy();
    expect(screen.getByLabelText('历史结果页')).toBeTruthy();
    vi.mocked(fixture.api.submitImdbRatings).mockRejectedValueOnce(new PreviewAdmissionClientError('preview_unavailable', undefined, 'raw_known', 3));
    let resolveKnown!: (value: PreviewJobView) => void;
    fixture.previewJob.mockImplementationOnce(() => new Promise<PreviewJobView>(resolve => { resolveKnown = resolve; }));
    fireEvent.click(screen.getByRole('button', { name: '提交只读预览' }));
    await screen.findByText(/服务端返回任务 raw_known/);
    expect(screen.queryByText('任务 preview_1')).toBeNull();
    expect(screen.queryByLabelText('历史结果页')).toBeNull();
    expect((screen.getByLabelText('已有任务 ID') as HTMLInputElement).value).toBe('raw_known');
    expect(fixture.api.submitImdbRatings).toHaveBeenCalledTimes(2);
    await act(async () => { resolveKnown(job({ id: 'raw_known', attempt: 3 })); await Promise.resolve(); });
  });

  it('keeps a sparse choice across pages, enforces the 512-byte query boundary, and makes explicit reopen read-only', async () => {
    const fixture = await mountEligibleResolution();
    const exactQuery = `${'界'.repeat(170)}ab`;
    fireEvent.change(screen.getByLabelText('记录 0 的搜索词'), { target: { value: exactQuery } });
    fireEvent.click(screen.getByRole('button', { name: '暂存记录 0 的搜索' }));
    expect(screen.getByLabelText('解析选择草稿').textContent).toContain('1 条已暂存');
    fixture.state.page = { ...fixture.state.page, offset: 1, items: [] };
    fireEvent.change(screen.getByLabelText('起始记录序号'), { target: { value: '1' } });
    fireEvent.click(screen.getByRole('button', { name: '读取该尝试的一页结果' }));
    await waitFor(() => expect(screen.getByText(/起始序号 1/)).toBeTruthy());
    expect(screen.getByLabelText('解析选择草稿').textContent).toContain('1 条已暂存');
    const callsBeforeClear = fixture.previewJob.mock.calls.length + fixture.previewResults.mock.calls.length;
    fireEvent.click(screen.getByRole('button', { name: '清除草稿记录 0 的选择' }));
    expect(screen.getByLabelText('解析选择草稿').textContent).toContain('0 条已暂存');
    expect(fixture.previewJob.mock.calls.length + fixture.previewResults.mock.calls.length).toBe(callsBeforeClear);
    fireEvent.change(screen.getByLabelText('已有任务 ID'), { target: { value: 'preview_1' } });
    fireEvent.click(screen.getByRole('button', { name: '打开任务' }));
    await waitFor(() => expect(screen.queryByLabelText('解析选择草稿')).toBeNull());
    expect(fixture.submitImdbRatingsWithResolution).not.toHaveBeenCalled();
  });

  it('submits an off-page sparse choice with its original ordinal, source ID, and exact profile revision', async () => {
    const fixture = await mountEligibleResolution();
    fireEvent.change(screen.getByLabelText('记录 0 的搜索词'), { target: { value: 'Off-page search' } });
    fireEvent.click(screen.getByRole('button', { name: '暂存记录 0 的搜索' }));
    fixture.state.page = { ...fixture.state.page, offset: 1, items: [] };
    fireEvent.change(screen.getByLabelText('起始记录序号'), { target: { value: '1' } });
    fireEvent.click(screen.getByRole('button', { name: '读取该尝试的一页结果' }));
    await waitFor(() => expect(screen.getByText(/起始序号 1/)).toBeTruthy());
    fireEvent.click(screen.getByRole('button', { name: '按选择生成新预览' }));
    await waitFor(() => expect(fixture.submitImdbRatingsWithResolution).toHaveBeenCalledTimes(1));
    expect(fixture.submitImdbRatingsWithResolution).toHaveBeenCalledWith({
      csv: expect.any(String),
      resolutionInputs: [{ kind: 'search', ordinal: 0, sourceRecordId: 'tt1234567', query: 'Off-page search' }],
    }, { profileId: 'imdb_ratings', profileVersion: 1 }, expect.any(AbortSignal));
  });

  it('invalidates local resolution ownership on profile, file, binding, and row-frame changes', async () => {
    const profileFixture = await mountEligibleResolution();
    fireEvent.change(screen.getByLabelText('保存的配置版本'), { target: { value: '2' } });
    expect(screen.queryByLabelText('解析选择草稿')).toBeNull();
    cleanup();

    await mountEligibleResolution();
    selectFile(new File(['x'], 'other.csv', { type: 'text/csv' }));
    expect(screen.queryByLabelText('解析选择草稿')).toBeNull();
    cleanup();

    const bindingFixture = await mountEligibleResolution();
    bindingFixture.state.job = job({ attempt: 1, observationCount: 2, status: 'running' });
    fireEvent.click(screen.getByRole('button', { name: '读取状态' }));
    await waitFor(() => expect((screen.getByRole('button', { name: '按选择生成新预览' }) as HTMLButtonElement).disabled).toBe(true));
    bindingFixture.state.job = job({ attempt: 1, observationCount: 2, bindingState: 'changed' });
    fireEvent.click(screen.getByRole('button', { name: '读取状态' }));
    await waitFor(() => expect(screen.queryByLabelText('解析选择草稿')).toBeNull());
    cleanup();

    const rowFixture = await mountEligibleResolution();
    rowFixture.state.page = { ...rowFixture.state.page, items: [{ ...rowFixture.state.page.items[0]!, account: 'mallory' }] };
    fireEvent.click(screen.getByRole('button', { name: '读取该尝试的一页结果' }));
    await waitFor(() => expect(screen.queryByLabelText('解析选择草稿')).toBeNull());
    expect(profileFixture.submitImdbRatingsWithResolution).not.toHaveBeenCalled();
  });

  it.each([
    ['exact', 0, true],
    ['plus one', 1, false],
  ] as const)('%s encoded JSON boundary includes escaping before preflight and transport', async (_label, extraByte, admitted) => {
    const small = new TextEncoder().encode('Const,Your Rating,Title,Title Type\ntt1234567,8,Example,movie\n');
    const query = 'quote " and slash \\';
    const choice = { kind: 'search' as const, ordinal: 0, sourceRecordId: 'tt1234567', query };
    const marker = 'csv "escaped" \\ value\n';
    const emptySize = new TextEncoder().encode(JSON.stringify({ csv: marker, resolutionInputs: [choice] })).byteLength;
    const csv = `${marker}${'a'.repeat(10 * 1024 * 1024 - emptySize + extraByte)}`;
    expect(new TextEncoder().encode(JSON.stringify({ csv, resolutionInputs: [choice] })).byteLength).toBe(10 * 1024 * 1024 + extraByte);
    const encodedCsv = new TextEncoder().encode(csv);
    const arrayBuffer = vi.fn().mockResolvedValueOnce(small.buffer).mockResolvedValueOnce(encodedCsv.buffer);
    const fixture = await mountEligibleResolution({ file: { name: 'ratings.csv', size: small.byteLength, arrayBuffer } as unknown as File });
    fireEvent.change(screen.getByLabelText('记录 0 的搜索词'), { target: { value: query } });
    fireEvent.click(screen.getByRole('button', { name: '暂存记录 0 的搜索' }));
    const preflightCalls = fixture.previewJob.mock.calls.length;
    fireEvent.click(screen.getByRole('button', { name: '按选择生成新预览' }));
    if (admitted) {
      await waitFor(() => expect(fixture.submitImdbRatingsWithResolution).toHaveBeenCalledTimes(1));
      expect(fixture.previewJob.mock.calls.length).toBeGreaterThan(preflightCalls);
      expect(fixture.submitImdbRatingsWithResolution.mock.calls[0]![0]).toEqual({ csv, resolutionInputs: [choice] });
    } else {
      await waitFor(() => expect(screen.getByRole('alert').textContent).toContain('JSON 请求超过 10 MiB'));
      expect(fixture.previewJob).toHaveBeenCalledTimes(preflightCalls);
      expect(fixture.submitImdbRatingsWithResolution).not.toHaveBeenCalled();
      fireEvent.click(screen.getByRole('button', { name: '读取状态' }));
      await waitFor(() => expect(fixture.previewJob).toHaveBeenCalledTimes(preflightCalls + 1));
    }
  });

  it.each([
    ['File replacement', 'fulfill'],
    ['profile replacement', 'reject'],
    ['unmount', 'fulfill'],
  ] as const)('does not preflight or POST when structured File reading settles after %s (%s)', async (change, mode) => {
    const small = new TextEncoder().encode('Const,Your Rating,Title,Title Type\ntt1234567,8,Example,movie\n');
    let settle!: (value: ArrayBuffer | Error) => void;
    const pending = new Promise<ArrayBuffer>((resolve, reject) => { settle = value => mode === 'fulfill' ? resolve(value as ArrayBuffer) : reject(value); });
    const arrayBuffer = vi.fn().mockResolvedValueOnce(small.buffer).mockReturnValueOnce(pending);
    const fixture = await mountEligibleResolution({ file: { name: 'ratings.csv', size: small.byteLength, arrayBuffer } as unknown as File });
    fireEvent.change(screen.getByLabelText('记录 0 的搜索词'), { target: { value: 'Example' } });
    fireEvent.click(screen.getByRole('button', { name: '暂存记录 0 的搜索' }));
    const preflightCalls = fixture.previewJob.mock.calls.length;
    fireEvent.click(screen.getByRole('button', { name: '按选择生成新预览' }));
    await waitFor(() => expect(arrayBuffer).toHaveBeenCalledTimes(2));
    if (change === 'File replacement') selectFile(new File(['changed'], 'changed.csv', { type: 'text/csv' }));
    else if (change === 'profile replacement') fireEvent.change(screen.getByLabelText('保存的配置版本'), { target: { value: '2' } });
    else cleanup();
    settle(mode === 'fulfill' ? small.buffer : new Error('late file read'));
    await act(async () => { try { await pending; } catch { /* Component consumes stale file rejection. */ } });
    if (change !== 'unmount') await waitFor(() => expect(fixture.previewJob).toHaveBeenCalledTimes(preflightCalls));
    expect(fixture.submitImdbRatingsWithResolution).not.toHaveBeenCalled();
  });

  it('does not let a stale structured File read suppress an explicitly opened new job', async () => {
    const small = new TextEncoder().encode('Const,Your Rating,Title,Title Type\ntt1234567,8,Example,movie\n');
    let settle!: (value: ArrayBuffer) => void;
    const pending = new Promise<ArrayBuffer>(resolve => { settle = resolve; });
    const arrayBuffer = vi.fn().mockResolvedValueOnce(small.buffer).mockReturnValueOnce(pending);
    const fixture = await mountEligibleResolution({ file: { name: 'ratings.csv', size: small.byteLength, arrayBuffer } as unknown as File });
    fireEvent.change(screen.getByLabelText('记录 0 的搜索词'), { target: { value: 'Example' } });
    fireEvent.click(screen.getByRole('button', { name: '暂存记录 0 的搜索' }));
    fireEvent.click(screen.getByRole('button', { name: '按选择生成新预览' }));
    await waitFor(() => expect(arrayBuffer).toHaveBeenCalledTimes(2));

    fixture.state.job = job({ id: 'new_job', attempt: 7 });
    fireEvent.change(screen.getByLabelText('已有任务 ID'), { target: { value: 'new_job' } });
    fireEvent.submit(screen.getByRole('button', { name: '打开任务' }).closest('form')!);
    settle(small.buffer);
    await act(async () => { await pending; });

    await waitFor(() => expect(fixture.previewJob).toHaveBeenCalledWith('new_job', expect.any(AbortSignal)));
    expect(await screen.findByText('任务 new_job')).toBeTruthy();
    expect(fixture.submitImdbRatingsWithResolution).not.toHaveBeenCalled();
  });

  it.each([
    ['known', new PreviewAdmissionClientError('preview_unavailable', undefined, 'preview_known', 1)],
    ['unknown', new AccessClientError('unavailable')],
    ['auth', new AccessClientError('csrf_invalid')],
  ] as const)('does not retry or retain ownership after a %s structured admission rejection', async (_kind, rejection) => {
    const fixture = await mountEligibleResolution();
    fixture.submitImdbRatingsWithResolution.mockRejectedValue(rejection);
    fireEvent.change(screen.getByLabelText('记录 0 的搜索词'), { target: { value: 'Example' } });
    fireEvent.click(screen.getByRole('button', { name: '暂存记录 0 的搜索' }));
    fireEvent.click(screen.getByRole('button', { name: '按选择生成新预览' }));
    await waitFor(() => expect(fixture.submitImdbRatingsWithResolution).toHaveBeenCalledTimes(1));
    await waitFor(() => expect(screen.queryByLabelText('解析选择草稿')).toBeNull());
    expect(fixture.submitImdbRatingsWithResolution).toHaveBeenCalledTimes(1);
    if (rejection instanceof PreviewAdmissionClientError) expect(await screen.findByText(/preview_known/)).toBeTruthy();
    if (rejection instanceof AccessClientError && rejection.code === 'csrf_invalid') expect(screen.getAllByText(/本地授权已失效/).length).toBeGreaterThan(0);
  });

  it.each(['fulfill', 'reject'] as const)('ignores stale structured %s after File replacement and blocks same-tick double submission', async mode => {
    let settle!: (value?: unknown) => void;
    const pending = new Promise((resolve, reject) => { settle = mode === 'fulfill' ? resolve : reject; });
    const fixture = await mountEligibleResolution();
    fixture.submitImdbRatingsWithResolution.mockReturnValue(pending);
    fireEvent.change(screen.getByLabelText('记录 0 的搜索词'), { target: { value: 'Example' } });
    fireEvent.click(screen.getByRole('button', { name: '暂存记录 0 的搜索' }));
    const button = screen.getByRole('button', { name: '按选择生成新预览' });
    fireEvent.click(button); fireEvent.click(button);
    await waitFor(() => expect(fixture.submitImdbRatingsWithResolution).toHaveBeenCalledTimes(1));
    selectFile(new File(['changed'], 'changed.csv', { type: 'text/csv' }));
    settle(mode === 'fulfill' ? { preview: { jobId: 'stale_job', attempt: 1, executable: false }, observationCount: 2, skippedRowCount: 0, skippedColumnCount: 0, duplicateRowCount: 0 } : new AccessClientError('csrf_invalid'));
    await act(async () => { try { await pending; } catch { /* Component consumes stale rejection. */ } });
    expect(screen.queryByText('任务 stale_job')).toBeNull();
    expect(screen.queryByLabelText('解析选择草稿')).toBeNull();
    expect(screen.queryByText(/请重新授权后再检查状态/)).toBeNull();
  });

  it.each(['fulfill', 'reject'] as const)('ignores a stale structured preflight %s after profile replacement', async mode => {
    const fixture = await mountEligibleResolution();
    fireEvent.change(screen.getByLabelText('记录 0 的搜索词'), { target: { value: 'Example' } });
    fireEvent.click(screen.getByRole('button', { name: '暂存记录 0 的搜索' }));
    let settle!: (value?: unknown) => void;
    const pending = new Promise((resolve, reject) => { settle = mode === 'fulfill' ? resolve : reject; });
    fixture.previewJob.mockImplementationOnce(() => pending as Promise<PreviewJobView>);
    fireEvent.click(screen.getByRole('button', { name: '按选择生成新预览' }));
    await waitFor(() => expect(fixture.previewJob.mock.results.at(-1)?.value).toBe(pending));
    fireEvent.change(screen.getByLabelText('保存的配置版本'), { target: { value: '2' } });
    settle(mode === 'fulfill' ? fixture.state.job : new AccessClientError('csrf_invalid'));
    await act(async () => { try { await pending; } catch { /* The panel owns the rejected preflight. */ } });
    expect(fixture.submitImdbRatingsWithResolution).not.toHaveBeenCalled();
    expect(screen.queryByText(/请重新授权后再检查状态/)).toBeNull();
    expect(screen.queryByLabelText('解析选择草稿')).toBeNull();
  });

  it('aborts and ignores a pending structured submission on unmount', async () => {
    let reject!: (reason: unknown) => void;
    const pending = new Promise((_resolve, rejectPromise) => { reject = rejectPromise; });
    const fixture = await mountEligibleResolution();
    fixture.submitImdbRatingsWithResolution.mockReturnValue(pending);
    fireEvent.change(screen.getByLabelText('记录 0 的搜索词'), { target: { value: 'Example' } });
    fireEvent.click(screen.getByRole('button', { name: '暂存记录 0 的搜索' }));
    fireEvent.click(screen.getByRole('button', { name: '按选择生成新预览' }));
    await waitFor(() => expect(fixture.submitImdbRatingsWithResolution).toHaveBeenCalledTimes(1));
    const signal = fixture.submitImdbRatingsWithResolution.mock.calls[0]![2] as AbortSignal;
    expect(signal.aborted).toBe(false);
    cleanup();
    expect(signal.aborted).toBe(true);
    reject(new AccessClientError('csrf_invalid'));
    await act(async () => { try { await pending; } catch { /* Component consumes this after unmount. */ } });
    expect(fixture.submitImdbRatingsWithResolution).toHaveBeenCalledTimes(1);
  });

  it('does nothing on mount or StrictMode replay and reads raw BOM only after an exact revision is valid', async () => {
    const api = mockClient();
    const bytes = new TextEncoder().encode('\uFEFFTitle,Const,Your Rating\nA,tt1,8\n');
    const arrayBuffer = vi.fn().mockResolvedValue(bytes.buffer);
    const file = { name: 'ratings.csv', size: bytes.byteLength, arrayBuffer } as unknown as File;
    render(<StrictMode><PreviewPanel client={api} /></StrictMode>);
    expect(api.previewJob).not.toHaveBeenCalled();
    expect(api.submitImdbRatings).not.toHaveBeenCalled();

    selectFile(file);
    fireEvent.change(screen.getByLabelText('保存的配置 ID'), { target: { value: 'imdb_ratings' } });
    fireEvent.submit(screen.getByRole('button', { name: '提交只读预览' }).closest('form')!);
    expect(arrayBuffer).not.toHaveBeenCalled();
    expect((await screen.findByRole('alert')).textContent).toContain('明确填写');

    fireEvent.change(screen.getByLabelText('保存的配置版本'), { target: { value: '1' } });
    fireEvent.click(screen.getByRole('button', { name: '提交只读预览' }));
    await waitFor(() => expect(api.submitImdbRatings).toHaveBeenCalledTimes(1));
    expect(api.submitImdbRatings).toHaveBeenCalledWith(expect.any(String), { profileId: 'imdb_ratings', profileVersion: 1 }, expect.any(AbortSignal));
    const sent = vi.mocked(api.submitImdbRatings).mock.calls[0]![0];
    expect(sent.charCodeAt(0)).toBe(0xfeff);
    expect(sent.slice(1)).toBe('Title,Const,Your Rating\nA,tt1,8\n');
    expect(await screen.findByText(/已接收任务 preview_1/)).toBeTruthy();
    await waitFor(() => expect(api.previewJob).toHaveBeenCalledWith('preview_1', expect.any(AbortSignal)));
  });

  it('rejects declared oversize and invalid UTF-8 files without submitting', async () => {
    const api = mockClient();
    render(<PreviewPanel client={api} />);
    selectFile({ name: 'large.csv', size: 10 * 1024 * 1024 + 1, arrayBuffer: vi.fn() } as unknown as File);
    expect(screen.getByRole('alert').textContent).toContain('超过 10 MiB');

    const invalid = new Uint8Array([0xc3, 0x28]);
    selectFile({ name: 'invalid.csv', size: 2, arrayBuffer: vi.fn().mockResolvedValue(invalid.buffer) } as unknown as File);
    fireEvent.change(screen.getByLabelText('保存的配置 ID'), { target: { value: 'p' } });
    fireEvent.change(screen.getByLabelText('保存的配置版本'), { target: { value: '1' } });
    fireEvent.click(screen.getByRole('button', { name: '提交只读预览' }));
    expect((await screen.findByRole('alert')).textContent).toContain('不是有效 UTF-8');
    expect(api.submitImdbRatings).not.toHaveBeenCalled();
  });

  it('refuses unsafe or out-of-grammar job IDs before any client read', () => {
    const api = mockClient();
    render(<PreviewPanel client={api} />);
    for (const id of ['.', '..', 'job/path', '任务']) {
      fireEvent.change(screen.getByLabelText('已有任务 ID'), { target: { value: id } });
      fireEvent.click(screen.getByRole('button', { name: '打开任务' }));
      expect(screen.getByRole('alert').textContent).toContain('任务 ID 必须');
    }
    expect(api.previewJob).not.toHaveBeenCalled();
  });

  it('rechecks actual CSV bytes at the exact 10 MiB boundary', async () => {
    const api = mockClient();
    render(<PreviewPanel client={api} />);
    fireEvent.change(screen.getByLabelText('保存的配置 ID'), { target: { value: 'p' } });
    fireEvent.change(screen.getByLabelText('保存的配置版本'), { target: { value: '1' } });
    const tooLarge = new Uint8Array(10 * 1024 * 1024 + 1); tooLarge.fill(0x61);
    selectFile({ name: 'lying.csv', size: 1, arrayBuffer: vi.fn().mockResolvedValue(tooLarge.buffer) } as unknown as File);
    fireEvent.click(screen.getByRole('button', { name: '提交只读预览' }));
    expect((await screen.findByRole('alert')).textContent).toContain('超过 10 MiB');
    expect(api.submitImdbRatings).not.toHaveBeenCalled();

    const exact = new Uint8Array(10 * 1024 * 1024); exact.fill(0x61);
    selectFile({ name: 'exact.csv', size: exact.byteLength, arrayBuffer: vi.fn().mockResolvedValue(exact.buffer) } as unknown as File);
    fireEvent.click(screen.getByRole('button', { name: '提交只读预览' }));
    await waitFor(() => expect(api.submitImdbRatings).toHaveBeenCalledTimes(1));
    expect(vi.mocked(api.submitImdbRatings).mock.calls[0]![0].length).toBe(10 * 1024 * 1024);
  });

  it('renders every historical outcome, identity and Known/Empty/Unknown field state', async () => {
    const api = mockClient();
    render(<PreviewPanel client={api} />);
    fireEvent.change(screen.getByLabelText('已有任务 ID'), { target: { value: 'preview_1' } });
    fireEvent.click(screen.getByRole('button', { name: '打开任务' }));
    expect(await screen.findByText('任务 preview_1')).toBeTruthy();
    expect(screen.getByText(/账户绑定：current/)).toBeTruthy();
    fireEvent.click(screen.getByRole('button', { name: '读取该尝试的一页结果' }));
    expect(await screen.findByText('Known · 8')).toBeTruthy();
    expect(screen.getByText('Empty · 已确认为空')).toBeTruthy();
    expect(screen.getByText('Unknown · 无法确认')).toBeTruthy();
    expect(screen.getByText((_, element) => element?.tagName === 'P' && element.textContent?.includes('证据来源：人工') === true)).toBeTruthy();
    expect(screen.getByText(/需要检查/)).toBeTruthy();
    expect(screen.getByText(/搜索候选/)).toBeTruthy();
    expect(screen.getByText(/不支持/)).toBeTruthy();
    expect(screen.getByText(/目录服务限流/)).toBeTruthy();
    expect(screen.getByText(/目录服务不可用/)).toBeTruthy();
    expect(screen.getAllByText('imdb:title').length).toBeGreaterThan(0);
    expect(screen.queryByRole('link', { name: 'javascript:alert(1)' })).toBeNull();
    expect(screen.getByText('javascript:alert(1)')).toBeTruthy();
  });

  it('rejects mismatched page identities and keeps an ambiguous known ID for inspection without retry', async () => {
    const api = mockClient({
      submitImdbRatings: vi.fn().mockRejectedValue(new PreviewAdmissionClientError('preview_admission_failed', undefined, 'known_job', 4)),
      previewJob: vi.fn().mockImplementation((id: string) => Promise.resolve(job({ id, attempt: id === 'known_job' ? 4 : 2 }))),
      previewResults: vi.fn().mockResolvedValue({ ...resultPage, jobId: 'other_job' }),
    });
    render(<PreviewPanel client={api} />);
    const bytes = new TextEncoder().encode('Title,Const,Your Rating\n');
    selectFile({ name: 'ratings.csv', size: bytes.byteLength, arrayBuffer: vi.fn().mockResolvedValue(bytes.buffer) } as unknown as File);
    fireEvent.change(screen.getByLabelText('保存的配置 ID'), { target: { value: 'imdb_ratings' } });
    fireEvent.change(screen.getByLabelText('保存的配置版本'), { target: { value: '1' } });
    fireEvent.click(screen.getByRole('button', { name: '提交只读预览' }));
    expect(await screen.findByText(/保留任务 known_job|服务端返回任务 known_job/)).toBeTruthy();
    expect(api.submitImdbRatings).toHaveBeenCalledTimes(1);
    expect(await screen.findByText('任务 known_job')).toBeTruthy();
    fireEvent.click(screen.getByRole('button', { name: '读取该尝试的一页结果' }));
    expect((await screen.findByRole('alert')).textContent).toContain('invalid_response');
    expect(screen.queryByLabelText('历史结果页')).toBeNull();
    expect(api.submitImdbRatings).toHaveBeenCalledTimes(1);
  });

  it('polls without overlap, ignores a stale selection and stops on terminal status', async () => {
    let resolveFirst!: (value: PreviewJobView) => void;
    let resolveSecond!: (value: PreviewJobView) => void;
    const previewJob = vi.fn()
      .mockImplementationOnce(() => new Promise<PreviewJobView>(resolve => { resolveFirst = resolve; }))
      .mockImplementationOnce(() => new Promise<PreviewJobView>(resolve => { resolveSecond = resolve; }));
    const api = mockClient({ previewJob });
    render(<PreviewPanel client={api} />);
    fireEvent.change(screen.getByLabelText('已有任务 ID'), { target: { value: 'old_job' } });
    fireEvent.click(screen.getByRole('button', { name: '打开任务' }));
    await waitFor(() => expect(previewJob).toHaveBeenCalledTimes(1));
    fireEvent.change(screen.getByLabelText('已有任务 ID'), { target: { value: 'new_job' } });
    fireEvent.click(screen.getByRole('button', { name: '打开任务' }));
    await waitFor(() => expect(previewJob).toHaveBeenCalledTimes(2));
    resolveFirst(job({ id: 'old_job', status: 'running' }));
    resolveSecond(job({ id: 'new_job', status: 'completed' }));
    expect(await screen.findByText('任务 new_job')).toBeTruthy();
    expect(screen.queryByText('任务 old_job')).toBeNull();
    expect(previewJob).toHaveBeenCalledTimes(2);
  });

  it('uses a four-second cadence, never overlaps a hung poll and aborts on unmount', async () => {
    vi.useFakeTimers();
    let resolveHung!: (value: PreviewJobView) => void;
    const previewJob = vi.fn()
      .mockResolvedValueOnce(job({ status: 'running' }))
      .mockImplementationOnce(() => new Promise<PreviewJobView>(resolve => { resolveHung = resolve; }));
    const api = mockClient({ previewJob });
    const view = render(<PreviewPanel client={api} />);
    fireEvent.change(screen.getByLabelText('已有任务 ID'), { target: { value: 'preview_1' } });
    fireEvent.click(screen.getByRole('button', { name: '打开任务' }));
    await act(async () => { await Promise.resolve(); await Promise.resolve(); });
    expect(previewJob).toHaveBeenCalledTimes(1);
    await act(async () => { await vi.advanceTimersByTimeAsync(3_999); });
    expect(previewJob).toHaveBeenCalledTimes(1);
    await act(async () => { await vi.advanceTimersByTimeAsync(1); });
    expect(previewJob).toHaveBeenCalledTimes(2);
    await act(async () => { await vi.advanceTimersByTimeAsync(12_000); });
    expect(previewJob).toHaveBeenCalledTimes(2);
    const hungSignal = vi.mocked(previewJob).mock.calls[1]![1] as AbortSignal;
    view.unmount();
    expect(hungSignal.aborted).toBe(true);
    resolveHung(job({ status: 'completed' }));
    await act(async () => { await Promise.resolve(); });
  });

  it('clears an old page when a refreshed attempt changes and explains empty sparse pages truthfully', async () => {
    let currentJob = job();
    const previewJob = vi.fn().mockImplementation(() => Promise.resolve(currentJob));
    const api = mockClient({ previewJob, previewResults: vi.fn().mockResolvedValue({ ...resultPage, items: [] }) });
    render(<PreviewPanel client={api} />);
    fireEvent.change(screen.getByLabelText('已有任务 ID'), { target: { value: 'preview_1' } });
    fireEvent.click(screen.getByRole('button', { name: '打开任务' }));
    await screen.findByText('任务 preview_1');
    fireEvent.click(screen.getByRole('button', { name: '读取该尝试的一页结果' }));
    expect(await screen.findByText(/本页为空；这不表示任务完成/)).toBeTruthy();
    await waitFor(() => expect(previewJob.mock.calls.length).toBeGreaterThanOrEqual(2));
    await waitFor(() => expect((screen.getByRole('button', { name: '读取状态' }) as HTMLButtonElement).disabled).toBe(false));
    expect(screen.getByText('尝试').nextElementSibling?.textContent).toBe('2');
    expect(screen.queryByRole('alert')).toBeNull();
    currentJob = job({ attempt: 3, status: 'interrupted' });
    fireEvent.click(screen.getByRole('button', { name: '读取状态' }));
    await waitFor(() => expect(screen.getByText('尝试').nextElementSibling?.textContent).toBe('3'));
    expect(screen.queryByLabelText('历史结果页')).toBeNull();
    expect(screen.queryByRole('alert')).toBeNull();
  });

  it('prevents same-tick duplicate submission and never submits after a deferred file read is unmounted', async () => {
    let resolveAdmission!: (value: unknown) => void;
    const submitImdbRatings = vi.fn().mockImplementation(() => new Promise(resolve => { resolveAdmission = resolve; }));
    const api = mockClient({ submitImdbRatings });
    const bytes = new TextEncoder().encode('Title,Const,Your Rating\n');
    const view = render(<PreviewPanel client={api} />);
    selectFile({ name: 'ratings.csv', size: bytes.byteLength, arrayBuffer: vi.fn().mockResolvedValue(bytes.buffer) } as unknown as File);
    fireEvent.change(screen.getByLabelText('保存的配置 ID'), { target: { value: 'p' } });
    fireEvent.change(screen.getByLabelText('保存的配置版本'), { target: { value: '1' } });
    const form = screen.getByRole('button', { name: '提交只读预览' }).closest('form')!;
    fireEvent.submit(form); fireEvent.submit(form);
    await waitFor(() => expect(submitImdbRatings).toHaveBeenCalledTimes(1));
    resolveAdmission({ preview: { jobId: 'preview_1', attempt: 1, executable: false }, observationCount: 0, skippedRowCount: 0, skippedColumnCount: 0, duplicateRowCount: 0 });
    await act(async () => { await Promise.resolve(); });
    view.unmount();

    let resolveRead!: (value: ArrayBuffer) => void;
    const secondApi = mockClient();
    const second = render(<PreviewPanel client={secondApi} />);
    selectFile({ name: 'slow.csv', size: bytes.byteLength, arrayBuffer: vi.fn().mockImplementation(() => new Promise(resolve => { resolveRead = resolve; })) } as unknown as File);
    fireEvent.change(screen.getByLabelText('保存的配置 ID'), { target: { value: 'p' } });
    fireEvent.change(screen.getByLabelText('保存的配置版本'), { target: { value: '1' } });
    fireEvent.click(screen.getByRole('button', { name: '提交只读预览' }));
    second.unmount();
    resolveRead(bytes.buffer);
    await act(async () => { await Promise.resolve(); });
    expect(secondApi.submitImdbRatings).not.toHaveBeenCalled();
  });

  it('keeps a no-receipt mutation uncertainty when old-job reconciliation also fails', async () => {
    const previewJob = vi.fn()
      .mockResolvedValueOnce(job({ id: 'old_job', status: 'running' }))
      .mockRejectedValue(new AccessClientError('preview_unavailable'));
    const api = mockClient({ previewJob, submitImdbRatings: vi.fn().mockRejectedValue(new AccessClientError('unavailable')) });
    render(<PreviewPanel client={api} />);
    fireEvent.change(screen.getByLabelText('已有任务 ID'), { target: { value: 'old_job' } });
    fireEvent.click(screen.getByRole('button', { name: '打开任务' }));
    await screen.findByText('任务 old_job');
    const bytes = new TextEncoder().encode('Title,Const,Your Rating\n');
    selectFile({ name: 'ratings.csv', size: bytes.byteLength, arrayBuffer: vi.fn().mockResolvedValue(bytes.buffer) } as unknown as File);
    fireEvent.change(screen.getByLabelText('保存的配置 ID'), { target: { value: 'p' } });
    fireEvent.change(screen.getByLabelText('保存的配置版本'), { target: { value: '1' } });
    fireEvent.click(screen.getByRole('button', { name: '提交只读预览' }));
    await waitFor(() => expect(previewJob).toHaveBeenCalledTimes(2));
    expect(screen.getByRole('alert').textContent).toContain('操作结果未能确认（unavailable）');
    expect(api.submitImdbRatings).toHaveBeenCalledTimes(1);
  });

  it('aborts pending observation before dispatching a mutation even if the port ignores abort', async () => {
    let resolveOld!: (value: PreviewJobView) => void;
    let oldSignal: AbortSignal | undefined;
    let abortedBeforeDispatch = false;
    const previewJob = vi.fn()
      .mockResolvedValueOnce(job({ status: 'running' }))
      .mockImplementationOnce((_id: string, signal: AbortSignal) => {
        oldSignal = signal;
        return new Promise<PreviewJobView>(resolve => { resolveOld = resolve; });
      })
      .mockResolvedValue(job({ status: 'cancelled' }));
    const cancelPreview = vi.fn().mockImplementation(() => {
      abortedBeforeDispatch = oldSignal?.aborted === true;
      resolveOld(job({ attempt: 99, status: 'running' }));
      return Promise.resolve({ applied: true });
    });
    render(<PreviewPanel client={mockClient({ previewJob, cancelPreview })} />);
    fireEvent.change(screen.getByLabelText('已有任务 ID'), { target: { value: 'preview_1' } });
    fireEvent.click(screen.getByRole('button', { name: '打开任务' }));
    await screen.findByText('任务 preview_1');
    fireEvent.click(screen.getByRole('button', { name: '读取状态' }));
    await waitFor(() => expect(previewJob).toHaveBeenCalledTimes(2));
    fireEvent.click(screen.getByRole('button', { name: '取消尝试 2' }));
    expect(abortedBeforeDispatch).toBe(true);
    expect(await screen.findByText('cancelled', { exact: true })).toBeTruthy();
    expect(screen.queryByRole('button', { name: '取消尝试 99' })).toBeNull();
    expect(screen.queryByRole('alert')).toBeNull();
  });

  it('serializes attempt-bound management, reconciles refusal and locks mutations after auth expiry', async () => {
    const cancelPreview = vi.fn().mockResolvedValueOnce({ applied: false }).mockRejectedValueOnce(new AccessClientError('authentication_required'));
    const api = mockClient({ previewJob: vi.fn().mockResolvedValue(job({ status: 'running' })), cancelPreview });
    render(<PreviewPanel client={api} />);
    fireEvent.change(screen.getByLabelText('已有任务 ID'), { target: { value: 'preview_1' } });
    fireEvent.click(screen.getByRole('button', { name: '打开任务' }));
    const cancel = await screen.findByRole('button', { name: '取消尝试 2' });
    fireEvent.click(cancel);
    expect(await screen.findByText(/未应用取消/)).toBeTruthy();
    expect(cancelPreview).toHaveBeenLastCalledWith('preview_1', 2, expect.any(AbortSignal));
    await waitFor(() => expect(cancel.hasAttribute('disabled')).toBe(false));
    fireEvent.click(cancel);
    expect(await screen.findByText(/所有变更已停用/)).toBeTruthy();
    expect(cancel.hasAttribute('disabled')).toBe(true);
    fireEvent.click(cancel);
    expect(cancelPreview).toHaveBeenCalledTimes(2);
    expect(screen.getAllByText(/历史证据/).length).toBeGreaterThan(0);
  });

  it('requires confirmation before deleting operational history', async () => {
    const confirm = vi.spyOn(window, 'confirm').mockReturnValue(false);
    const api = mockClient();
    render(<PreviewPanel client={api} />);
    fireEvent.change(screen.getByLabelText('已有任务 ID'), { target: { value: 'preview_1' } });
    fireEvent.click(screen.getByRole('button', { name: '打开任务' }));
    fireEvent.click(await screen.findByRole('button', { name: '删除运行历史' }));
    expect(confirm).toHaveBeenCalled();
    expect(api.deletePreview).not.toHaveBeenCalled();
    confirm.mockReturnValue(true);
    fireEvent.click(screen.getByRole('button', { name: '删除运行历史' }));
    expect(await screen.findByText('服务器确认运行历史已删除；公共目录条目不受影响。')).toBeTruthy();
  });

  it('renders a schema-valid Unix timestamp outside Date range without crashing', async () => {
    const api = mockClient({ previewJob: vi.fn().mockResolvedValue(job({ createdAt: 9_000_000_000_000_000_000 })) });
    render(<PreviewPanel client={api} />);
    fireEvent.change(screen.getByLabelText('已有任务 ID'), { target: { value: 'preview_1' } });
    fireEvent.click(screen.getByRole('button', { name: '打开任务' }));
    expect(await screen.findByText('Unix 9000000000000000000')).toBeTruthy();
    expect(screen.queryByRole('alert')).toBeNull();
  });
});
