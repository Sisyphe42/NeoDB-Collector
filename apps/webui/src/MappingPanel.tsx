import { useRef, useState } from 'react';
import { AccessClientError, type AccessClient, type MappingProfile as Profile, type ProfileFormat as Format } from '@neodb-collector/core-client';

function failure(cause: unknown) {
  if (!(cause instanceof AccessClientError)) return '操作未确认成功；表单已保留。请检查连接后重新读取已保存版本。';
  if (cause.code === 'mapping_version_conflict') return '版本冲突：其他客户端已保存。当前草稿已保留，可先导出，再重新读取并核对修改。';
  if (cause.code === 'mapping_profile_not_found') return '尚未保存这个配置。可从 IMDb 默认规则开始，或导入配置文本。';
  if (['authentication_required', 'csrf_invalid'].includes(cause.code)) return '本地授权已失效。草稿暂留在此页面，可先复制文本，再重新授权。';
  if (cause.code === 'connector_not_activated') return '当前只能编辑禁用配置；请将 enabled 设为 false。保存不会启用同步。';
  if (cause.code === 'mapping_document_too_large') return '配置超过 64 KiB，请缩小后重试。';
  if (cause.code.startsWith('mapping_')) return `配置未通过验证${cause.field ? `（${cause.field}）` : ''}。请检查字段、规则和数值范围；草稿已保留。`;
  return '操作未确认成功；表单已保留。请检查连接后重新读取已保存版本。';
}

