import { useEffect, useState, type ReactNode } from 'react';
import { CoreClientError, createCoreClient, type Snapshot } from '@neodb-collector/core-client';
import './style.css';

type State = { kind: 'loading' } | { kind: 'connected' | 'degraded'; snapshot: Snapshot } | { kind: 'unavailable' | 'incompatible' | 'invalid_response' };
const client = createCoreClient();
const labels = {
  loading: ['正在连接本地服务', '读取服务状态与可用能力…'],
  connected: ['本地服务已连接', '基础服务运行正常。此版本尚未接入平台，不会读取或写入任何平台账户。'],
  degraded: ['本地数据库不可用', '服务仍可访问，但数据库检查失败。请在终端运行 collector doctor 查看诊断。'],
  unavailable: ['无法连接本地服务', '请确认 collector serve 正在运行，然后重新检查。'],
  incompatible: ['客户端与服务版本不兼容', '请使用同一版本的 WebUI 与 Core，再重新启动服务。'],
  invalid_response: ['服务响应格式异常', '请确认当前端口运行的是兼容的 Collector Core，再重新检查。'],
} as const;

export function App({ loadStatus = client.status, access }: { loadStatus?: (signal?: AbortSignal) => Promise<Snapshot>; access?: ReactNode }) {
  const [state, setState] = useState<State>({ kind: 'loading' });
  const [revision, setRevision] = useState(0);
  useEffect(() => {
    const controller = new AbortController();
    let active = true;
    loadStatus(controller.signal).then(snapshot => {
      if (active) setState({ kind: snapshot.health.status === 'ok' ? 'connected' : 'degraded', snapshot });
    }).catch((error: unknown) => {
      if (active) setState({ kind: error instanceof CoreClientError ? error.kind : 'unavailable' });
    });
    return () => { active = false; controller.abort(); };
  }, [revision, loadStatus]);
  const snapshot = 'snapshot' in state ? state.snapshot : undefined;
  const [title, description] = labels[state.kind];
  return <main>
    <header className="masthead"><a href="/" className="wordmark">NeoDB <strong>Collector</strong></a><span className="local-label">本地控制台</span></header>
    <section className="intro"><p className="eyebrow">服务状态</p><h1>记录流动之前，<br />先确认连接。</h1><p>检查本地服务、数据库与当前可用的平台连接。</p></section>
    <section className={`status-panel ${state.kind}`} aria-labelledby="connection-title">
      <div className="status-copy" role="status" aria-live="polite"><span className="signal" aria-hidden="true" /><div><h2 id="connection-title">{title}</h2><p>{description}</p></div></div>
      <button disabled={state.kind === 'loading'} onClick={() => { setState({ kind: 'loading' }); setRevision(r => r + 1); }}>{state.kind === 'loading' ? '正在检查…' : '重新检查'}</button>
    </section>
    <div className="details">
      <section aria-labelledby="runtime-title"><h2 id="runtime-title">运行环境</h2><dl>
        <div><dt>Core 版本</dt><dd>{snapshot?.health.coreVersion ?? '—'}</dd></div>
        <div><dt>API / 数据契约</dt><dd>{snapshot ? `${snapshot.health.apiVersion} / ${snapshot.health.schemaVersion}` : '—'}</dd></div>
        <div><dt>本地数据库</dt><dd>{snapshot ? (snapshot.health.storage === 'ready' ? '可用' : '不可用') : '尚未确认'}</dd></div>
      </dl></section>
      <section aria-labelledby="platform-title"><div className="section-title"><h2 id="platform-title">平台连接</h2>{snapshot && <span>{snapshot.capabilities.connectorIds.length} 个</span>}</div>
        {snapshot ? (snapshot.capabilities.connectorIds.length ? <ul>{snapshot.capabilities.connectorIds.map(id => <li key={id}>{id}</li>)}</ul> : <div className="empty"><p>尚无可用的平台连接</p><span>当前是基础服务版本；同步与导入功能将在后续版本接入。</span></div>) : <p className="empty">连接服务后显示可用平台。</p>}
      </section>
    </div>
    {(state.kind === 'connected' || state.kind === 'degraded') && access}
    <footer><span>管理本地访问 · 不执行同步</span><code>collector doctor --format json</code></footer>
  </main>;
}
