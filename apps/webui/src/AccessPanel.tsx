import { useEffect, useState } from 'react';
import { NeoDbPanel } from './NeoDbPanel.js';
import { AccessClientError, type AccessClient, type PairedClient, type SessionView, type PairingLink } from '@neodb-collector/core-client';

function message(error: unknown) {
  const code = error instanceof AccessClientError ? error.code : 'unavailable';
  if (['authentication_required', 'challenge_invalid', 'csrf_invalid'].includes(code)) return '授权已过期或链接已使用。请重新启动 Core，打开终端中新的一次性链接。';
  if (code === 'auth_rate_limited') return '操作过于频繁，请等待一分钟后重试。';
  return '操作未确认成功。请检查服务连接后刷新状态，不要重复兑换旧链接。';
}
export function AccessPanel({ client, initialSession }: { client: AccessClient; initialSession: Promise<SessionView> }) {
  const [session, setSession] = useState<SessionView>();
  const [clients, setClients] = useState<PairedClient[]>([]);
  const [loading, setLoading] = useState(true);
  const [busy, setBusy] = useState(false);
  const [error, setError] = useState('');
  const [name, setName] = useState('Collector CLI');
  const [kind, setKind] = useState('cli');
  const [link, setLink] = useState<PairingLink>();
  useEffect(() => {
    let active = true;
    initialSession.then(async view => {
      if (!active) return;
      const entries = await client.clients();
      if (active) { setSession(view); setClients(entries); }
    }).catch((cause: unknown) => { if (active) setError(message(cause)); })
      .finally(() => { if (active) setLoading(false); });
    return () => { active = false; };
  }, [initialSession, client]);
  useEffect(() => {
    if (!link) return;
    const timer = setTimeout(() => setLink(undefined), Math.max(0, link.expiresAt * 1000 - Date.now()));
    return () => clearTimeout(timer);
  }, [link]);
  async function act(action: () => Promise<void>) {
    setBusy(true); setError('');
    try { await action(); } catch (cause) {
      setError(message(cause));
      if (cause instanceof AccessClientError && ['authentication_required', 'csrf_invalid'].includes(cause.code)) { setSession(undefined); setLink(undefined); setClients([]); }
    } finally { setBusy(false); }
  }
  return <section className="access-panel" aria-labelledby="access-title">
    <div className="section-title"><h2 id="access-title">本地访问授权</h2><span>{loading ? '正在检查' : session ? '已授权' : '未授权'}</span></div>
    {error && <p role="alert" className="access-error">{error}</p>}
    {!loading && !session && !error && <p>已退出。重新授权需打开 Core 启动时给出的一次性链接。</p>}
    {session && <>
      <NeoDbPanel client={client} />
      <p className="access-hint">以下配对授权用于连接本地客户端，独立于 NeoDB 账户授权。</p>
      <form onSubmit={event => { event.preventDefault(); setLink(undefined); void act(async () => setLink(await client.createPairing({ clientName: name, clientKind: kind }))); }}>
        <label>客户端名称<input value={name} maxLength={80} required onChange={event => setName(event.target.value)} /></label>
        <label>客户端类型<select value={kind} onChange={event => setKind(event.target.value)}><option value="cli">命令行</option><option value="browser_extension">浏览器扩展（预留）</option><option value="local_plugin">本地插件（预留）</option></select></label>
        <button disabled={busy || !name.trim()} type="submit">生成配对链接</button>
      </form>
      {link && <div className="pairing-link"><label>一次性链接 · 10 分钟内有效<textarea readOnly value={link.uri} rows={3} onFocus={event => event.target.select()} /></label><p>仅交给刚才指定的客户端。不要公开分享；成功兑换后不可再次使用。</p><button disabled={busy} onClick={() => setLink(undefined)}>隐藏链接</button></div>}
      <div className="section-title client-heading"><h3>已配对客户端</h3><button disabled={busy} onClick={() => void act(async () => { setClients(await client.clients()); })}>刷新列表</button></div>
      {clients.length ? <ul className="client-list">{clients.map(entry => <li key={entry.id}><span>{entry.name}<small>{entry.kind} · {entry.revoked ? '已撤销' : '已配对'}</small></span>{!entry.revoked && <button disabled={busy} onClick={() => void act(async () => { await client.revoke(entry.id); setClients(await client.clients()); })}>撤销 {entry.name}</button>}</li>)}</ul> : <p className="access-hint">尚无已配对客户端。生成链接并完成兑换后，在此刷新查看。</p>}
      <button className="secondary" disabled={busy} onClick={() => void act(async () => { await client.logout(); setSession(undefined); setLink(undefined); setClients([]); })}>退出本地授权</button>
    </>}
  </section>;
}
