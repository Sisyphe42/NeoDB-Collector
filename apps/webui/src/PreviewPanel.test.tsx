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
    resumePreview: vi.fn().mockResolvedValue({ jobId: 'preview_1', attempt: 3, executable: false }),
    cancelPreview: vi.fn().mockResolvedValue({ applied: true }),
    deletePreview: vi.fn().mockResolvedValue({ applied: true }),
    ...overrides,
  } as unknown as AccessClient;
}

function selectFile(file: File) {
  fireEvent.change(screen.getByLabelText('CSV 文件（UTF-8，最大 10 MiB）'), { target: { files: [file] } });
}

describe('PreviewPanel', () => {
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
