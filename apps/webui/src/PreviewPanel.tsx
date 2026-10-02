import { useEffect, useRef, useState, type FormEvent, type ReactNode } from 'react';
import {
  AccessClientError,
  PreviewAdmissionClientError,
  type AccessClient,
  type PreviewJobView,
  type PreviewResultPage,
} from '@neodb-collector/core-client';

const CSV_LIMIT = 10 * 1024 * 1024;
const POLL_DELAY_MS = 4_000;
const LOCAL_ID = /^[A-Za-z0-9_.:-]{1,128}$/;
type Result = PreviewResultPage['items'][number];
type Outcome = Result['outcome'];
type CatalogItem = Extract<Outcome, { item: unknown }>['item'];

function errorText(error: unknown, uncertain = false) {
  const code = error instanceof AccessClientError ? error.code : 'unavailable';
  const field = error instanceof AccessClientError && error.field ? `（字段：${error.field}）` : '';
  if (code === 'authentication_required' || code === 'csrf_invalid') return `本地授权已失效${field}。请重新授权后再检查状态。`;
  if (code === 'preview_job_not_found') return `未找到该预览任务，可能已删除或过期${field}。`;
  if (code === 'preview_attempt_changed') return `任务尝试号已变化${field}。请重新读取状态后再操作。`;
  if (code === 'mapping_profile_not_found') return `找不到指定的配置 ID 与版本${field}；不会改用最新版本。`;
  if (code === 'preview_runtime_busy') return '预览队列已满，未自动重试。';
  if (code === 'neodb_not_connected') return 'NeoDB 账户尚未连接，未提交预览。';
  return uncertain
    ? `操作结果未能确认（${code}${field}）。请读取任务状态核对；系统不会自动重试。`
    : `请求失败（${code}${field}）。未自动重试。`;
}

function timestamp(value: number) {
  const date = new Date(value * 1000);
  if (!Number.isFinite(value) || Number.isNaN(date.valueOf())) return <span>Unix {String(value)}</span>;
  return <time dateTime={date.toISOString()}>{date.toLocaleString()}</time>;
}

function scalar(value: string | number | boolean | null | undefined) {
  if (value === undefined) return '未提供';
  if (value === null) return '空';
  return typeof value === 'string' ? value : JSON.stringify(value);
}

function CatalogEvidence({ item }: { item: CatalogItem }) {
  let canonical: ReactNode = item.canonicalUrl;
  try {
    if (new URL(item.canonicalUrl).protocol === 'https:') canonical = <a href={item.canonicalUrl} rel="noreferrer" target="_blank">{item.canonicalUrl}</a>;
  } catch { /* Generated validation owns shape; unsafe URLs remain inert text. */ }
  return <div className="preview-catalog">
    <dl>
      <div><dt>NeoDB UUID</dt><dd>{item.uuid}</dd></div>
      <div><dt>媒体类型 / 模型</dt><dd>{item.mediaType} / {item.model}</dd></div>
      <div><dt>规范链接</dt><dd>{canonical}</dd></div>
      <div><dt>详情链接</dt><dd>{item.detailUrl}</dd></div>
      <div><dt>父级 UUID</dt><dd>{item.parentUuid ?? '无'}</dd></div>
    </dl>
    <div className="preview-identities">
      <strong>身份声明（逐条保留）</strong>
      {item.externalIds.length
        ? <ul>{item.externalIds.map((claim, index) => <li key={`${claim.scheme}:${claim.value}:${index}`}><code>{claim.scheme}</code> · <code>{claim.value}</code></li>)}</ul>
        : <p>无外部身份声明。</p>}
    </div>
  </div>;
}

