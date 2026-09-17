import { cleanup, fireEvent, render, screen } from '@testing-library/react';
import { afterEach, expect, it, vi } from 'vitest';
import { createAccessClient, type MappingProfile } from '@neodb-collector/core-client';
import { MappingPanel } from './MappingPanel.js';

afterEach(cleanup);
const fixture: MappingProfile = {
  schemaVersion: 1, id: 'imdb_ratings', version: 1, enabled: false,
  sourceConnectorId: 'imdb:ratings_csv', sinkConnectorId: 'neodb',
  rules: [
    { id: 'rating', enabled: true, target: 'rating_grade', operation: { kind: 'clamp', from: 'rating', min: 0, max: 10 } },
    { id: 'rating_implies_complete', enabled: false, target: 'shelf_type', operation: { kind: 'condition', from: 'rating', test: { kind: 'exists' }, value: 'complete' } },
  ],
};
it('opts into completion and saves disabled versions without trusting imported version numbers', async () => {
  const saved: { expectedVersion: number; document: string }[] = [];
  const client = createAccessClient({ baseUrl: 'http://localhost', fetch: async input => {
    const request = new Request(input);
    if (request.method === 'GET') return Response.json(fixture);
    const body = await request.json() as { expectedVersion: number; document: string };
    if (request.method === 'PUT') saved.push(body);
    return Response.json(JSON.parse(body.document));
  } });
  render(<MappingPanel client={client} />);
  fireEvent.click(screen.getByText('使用 IMDb 默认规则'));
  const complete = await screen.findByLabelText('评分即看完（需主动开启）');
  expect((complete as HTMLInputElement).checked).toBe(false);
  fireEvent.click(complete);
  fireEvent.click(screen.getByText('保存禁用版本'));
  await screen.findByText('已保存 v1（禁用）；未执行任何媒体同步。');
  expect(saved[0]?.expectedVersion).toBe(0);
  expect(JSON.parse(saved[0]!.document)).toMatchObject({ enabled: false, version: 1, rules: [{ enabled: true }, { enabled: true }] });
  fireEvent.change(screen.getByLabelText('文本格式'), { target: { value: 'json' } });
  fireEvent.change(screen.getByLabelText('配置文本'), { target: { value: JSON.stringify({ ...fixture, version: 99 }) } });
  fireEvent.click(screen.getByText('验证并导入文本'));
  await screen.findByText('已验证并导入到表单，尚未保存；保存版本由已读取的版本决定。');
  fireEvent.click(screen.getByText('保存禁用版本'));
  await screen.findByText('已保存 v2（禁用）；未执行任何媒体同步。');
  expect(saved[1]?.expectedVersion).toBe(1);
  expect(JSON.parse(saved[1]!.document).version).toBe(2);
});

it('keeps edits on conflict and shows field errors without replacing form on invalid import', async () => {
  const client = createAccessClient({ baseUrl: 'http://localhost', fetch: async input => {
    const request = new Request(input);
    if (request.method === 'GET') return Response.json(fixture);
    return Response.json({ code: request.method === 'PUT' ? 'mapping_version_conflict' : 'mapping_arguments_invalid', message: 'private upstream details', field: 'rules.0', retryable: false }, { status: request.method === 'PUT' ? 409 : 400 });
  } });
  render(<MappingPanel client={client} />);
  fireEvent.click(screen.getByText('读取并替换表单'));
  fireEvent.click(await screen.findByLabelText('评分即看完（需主动开启）'));
  fireEvent.click(screen.getByText('保存禁用版本'));
  expect((await screen.findByRole('alert')).textContent).toContain('版本冲突');
  expect((screen.getByLabelText('评分即看完（需主动开启）') as HTMLInputElement).checked).toBe(true);
  fireEvent.change(screen.getByLabelText('配置文本'), { target: { value: 'invalid config' } });
  fireEvent.click(screen.getByText('验证并导入文本'));
  await vi.waitFor(() => expect(screen.getByRole('alert').textContent).toContain('rules.0'));
  expect((screen.getByLabelText('配置文本') as HTMLTextAreaElement).value).toBe('invalid config');
  expect((screen.getByLabelText('评分即看完（需主动开启）') as HTMLInputElement).checked).toBe(true);
  expect(screen.queryByText('private upstream details')).toBeNull();
});

it('exports through Core and does not save merely by converting a document', async () => {
  const seen: Request[] = [];
  const client = createAccessClient({ baseUrl: 'http://localhost', fetch: async input => {
    const request = new Request(input); seen.push(request);
    return Response.json(request.method === 'GET' ? fixture : { format: 'yaml', document: 'schemaVersion: 1\nid: imdb_ratings\n' });
  } });
  render(<MappingPanel client={client} />);
  fireEvent.click(screen.getByText('使用 IMDb 默认规则'));
  fireEvent.click(await screen.findByText('导出表单为 YAML'));
  await screen.findByText('表单已导出到下方 YAML 文本，可复制或下载。');
  expect((screen.getByLabelText('配置文本') as HTMLTextAreaElement).value).toContain('schemaVersion: 1');
  expect(seen.some(request => request.method === 'PUT')).toBe(false);
  expect((await seen[1]!.json()).targetFormat).toBe('yaml');
});
