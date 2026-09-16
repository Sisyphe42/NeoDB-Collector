import { useEffect, useState } from 'react';
import { type AccessClient, type NeoDbStatus } from '@neodb-collector/core-client';
import { oauthReturnFailed } from './session.js';

export function NeoDbPanel({ client, navigate = (url: string) => window.location.assign(url) }: { client: AccessClient; navigate?: (url: string) => void }) {
  const [status, setStatus] = useState<NeoDbStatus>();
  const [base, setBase] = useState('https://neodb.social');
  const [busy, setBusy] = useState(false);
  const [error, setError] = useState(oauthReturnFailed ? '本次授权未完整完成。请核对当前连接状态，再重试或取消。' : '');
  useEffect(() => {
    let active = true;
    client.neodbStatus().then(view => { if (active) setStatus(view); })
      .catch(() => { if (active) setError('无法读取 NeoDB 状态，请检查本地服务和系统凭据库后刷新。'); });
    return () => { active = false; };
  }, [client]);
  async function act(action: () => Promise<void>) {
    setBusy(true); setError('');
    try { await action(); }
    catch { setError('操作未确认完成。请刷新连接状态后重试；原连接可能仍然有效。'); }
    finally { setBusy(false); }
  }
  return <div className="neodb-panel">
    <div className="section-title"><h3>NeoDB 账户</h3><span>{status ? status.connected ? '已连接' : '未连接' : '正在读取'}</span></div>
    <p className="access-hint">授权仅申请读取权限，用于后续预览对比。更换账户将在成功授权后替换当前连接。</p>
    {status?.connected && <p>{status.account} · {status.baseUrl}</p>}
    {status?.pending && <p>授权正在等待完成。可返回 NeoDB 授权页继续，或取消后重新开始。</p>}
    {error && <p role="alert" className="access-error">{error}</p>}
    <form onSubmit={event => { event.preventDefault(); void act(async () => {
      const view = await client.startNeoDb(base.trim());
      const url = new URL(view.authorizationUrl);
      const requested = new URL(base.trim());
      if (url.origin !== requested.origin || !['https:', 'http:'].includes(url.protocol) || url.pathname !== '/oauth/authorize') throw new Error('Invalid authorization destination');
      navigate(url.href);
    }); }}>
      <label>NeoDB 实例地址<input type="url" required value={base} onChange={event => setBase(event.target.value)} /></label>
      <button type="submit" disabled={busy || !status || status.pending}>{status?.connected ? '更换 NeoDB 账户' : '授权 NeoDB'}</button>
    </form>
    <button disabled={busy} onClick={() => void act(async () => setStatus(await client.neodbStatus()))}>刷新连接状态</button>
    {status?.pending && <button disabled={busy} onClick={() => void act(async () => { await client.cancelNeoDb(); setStatus(await client.neodbStatus()); })}>取消 NeoDB 授权</button>}
  </div>;
}