function OutcomeEvidence({ outcome }: { outcome: Outcome }) {
  switch (outcome.kind) {
    case 'compared':
      return <>
        <p><strong>结果：</strong>已比较 · <strong>证据来源：</strong>{outcome.provenance === 'automatic' ? '自动' : '人工'} · <strong>快照时间：</strong>{timestamp(outcome.observedAt)}</p>
        <CatalogEvidence item={outcome.item} />
        <p>比较配置：<code>{outcome.comparison.profileId}</code> v{outcome.comparison.profileVersion}</p>
        <div className="preview-fields">
          {outcome.comparison.fields.map((field, index) => <article key={`${field.ruleId}:${field.field}:${index}`} className="preview-field">
            <h5>{field.field}</h5>
            <dl>
              <div><dt>规则</dt><dd>{field.ruleId}</dd></div>
              <div><dt>建议值</dt><dd>{scalar(field.suggestion)}</dd></div>
              <div><dt>快照值状态</dt><dd>{field.sink.kind === 'known' ? `Known · ${scalar(field.sink.value)}` : field.sink.kind === 'empty' ? 'Empty · 已确认为空' : 'Unknown · 无法确认'}</dd></div>
              <div><dt>决定</dt><dd>{field.status}</dd></div>
              <div><dt>原因</dt><dd>{field.reason}</dd></div>
            </dl>
          </article>)}
        </div>
      </>;
    case 'needs_review':
      return <><p><strong>结果：</strong>需要检查 · {outcome.reason}</p><CatalogEvidence item={outcome.item} /></>;
    case 'search_candidates':
      return <>
        <p><strong>结果：</strong>搜索候选 · {outcome.moreAvailable ? '服务端说明还有候选' : '服务端未说明还有候选'}</p>
        {outcome.candidates.length ? outcome.candidates.map((candidate, index) => <div className="preview-candidate" key={`${candidate.item.uuid}:${index}`}><h5>{candidate.title}</h5><CatalogEvidence item={candidate.item} /></div>) : <p>本页没有候选。</p>}
      </>;
    case 'unsupported': return <p><strong>结果：</strong>不支持</p>;
    case 'rate_limited': return <p><strong>结果：</strong>目录服务限流</p>;
    case 'unavailable': return <p><strong>结果：</strong>目录服务不可用</p>;
    default: {
      const exhaustive: never = outcome;
      return exhaustive;
    }
  }
}

function ResultCard({ result }: { result: Result }) {
  return <article className="preview-result">
    <div className="section-title"><h4>记录序号 {result.ordinal}</h4><span>尝试 {result.attempt}</span></div>
    <p className="preview-history">历史证据 · 不可执行 · 结果保存时间 {timestamp(result.committedAt)}</p>
    <dl>
      <div><dt>来源连接器</dt><dd>{result.sourceConnectorId}</dd></div>
      <div><dt>来源记录 ID</dt><dd>{result.sourceRecordId}</dd></div>
      <div><dt>来源指纹</dt><dd>{result.sourceFingerprint}</dd></div>
      <div><dt>配置</dt><dd>{result.profileId} v{result.profileVersion}</dd></div>
      <div><dt>NeoDB 绑定</dt><dd>{result.baseUrl} · {result.account}</dd></div>
    </dl>
    <OutcomeEvidence outcome={result.outcome} />
  </article>;
}