export function MappingPanel({ client }: { client: AccessClient }) {
  const [profile, setProfile] = useState<Profile>();
  const [saved, setSaved] = useState<{ id: string; version: number }>();
  const [lookup, setLookup] = useState('imdb_ratings');
  const [dirty, setDirty] = useState(false);
  const [format, setFormat] = useState<Format>('yaml');
  const [text, setText] = useState('');
  const [busy, setBusy] = useState(false);
  const locked = useRef(false);
  const [error, setError] = useState('');
  const [notice, setNotice] = useState('');
  const expected = profile && saved && saved.id === profile.id ? saved.version : 0;
  const proposed = profile ? { ...profile, version: expected + 1 } : undefined;

  async function act(action: () => Promise<void>) {
    if (locked.current) return;
    locked.current = true; setBusy(true); setError(''); setNotice('');
    try { await action(); } catch (cause) { setError(failure(cause)); }
    finally { locked.current = false; setBusy(false); }
  }
  function edit(next: Profile) { setProfile(next); setDirty(true); setNotice(''); setError(''); }
  function document() {
    if (!proposed) throw new Error('No profile');
    return { format: 'json' as const, document: JSON.stringify(proposed) };
  }

  return <section className="mapping-panel" aria-labelledby="mapping-title" aria-busy={busy}>
    <div className="section-title"><h3 id="mapping-title">字段映射</h3><span>同步未启用</span></div>
    <p className="access-hint">决定个人记录如何对应 NeoDB 字段。默认只映射评分，不把评分日期当作看完日期；保存配置不会导入条目或修改标记。</p>
    {error && <p role="alert" className="access-error">{error}</p>}
    {notice && <p role="status" className="mapping-notice">{notice}</p>}
    {busy && <p role="status" className="access-hint">正在处理配置…</p>}
    <fieldset disabled={busy} className="mapping-controls">
      <legend className="sr-only">映射配置</legend>
      <div className="mapping-toolbar">
        <label>读取配置 ID<input value={lookup} maxLength={128} onChange={event => setLookup(event.target.value)} /></label>
        <button disabled={!lookup.trim()} onClick={() => void act(async () => {
          const next = await client.mappingProfile(lookup.trim());
          setProfile(next); setSaved({ id: next.id, version: next.version }); setDirty(false);
          setNotice(`已读取 v${next.version}；替换了上方表单，配置文本仍保留。`);
        })}>读取并替换表单</button>
        <button className="secondary" onClick={() => void act(async () => {
          const next = await client.defaultMapping();
          setProfile(next); setDirty(true);
          setNotice('已载入默认规则。若此 ID 已有配置，请先读取当前版本，再编辑保存。');
        })}>使用 IMDb 默认规则</button>
      </div>
      {profile && <>
        <div className="mapping-meta">
          <label>配置 ID<input value={profile.id} maxLength={128} onChange={event => edit({ ...profile, id: event.target.value })} /></label>
          <label>来源连接器<input value={profile.sourceConnectorId} maxLength={128} onChange={event => edit({ ...profile, sourceConnectorId: event.target.value })} /></label>
          <p>目标 <strong>{profile.sinkConnectorId}</strong><br />{expected ? `已读取 v${expected}` : '新配置'} · 下次保存 v{expected + 1}{dirty ? ' · 未保存' : ''}</p>
        </div>
        <div className="mapping-rules">{profile.rules.map((rule, index) => <div className="mapping-rule" key={index}>
          <label className="mapping-toggle"><input type="checkbox" checked={rule.enabled} onChange={event => edit({ ...profile, rules: profile.rules.map((r, i) => i === index ? { ...r, enabled: event.target.checked } : r) })} />{rule.id === 'rating_implies_complete' ? '评分即看完（需主动开启）' : `启用 ${rule.id}`}</label>
          <div className="mapping-flow"><code>{'from' in rule.operation ? rule.operation.from : '—'}</code><span aria-hidden="true">→</span><label>规则 {index + 1} 目标<select value={rule.target} onChange={event => edit({ ...profile, rules: profile.rules.map((r, i) => i === index ? { ...r, target: event.target.value } : r) })}><option value="rating_grade">个人评分 · rating_grade</option><option value="shelf_type">收藏状态 · shelf_type</option></select></label></div>
          <code className="mapping-operation">{JSON.stringify(rule.operation)}</code>
        </div>)}</div>
        <div className="mapping-actions">
          <button className="secondary" onClick={() => void act(async () => { await client.validateMapping(document()); setNotice('规则验证通过，尚未保存或启用同步。'); })}>验证表单</button>
          <button disabled={!dirty} onClick={() => void act(async () => {
            const next = await client.saveMapping(profile.id, { ...document(), expectedVersion: expected });
            setProfile(next); setSaved({ id: next.id, version: next.version }); setLookup(next.id); setDirty(false);
            setNotice(`已保存 v${next.version}（禁用）；未执行任何媒体同步。`);
          })}>保存禁用版本</button>
          <button className="secondary" onClick={() => void act(async () => {
            const output = await client.convertMapping({ input: { format: 'json', document: JSON.stringify(dirty ? proposed : profile) }, targetFormat: 'yaml' });
            setText(output.document); setFormat('yaml'); setNotice('表单已导出到下方 YAML 文本，可复制或下载。');
          })}>导出表单为 YAML</button>
        </div>
      </>}
      <details className="mapping-document">
        <summary>配置文本 · 完整规则编辑 / 导入导出</summary>
        <p className="access-hint">可编辑 direct、numeric_scale、clamp、enum_map、condition、skip。保存只作用于上方表单；文本修改请先验证并导入。</p>
        <div className="mapping-toolbar">
          <label>文本格式<select value={format} onChange={event => setFormat(event.target.value === 'json' ? 'json' : 'yaml')}><option value="yaml">YAML</option><option value="json">JSON</option></select></label>
          <label>从文件读取<input type="file" accept=".yaml,.yml,.json" onChange={event => {
            const file = event.target.files?.[0]; event.target.value = '';
            if (file) void act(async () => {
              if (file.size > 65_536) throw new AccessClientError('mapping_document_too_large');
              const content = new TextDecoder('utf-8', { fatal: true }).decode(await file.arrayBuffer());
              setText(content); setFormat(file.name.endsWith('.json') ? 'json' : 'yaml'); setNotice('文件已读入文本区；验证并导入后才会替换表单。');
            });
          }} /></label>
        </div>
        <label htmlFor="mapping-document-text">配置文本</label>
        <textarea id="mapping-document-text" value={text} rows={14} spellCheck={false} onChange={event => setText(event.target.value)} />
        <div className="mapping-actions">
          <button disabled={!text.trim()} onClick={() => void act(async () => {
            const next = await client.validateMapping({ format, document: text });
            setProfile(next); setDirty(true); setNotice('已验证并导入到表单，尚未保存；保存版本由已读取的版本决定。');
          })}>验证并导入文本</button>
          <button disabled={!text.trim()} className="secondary" onClick={() => {
            const url = URL.createObjectURL(new Blob([text], { type: 'text/plain;charset=utf-8' }));
            const link = window.document.createElement('a'); link.href = url; link.download = `mapping.${format === 'json' ? 'json' : 'yaml'}`;
            link.click(); setTimeout(() => URL.revokeObjectURL(url), 0);
          }}>下载文本</button>
        </div>
      </details>
    </fieldset>
  </section>;
}