export function PreviewPanel({ client }: { client: AccessClient }) {
  const [profileId, setProfileId] = useState('');
  const [profileVersion, setProfileVersion] = useState('');
  const [selectedFile, setSelectedFile] = useState<File>();
  const [fileName, setFileName] = useState('');
  const [knownId, setKnownId] = useState('');
  const [selectedId, setSelectedId] = useState('');
  const [job, setJob] = useState<PreviewJobView>();
  const [page, setPage] = useState<PreviewResultPage>();
  const [offset, setOffset] = useState('0');
  const [limit, setLimit] = useState('25');
  const [inspectionVersion, setInspectionVersion] = useState(0);
  const [loading, setLoading] = useState(false);
  const [mutating, setMutating] = useState(false);
  const [authExpired, setAuthExpired] = useState(false);
  const [error, setError] = useState('');
  const [notice, setNotice] = useState('');
  const mounted = useRef(true);
  const mountEpoch = useRef(0);
  const actionControllers = useRef(new Set<AbortController>());
  const inspectionController = useRef<AbortController | undefined>(undefined);
  const actionBusy = useRef(false);
  const preserveMutationError = useRef(false);
  const selectionGeneration = useRef(0);
  const fileGeneration = useRef(0);
  const pageRequest = useRef<{ jobId: string; attempt: number; offset: number; limit: number } | undefined>(undefined);
  const suppressNextPageRefresh = useRef(false);
  const alive = (epoch: number) => mounted.current && mountEpoch.current === epoch;

  useEffect(() => {
    const epoch = ++mountEpoch.current;
    mounted.current = true;
    return () => {
      if (mountEpoch.current === epoch) mounted.current = false;
      for (const controller of actionControllers.current) controller.abort();
      actionControllers.current.clear();
    };
  }, []);

  useEffect(() => {
    if (!selectedId || mutating) return;
    const epoch = mountEpoch.current;
    const generation = selectionGeneration.current;
    const controller = new AbortController();
    inspectionController.current = controller;
    let active = true;
    let timer: ReturnType<typeof setTimeout> | undefined;
    const inspect = async () => {
      if (!active || controller.signal.aborted || !alive(epoch) || generation !== selectionGeneration.current) return;
      setLoading(true);
      try {
        const next = await client.previewJob(selectedId, controller.signal);
        if (!active || controller.signal.aborted || !alive(epoch) || generation !== selectionGeneration.current) return;
        if (next.id !== selectedId) throw new AccessClientError('invalid_response');
        let requestedPage = pageRequest.current;
        if (requestedPage && (requestedPage.jobId !== next.id || requestedPage.attempt !== next.attempt)) {
          requestedPage = undefined;
          pageRequest.current = undefined;
          setPage(undefined);
        }
        setJob(next);
        setKnownId(next.id);
        if (requestedPage && suppressNextPageRefresh.current) {
          suppressNextPageRefresh.current = false;
        } else if (requestedPage) {
          const nextPage = await client.previewResults(next.id, {
            attempt: requestedPage.attempt,
            offset: requestedPage.offset,
            limit: requestedPage.limit,
          }, controller.signal);
          if (!active || controller.signal.aborted || !alive(epoch) || generation !== selectionGeneration.current) return;
          if (nextPage.jobId !== requestedPage.jobId || nextPage.attempt !== requestedPage.attempt || nextPage.offset !== requestedPage.offset || nextPage.limit !== requestedPage.limit) {
            throw new AccessClientError('invalid_response');
          }
          setPage(nextPage);
        }
        if (next.status === 'queued' || next.status === 'running') timer = setTimeout(() => void inspect(), POLL_DELAY_MS);
      } catch (cause) {
        if (active && !controller.signal.aborted && alive(epoch) && generation === selectionGeneration.current) {
          if (cause instanceof AccessClientError && ['authentication_required', 'csrf_invalid'].includes(cause.code)) setAuthExpired(true);
          if (!preserveMutationError.current) setError(errorText(cause));
        }
      } finally {
        if (active && !controller.signal.aborted && alive(epoch) && generation === selectionGeneration.current) setLoading(false);
      }
    };
    void inspect();
    return () => {
      active = false;
      controller.abort();
      if (inspectionController.current === controller) inspectionController.current = undefined;
      if (timer) clearTimeout(timer);
    };
  }, [client, selectedId, inspectionVersion, mutating]);

  async function withAction(action: (signal: AbortSignal, epoch: number) => Promise<void>, uncertain = false) {
    if (actionBusy.current) return;
    actionBusy.current = true;
    // Invalidate observation synchronously, before React can run effect cleanup.
    inspectionController.current?.abort();
    setLoading(false);
    preserveMutationError.current = false;
    const controller = new AbortController();
    const epoch = mountEpoch.current;
    actionControllers.current.add(controller);
    setMutating(true); setError(''); setNotice('');
    try { await action(controller.signal, epoch); }
    catch (cause) {
      if (alive(epoch)) {
        if (cause instanceof AccessClientError && ['authentication_required', 'csrf_invalid'].includes(cause.code)) setAuthExpired(true);
        preserveMutationError.current = uncertain;
        setError(errorText(cause, uncertain));
      }
    }
    finally {
      actionControllers.current.delete(controller);
      actionBusy.current = false;
      if (alive(epoch)) setMutating(false);
    }
  }

  function inspect(id: string) {
    const normalized = id.trim();
    if (!LOCAL_ID.test(normalized) || normalized === '.' || normalized === '..') { setError('任务 ID 必须是 1–128 位 ASCII 字母、数字或 _ . : -，且不能是 . 或 ..。'); return; }
    selectionGeneration.current += 1;
    preserveMutationError.current = false;
    pageRequest.current = undefined;
    suppressNextPageRefresh.current = false;
    setJob(undefined); setPage(undefined); setNotice(''); setError('');
    setSelectedId(normalized);
    setInspectionVersion(value => value + 1);
  }

  function chooseFile(file: File | undefined) {
    fileGeneration.current += 1;
    setSelectedFile(undefined); setFileName(''); setError(''); setNotice('');
    if (!file) return;
    if (file.size > CSV_LIMIT) { setError('CSV 超过 10 MiB，未读取或提交。'); return; }
    setSelectedFile(file); setFileName(file.name);
    setNotice(`已选择 ${file.name}（${file.size} 字节），尚未读取或提交。`);
  }

  function submit(event: FormEvent) {
    event.preventDefault();
    const id = profileId.trim();
    const version = Number(profileVersion);
    if (!selectedFile) { setError('请先选择 UTF-8 CSV。'); return; }
    if (authExpired) { setError('本地授权已失效；重新授权并重新挂载页面前不会再提交变更。'); return; }
    if (!LOCAL_ID.test(id) || !Number.isInteger(version) || version < 1 || version > 4_294_967_295) { setError('请明确填写合法的保存配置 ID 与正整数版本；不会自动选择最新版本。'); return; }
    const file = selectedFile;
    const requestedFileGeneration = fileGeneration.current;
    void withAction(async (signal, epoch) => {
      let decoded: string;
      try {
        const bytes = await file.arrayBuffer();
        if (bytes.byteLength > CSV_LIMIT) {
          if (alive(epoch) && requestedFileGeneration === fileGeneration.current) setError('CSV 超过 10 MiB，未读取或提交。');
          return;
        }
        decoded = new TextDecoder('utf-8', { fatal: true, ignoreBOM: true }).decode(bytes);
      } catch {
        if (alive(epoch) && requestedFileGeneration === fileGeneration.current) setError('CSV 不是有效 UTF-8，未提交。');
        return;
      }
      if (!alive(epoch) || requestedFileGeneration !== fileGeneration.current) return;
      try {
        const accepted = await client.submitImdbRatings(decoded, { profileId: id, profileVersion: version }, signal);
        if (!alive(epoch) || requestedFileGeneration !== fileGeneration.current) return;
        selectionGeneration.current += 1;
        pageRequest.current = undefined;
        suppressNextPageRefresh.current = false;
        setKnownId(accepted.preview.jobId);
        setSelectedId(accepted.preview.jobId);
        setInspectionVersion(value => value + 1);
        setNotice(`已接收任务 ${accepted.preview.jobId}，尝试 ${accepted.preview.attempt}；导入 ${accepted.observationCount} 条，跳过行 ${accepted.skippedRowCount}、列 ${accepted.skippedColumnCount}，重复行 ${accepted.duplicateRowCount}。`);
      } catch (cause) {
        if (cause instanceof PreviewAdmissionClientError) {
          if (alive(epoch) && requestedFileGeneration === fileGeneration.current) {
            if (['authentication_required', 'csrf_invalid'].includes(cause.code)) setAuthExpired(true);
            selectionGeneration.current += 1;
            pageRequest.current = undefined;
            suppressNextPageRefresh.current = false;
            setKnownId(cause.jobId); setSelectedId(cause.jobId); setInspectionVersion(value => value + 1);
            setNotice(`提交结果不确定（${cause.code}），但服务端返回任务 ${cause.jobId}、尝试 ${cause.attempt}。正在读取状态，未自动重试。`);
          }
          return;
        }
        throw cause;
      }
    }, true);
  }

  function mutate(kind: 'cancel' | 'resume' | 'delete') {
    if (!job) return;
    if (authExpired) { setError('本地授权已失效；重新授权并重新挂载页面前不会再提交变更。'); return; }
    if (kind === 'delete' && !window.confirm('仅删除此任务的运行历史；已创建的公共目录条目不会被撤销。确认删除？')) return;
    void withAction(async (signal, epoch) => {
      const generation = selectionGeneration.current;
      if (kind === 'resume') {
        try {
          const accepted = await client.resumePreview(job.id, { expectedAttempt: job.attempt }, signal);
          if (accepted.jobId !== job.id || accepted.attempt !== job.attempt + 1) throw new AccessClientError('invalid_response');
          if (!alive(epoch) || generation !== selectionGeneration.current) return;
          setNotice(`已接收恢复请求：尝试 ${accepted.attempt}。状态仍以服务器读取结果为准。`);
        } catch (cause) {
          if (cause instanceof PreviewAdmissionClientError) {
            if (alive(epoch) && generation === selectionGeneration.current) {
              if (['authentication_required', 'csrf_invalid'].includes(cause.code)) setAuthExpired(true);
              setKnownId(cause.jobId); setSelectedId(cause.jobId);
              setNotice(`恢复结果不确定，但保留任务 ${cause.jobId}、尝试 ${cause.attempt} 供核对；未自动重试。`);
              setInspectionVersion(value => value + 1);
            }
            return;
          }
          throw cause;
        }
      } else if (kind === 'cancel') {
        const result = await client.cancelPreview(job.id, job.attempt, signal);
        if (!alive(epoch) || generation !== selectionGeneration.current) return;
        setNotice(result.applied ? '服务器确认取消已应用；部分历史证据仍保留。' : '服务器未应用取消；正在回读状态，未推断任务已取消。');
      } else {
        const result = await client.deletePreview(job.id, signal);
        if (!alive(epoch) || generation !== selectionGeneration.current) return;
        if (result.applied) {
          setNotice('服务器确认运行历史已删除；公共目录条目不受影响。');
          setSelectedId(''); setJob(undefined); setPage(undefined);
          return;
        }
        setNotice('服务器未应用删除；正在回读状态，未推断历史已删除。');
      }
      setInspectionVersion(value => value + 1);
    }, true);
  }

  function refresh() {
    preserveMutationError.current = false;
    setError('');
    setInspectionVersion(value => value + 1);
  }

  function loadPage(event: FormEvent) {
    event.preventDefault();
    if (!job) return;
    const requestedOffset = Number(offset);
    const requestedLimit = Number(limit);
    if (!Number.isInteger(requestedOffset) || requestedOffset < 0 || requestedOffset > 4_294_967_295 || !Number.isInteger(requestedLimit) || requestedLimit < 1 || requestedLimit > 100) {
      setError('页起始序号需为非负整数，单页数量需为 1–100。'); return;
    }
    void withAction(async (signal, epoch) => {
      const generation = selectionGeneration.current;
      const next = await client.previewResults(job.id, { attempt: job.attempt, offset: requestedOffset, limit: requestedLimit }, signal);
      if (next.jobId !== job.id || next.attempt !== job.attempt || next.offset !== requestedOffset || next.limit !== requestedLimit) throw new AccessClientError('invalid_response');
      if (alive(epoch) && generation === selectionGeneration.current) {
        pageRequest.current = { jobId: job.id, attempt: job.attempt, offset: requestedOffset, limit: requestedLimit };
        suppressNextPageRefresh.current = true;
        setPage(next);
        setNotice(`已读取尝试 ${next.attempt} 从序号 ${next.offset} 开始的一页；空页或稀疏页不表示任务完成。`);
      }
    });
  }

  let jobDetails: ReactNode = null;
  if (job) jobDetails = <div className="preview-job" aria-live="polite">
    <div className="section-title"><h4>任务 {job.id}</h4><span>{job.status}</span></div>
    <p className="preview-history">历史证据 · 不可执行 · 账户绑定：{job.bindingState}</p>
    <dl>
      <div><dt>尝试</dt><dd>{job.attempt}</dd></div>
      <div><dt>观测记录数</dt><dd>{job.observationCount}</dd></div>
      <div><dt>固定配置</dt><dd>{job.profileId} v{job.profileVersion}</dd></div>
      <div><dt>NeoDB 绑定</dt><dd>{job.baseUrl} · {job.account}</dd></div>
      <div><dt>创建 / 更新</dt><dd>{timestamp(job.createdAt)} / {timestamp(job.updatedAt)}</dd></div>
      <div><dt>历史保留至</dt><dd>{timestamp(job.expiresAt)}</dd></div>
    </dl>
    <div className="preview-actions">
      <button type="button" disabled={mutating || loading} onClick={refresh}>读取状态</button>
      <button type="button" disabled={mutating || authExpired || job.status === 'completed'} onClick={() => mutate('cancel')}>取消尝试 {job.attempt}</button>
      <button type="button" disabled={mutating || authExpired || !['interrupted', 'failed', 'cancelled'].includes(job.status)} onClick={() => mutate('resume')}>显式恢复尝试 {job.attempt}</button>
      <button type="button" className="secondary" disabled={mutating || authExpired} onClick={() => mutate('delete')}>删除运行历史</button>
    </div>
    <form className="preview-page-form" onSubmit={loadPage}>
      <label>起始记录序号<input type="number" min="0" max="4294967295" value={offset} onChange={event => setOffset(event.target.value)} /></label>
      <label>单页数量<input type="number" min="1" max="100" value={limit} onChange={event => setLimit(event.target.value)} /></label>
      <button disabled={mutating} type="submit">读取该尝试的一页结果</button>
    </form>
  </div>;

  return <section className="preview-panel" aria-labelledby="preview-title" aria-busy={loading || mutating}>
    <div className="section-title"><h3 id="preview-title">历史预览</h3><span>{loading ? '正在读取' : mutating ? '正在处理' : '仅供检查'}</span></div>
    <p className="preview-warning">预览可能让 NeoDB 创建公共目录条目，但不会写入 NeoDB 个人标记，也不会修改来源平台。所有结果都是历史证据、不可执行；当前界面不提供人工选择。</p>
    {authExpired && <p role="alert" className="access-error">本地授权已失效。下方已读取内容仍可作为历史证据查看，但所有变更已停用；请重新授权。</p>}
    {error && <p role="alert" className="access-error">{error}</p>}
    {notice && <p role="status" className="preview-notice">{notice}</p>}

    <form className="preview-import" onSubmit={submit}>
      <fieldset disabled={mutating || authExpired}>
        <legend>导入本地 IMDb 评分 CSV</legend>
        <div className="preview-form-grid">
          <label>CSV 文件（UTF-8，最大 10 MiB）<input type="file" accept=".csv,text/csv" onChange={event => chooseFile(event.target.files?.[0])} /></label>
          <label>保存的配置 ID<input required maxLength={128} value={profileId} onChange={event => setProfileId(event.target.value)} autoComplete="off" /></label>
          <label>保存的配置版本<input required type="number" min="1" max="4294967295" value={profileVersion} onChange={event => setProfileVersion(event.target.value)} /></label>
        </div>
        <p className="access-hint">{fileName ? `当前文件：${fileName}。` : '尚未选择文件。'} 必须明确填写版本；不会静默使用默认或最新配置。</p>
        <button type="submit">提交只读预览</button>
      </fieldset>
    </form>

    <form className="preview-open" onSubmit={event => { event.preventDefault(); inspect(knownId); }}>
      <label>已有任务 ID<input maxLength={128} value={knownId} onChange={event => setKnownId(event.target.value)} autoComplete="off" /></label>
      <button disabled={mutating || !knownId.trim()} type="submit">打开任务</button>
    </form>

    {jobDetails}
    {page && <section className="preview-page" aria-label="历史结果页">
      <p className="preview-history">任务 {page.jobId} · 尝试 {page.attempt} · 起始序号 {page.offset} · 请求上限 {page.limit} · 历史、不可执行</p>
      {page.items.length ? page.items.map(result => <ResultCard key={`${result.attempt}:${result.ordinal}`} result={result} />) : <p className="empty">本页为空；这不表示任务完成，也不表示后续没有稀疏结果。</p>}
    </section>}
  </section>;
}
