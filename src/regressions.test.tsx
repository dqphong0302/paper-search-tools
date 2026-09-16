// @vitest-environment jsdom
import { act, type ComponentProps, type ReactNode } from 'react';
import { createRoot, type Root } from 'react-dom/client';
import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest';
import { App } from './App';
import { Explorer } from './components/Explorer';
import { SearchHistory } from './components/SearchHistory';
import { IntegrationsPage } from './components/IntegrationsPage';
import { AgentAccess } from './components/AgentAccess';
import { Library } from './components/Library';
import { invoke, isTauri } from '@tauri-apps/api/core';
import type { ResearchWorkspace } from './components/ResearchWorkspace';
import { gatewayFetch, initGateway } from './lib/gateway';
import type { Paper, SearchResponse, WorkspacePaper } from './types';

vi.mock('./lib/gateway', () => ({ DEFAULT_GATEWAY_PORT: 8795, initGateway: vi.fn(), gatewayFetch: vi.fn(), gatewayUrl: (path: string) => `http://localhost:8795${path}` }));
vi.mock('@tauri-apps/api/core', () => ({ invoke: vi.fn(), isTauri: vi.fn() }));
vi.mock('./components/SearchPage', () => ({ SearchPage: () => null }));
vi.mock('./components/AgentGateway', () => ({ AgentGateway: () => null }));
vi.mock('./components/ClinicalSuite', () => ({ ClinicalSuite: () => null }));
vi.mock('./components/SettingsPage', () => ({ SettingsPage: () => null }));
vi.mock('./components/ResearchGapPanel', () => ({ ResearchGapPanel: () => null }));
vi.mock('./components/SearchScanner', () => ({ SearchScanner: () => null }));
vi.mock('./components/layout/SideNav', () => ({
  SideNav: ({ setActiveTab }: { setActiveTab: (tab: string) => void }) =>
    <button id="research" onClick={() => setActiveTab('research')}>Research</button>,
}));
vi.mock('./components/ResearchWorkspace', () => ({
  ResearchWorkspace: ({ workspacePapers, onUpdatePaper }: ComponentProps<typeof ResearchWorkspace>) =>
    <div id="workspace-papers">
      {workspacePapers.map(wp => <div key={wp.paper.id}>{wp.paper.title}: {wp.note}</div>)}
      <button id="update-note" onClick={() => onUpdatePaper('shared', { note: 'edited-A' })}>Update</button>
    </div>,
}));

const response = (data: unknown) => new Response(JSON.stringify(data), { status: 200 });
function deferred<T>() {
  let resolve!: (value: T) => void;
  let reject!: (reason: Error) => void;
  const promise = new Promise<T>((yes, no) => { resolve = yes; reject = no; });
  return { promise, resolve, reject };
}
const paper = (id: string): Paper => ({ id, title: `Paper-${id}`, source: 'Crossref', authors: [], open_access: false });
const page = (query: string, ids: string[], available = 40): SearchResponse => ({
  query, papers: ids.map(paper), total: ids.length, available_total: available, cache_hit: false, elapsed_ms: 1,
});
const workspacePaper = (workspace: string): WorkspacePaper => ({
  paper: { ...paper('shared'), title: `Workspace-${workspace}-paper` }, note: `${workspace}-note`, added_at: 1,
});
const fetchMock = vi.mocked(gatewayFetch);
let host: HTMLDivElement;
let root: Root;
async function render(node: ReactNode) { await act(async () => { root.render(node); }); }
async function click(selector: string) {
  const element = host.querySelector<HTMLButtonElement>(selector);
  expect(element).not.toBeNull();
  await act(async () => { element!.click(); });
}
async function switchWorkspace(value: string) {
  await act(async () => {
    const select = host.querySelector<HTMLSelectElement>('#workspace-switcher')!;
    select.value = value;
    select.dispatchEvent(new Event('change', { bubbles: true }));
  });
}
function appRequests(papers: (id: string, init?: RequestInit) => Promise<Response>) {
  fetchMock.mockImplementation(async (url, init) => {
    if (url === '/api/workspaces') return response(['A', 'B'].map(id => ({ id, name: id, paper_count: 1 })));
    if (url === '/api/telemetry') return response({ recent_logs: [] });
    if (url === '/api/history/downloads') return response([]);
    const match = url.match(/^\/api\/workspaces\/([^/]+)\/papers/);
    if (match) return papers(match[1], init);
    throw new Error(`Unexpected request ${url}`);
  });
}
beforeEach(() => {
  Object.assign(globalThis, { IS_REACT_ACT_ENVIRONMENT: true });
  vi.resetAllMocks();
  localStorage.clear();
  vi.mocked(initGateway).mockResolvedValue(8795);
  host = document.createElement('div');
  document.body.append(host);
  root = createRoot(host);
});
afterEach(async () => {
  await act(async () => { root.unmount(); });
  host.remove();
  vi.restoreAllMocks();
});

describe('workspace request isolation', () => {
  it('waits for gateway initialization before loading a remembered workspace', async () => {
    localStorage.setItem('sg_active_workspace', 'A');
    const ready = deferred<number>();
    vi.mocked(initGateway).mockReturnValue(ready.promise);
    appRequests(async id => response([workspacePaper(id)]));
    await render(<App />);
    expect(fetchMock).not.toHaveBeenCalled();
    await act(async () => { ready.resolve(9876); });
    await click('#research');
    expect(host.textContent).toContain('Workspace-A-paper');
  });

  it('ignores a late response from the previous workspace', async () => {
    const old = deferred<Response>();
    appRequests(async id => id === 'A' ? old.promise : response([workspacePaper(id)]));
    await render(<App />);
    await click('#research');
    await switchWorkspace('B');
    expect(host.textContent).toContain('Workspace-B-paper');
    await act(async () => { old.resolve(response([workspacePaper('A')])); });
    expect(host.textContent).toContain('Workspace-B-paper');
    expect(host.textContent).not.toContain('Workspace-A-paper');
  });

  it('hides old papers immediately and does not apply an old note update to the new workspace', async () => {
    const next = deferred<Response>();
    const patch = deferred<Response>();
    appRequests(async (id, init) => init?.method === 'PATCH' ? patch.promise
      : id === 'B' ? next.promise : response([workspacePaper(id)]));
    await render(<App />);
    await click('#research');
    await click('#update-note');
    await switchWorkspace('B');
    expect(host.textContent).not.toContain('Workspace-A-paper');
    await act(async () => { next.resolve(response([workspacePaper('B')])); });
    await act(async () => { patch.resolve(response({ success: true })); });
    expect(host.textContent).toContain('B-note');
    expect(host.textContent).not.toContain('edited-A');
  });
});

describe('search pagination', () => {
  const explorer = (query: string) => <Explorer initialQuery={query} searchNonce={1}
    port={8795} onSavePaper={() => {}} savedPaperIds={new Set()} workspaceId="A" />;

  it.each(['success', 'failure'])('ignores an old load-more %s after a new query', async outcome => {
    const old = deferred<Response>();
    fetchMock.mockImplementation(async (_url, init) => {
      const body = JSON.parse(String(init?.body));
      return body.offset ? old.promise : response(page(body.query, [`${body.query}-first`]));
    });
    await render(explorer('A'));
    await click('#search-load-more');
    await render(explorer('B'));
    await act(async () => {
      if (outcome === 'success') old.resolve(response(page('A', ['A-stale'])));
      else old.reject(new Error('stale-load-error'));
    });
    expect(host.textContent).toContain('Paper-B-first');
    expect(host.textContent).not.toContain('Paper-A-stale');
    expect(host.textContent).not.toContain('stale-load-error');
  });

  it('uses the submitted filters, prevents double fetches and advances past duplicate rows', async () => {
    const more = deferred<Response>();
    fetchMock.mockResolvedValueOnce(response(page('A', ['first']))).mockReturnValueOnce(more.promise)
      .mockResolvedValueOnce(response(page('A', [], 2)));
    await render(explorer('A'));
    await act(async () => {
      const input = host.querySelector<HTMLInputElement>('#search-year-min')!;
      Object.getOwnPropertyDescriptor(HTMLInputElement.prototype, 'value')!.set!.call(input, '2024');
      input.dispatchEvent(new Event('input', { bubbles: true }));
    });
    await click('#search-load-more');
    await click('#search-load-more');
    expect(fetchMock).toHaveBeenCalledTimes(2);
    const body = JSON.parse(String(fetchMock.mock.calls[1][1]?.body));
    expect(body).toMatchObject({ query: 'A', offset: 1, workspace_id: 'A' });
    expect(body.year_min).toBeUndefined();
    await act(async () => { more.resolve(response(page('A', ['first']))); });
    await click('#search-load-more');
    expect(JSON.parse(String(fetchMock.mock.calls[2][1]?.body)).offset).toBe(2);
    expect(host.querySelector('#search-load-more')).toBeNull();
  });

  it('continues beyond 100 merged papers until the candidate pool is exhausted', async () => {
    fetchMock.mockImplementation(async (_url, init) => {
      const { offset = 0 } = JSON.parse(String(init?.body));
      const count = Math.min(offset === 0 ? 50 : 15, 115 - offset);
      return response(page('A', Array.from({ length: count }, (_, i) => String(offset + i)), 115));
    });
    await render(explorer('A'));
    for (let i = 0; i < 5; i++) await click('#search-load-more');
    expect(host.textContent).toContain('Paper-114');
    expect(host.querySelector('#search-load-more')).toBeNull();
    expect(fetchMock).toHaveBeenCalledTimes(6);
  });
});

it('opens paper details on demand and restores keyboard focus on Escape', async () => {
  fetchMock.mockResolvedValue(response({ ...page('A', ['first']), papers: [{ ...paper('first'), abstract: 'A unique abstract.' }] }));
  const save = vi.fn();
  await render(<Explorer initialQuery="A" port={8795} onSavePaper={save} savedPaperIds={new Set()} />);
  expect(host.querySelector('.paper-detail-panel')).toBeNull();
  const saveButton = host.querySelector<HTMLButtonElement>('button[title="Save to workspace"]')!;
  await act(async () => saveButton.click());
  expect(save).toHaveBeenCalledTimes(1);
  await click('.paper-title-button');
  const panel = host.querySelector<HTMLElement>('.paper-detail-panel')!;
  expect(panel.textContent).toContain('A unique abstract.');
  expect(document.activeElement).toBe(panel);
  await act(async () => panel.dispatchEvent(new KeyboardEvent('keydown', { key: 'Escape', bubbles: true })));
  expect(host.querySelector('.paper-detail-panel')).toBeNull();
  expect(document.activeElement).toBe(host.querySelector('.paper-title-button'));
});

it('previews a bundled skill and installs only after confirmation', async () => {
  vi.mocked(isTauri).mockReturnValue(true);
  localStorage.setItem('sg_skills_root', '/test/skills');
  const preview = { name: 'paper-search', source: 'builtin:paper-search', path: '', content: 'Skill content', files: 1, bytes: 20, enabled: true };
  vi.mocked(invoke).mockImplementation(async command => command === 'list_skills' ? [preview] : preview);
  const confirm = vi.spyOn(window, 'confirm').mockReturnValue(false);
  await render(<IntegrationsPage port={8795} />);
  expect(host.querySelector<HTMLInputElement>('#skills-source')).toBeNull();
  expect(host.querySelector<HTMLButtonElement>('#skills-install')!.disabled).toBe(true);
  await click('#skills-preview');
  expect(invoke).toHaveBeenCalledWith('preview_skill', { source: 'builtin:paper-search' });
  await click('#skills-install');
  expect(invoke).not.toHaveBeenCalledWith('install_skill', expect.anything());
  confirm.mockReturnValue(true);
  await click('#skills-install');
  expect(invoke).toHaveBeenCalledWith('install_skill', { source: 'builtin:paper-search', targetRoot: '/test/skills' });
  await act(async () => {
    const select = host.querySelector<HTMLSelectElement>('#skills-bundle')!;
    select.value = 'builtin:research-resume';
    select.dispatchEvent(new Event('change', { bubbles: true }));
  });
  expect(host.querySelector<HTMLButtonElement>('#skills-install')!.disabled).toBe(true);
});

it('creates scoped read-only access, tests with the agent token and confirms revocation', async () => {
  const grant = { id: 'agent-one', name: 'Reader', workspace_ids: ['A'], writable: false, revoked: false };
  let created = false;
  fetchMock.mockImplementation(async (path, init) => {
    if (path === '/api/workspaces') return response([{ id: 'A', name: 'Project A' }]);
    if (path === '/api/agents' && init?.method === 'POST') {
      expect(JSON.parse(String(init.body))).toEqual({ name: 'Reader', workspace_ids: ['A'], writable: false });
      created = true;
      return response({ agent: grant, token: 'sg_agent_fixture' });
    }
    if (path === '/api/agents') return response(created ? [grant] : []);
    if (path === '/api/agents/agent-one' && init?.method === 'DELETE') { grant.revoked = true; return response({ success: true }); }
    throw new Error(`Unexpected request: ${path}`);
  });
  const probe = vi.spyOn(globalThis, 'fetch').mockResolvedValue(response({ jsonrpc:'2.0', id:1, result:{isError:false} }));
  const confirm = vi.spyOn(window, 'confirm').mockReturnValue(false);
  await render(<AgentAccess />);
  await act(async () => {
    const input = host.querySelector<HTMLInputElement>('#agent-name')!;
    Object.getOwnPropertyDescriptor(HTMLInputElement.prototype, 'value')!.set!.call(input, 'Reader');
    input.dispatchEvent(new Event('input', { bubbles: true }));
  });
  await click('#agent-workspace-A');
  await click('#agent-create');
  expect(host.querySelector<HTMLInputElement>('#agent-new-token')!.value).toBe('sg_agent_fixture');
  expect(localStorage.getItem('sg_agent_fixture')).toBeNull();
  await click('#agent-test');
  expect(probe).toHaveBeenCalledWith('http://localhost:8795/mcp', expect.objectContaining({
    headers: expect.objectContaining({ Authorization: 'Bearer sg_agent_fixture' }),
  }));
  expect(host.textContent).toContain('Project access verified');
  await click('#agent-revoke-agent-one');
  expect(grant.revoked).toBe(false);
  confirm.mockReturnValue(true);
  await click('#agent-revoke-agent-one');
  expect(grant.revoked).toBe(true);
  expect(host.querySelector('#agent-new-token')).toBeNull();
});

it('keeps saved paper editing available inside the on-demand detail panel', async () => {
  const update = vi.fn();
  const saved = { ...workspacePaper('A'), paper: { ...paper('saved'), abstract: 'Saved abstract' } };
  await render(<Library workspacePapers={[saved]} onRemovePaper={vi.fn()} onUpdatePaper={update} />);
  expect(host.querySelector('.paper-detail-panel')).toBeNull();
  expect(host.querySelector('[aria-label="Paper tags"]')).toBeNull();
  await click('.paper-title-button');
  expect(host.querySelector('.paper-detail-panel')!.textContent).toContain('Saved abstract');
  await click('button[title="Mark as favorite"]');
  expect(update).toHaveBeenCalledWith('saved', { favorite: true });
  const editNote = [...host.querySelectorAll('button')].find(button => button.textContent?.includes('Edit Note'))!;
  await act(async () => editNote.click());
  expect(host.querySelector<HTMLTextAreaElement>('[aria-label="Paper note"]')!.value).toBe('A-note');
  await click('button[aria-label="Close saved paper details"]');
  expect(host.querySelector('.paper-detail-panel')).toBeNull();
  expect(document.activeElement).toBe(host.querySelector('.paper-title-button'));
});

it('keeps search options collapsed and applies edited filters explicitly', async () => {
  fetchMock.mockResolvedValue(response(page('A', ['first'])));
  await render(<Explorer initialQuery="A" port={8795} onSavePaper={() => {}} savedPaperIds={new Set()} />);
  expect(host.querySelector<HTMLDetailsElement>('#search-options')!.open).toBe(false);
  await click('#search-options > summary');
  expect(host.querySelector<HTMLDetailsElement>('#search-options')!.open).toBe(true);
  await act(async () => {
    const input = host.querySelector<HTMLInputElement>('#search-year-min')!;
    Object.getOwnPropertyDescriptor(HTMLInputElement.prototype, 'value')!.set!.call(input, '2023');
    input.dispatchEvent(new Event('input', { bubbles: true }));
  });
  expect(fetchMock).toHaveBeenCalledTimes(1);
  await click('#search-apply-options');
  expect(JSON.parse(String(fetchMock.mock.calls[1][1]?.body))).toMatchObject({ query: 'A', year_min: 2023 });
});

it('lets users choose a discipline before their first search without changing keywords', async () => {
  fetchMock.mockImplementation(async () => response(page('6g network', ['first'])));
  const submit = vi.fn();
  const view = (initialQuery = '', searchNonce = 0) => <Explorer initialQuery={initialQuery}
    draftQuery="6g network" searchNonce={searchNonce} onSubmitQuery={submit} hideSearchBar
    port={8795} onSavePaper={() => {}} savedPaperIds={new Set()} />;
  await render(view());
  await click('#search-options > summary');
  await click('#discipline-engineering');
  expect(host.querySelector('#discipline-engineering')!.getAttribute('aria-pressed')).toBe('true');
  expect(fetchMock).not.toHaveBeenCalled();
  await click('#search-apply-options');
  expect(submit).toHaveBeenCalledExactlyOnceWith('6g network');
  await render(view('6g network', 1));
  expect(fetchMock).toHaveBeenCalledTimes(1);
  expect(host.querySelector<HTMLDetailsElement>('#search-options')!.open).toBe(false);
  expect(JSON.parse(String(fetchMock.mock.calls[0][1]?.body))).toMatchObject({ query: '6g network', sources: ['engineering'] });
  await click('#discipline-cs_ai');
  expect(fetchMock).toHaveBeenCalledTimes(1);
  await render(view('6g network', 2));
  expect(JSON.parse(String(fetchMock.mock.calls[1][1]?.body))).toMatchObject({ query: '6g network', sources: ['cs_ai'] });
  await click('#search-reset-options');
  expect(fetchMock).toHaveBeenCalledTimes(2);
  expect(host.querySelector<HTMLSelectElement>('#search-scope')!.value).toBe('default');
  await render(view());
  expect(host.textContent).not.toContain('Paper-first');
});

it('clears history only for the displayed workspace', async () => {
  vi.spyOn(window, 'confirm').mockReturnValue(true);
  fetchMock.mockResolvedValueOnce(response([{ id: 's1', query: 'fixture', result_count: 1, elapsed_ms: 1, created_at: 1 }]))
    .mockResolvedValueOnce(response({ success: true }));
  await render(<SearchHistory port={8795} workspaceId="A" onRerunSearch={() => {}} />);
  const clear = [...host.querySelectorAll('button')].find(button => button.textContent?.includes('Clear All'))!;
  await act(async () => { clear.click(); });
  expect(fetchMock).toHaveBeenLastCalledWith('/api/history/searches?workspace_id=A', { method: 'DELETE' });
  expect(window.confirm).toHaveBeenCalledWith(expect.stringContaining('in this workspace'));
});

// Sources still waiting for an API key were counted in the "N/M sources
// unresponsive" warning, so an ordinary search looked like the app was broken.
it('separates sources that need setup from sources that actually failed', async () => {
  fetchMock.mockImplementation(async () => response({
    ...page('quantum', ['ok1']),
    sources: [
      { id: 'openalex', name: 'OpenAlex', queried: true, ok: true, count: 12 },
      { id: 'arxiv', name: 'arXiv', queried: true, ok: false, count: 0, error: 'HTTP 429' },
      { id: 'scopus', name: 'Scopus', queried: true, ok: false, count: 0, error: 'scopus: requires SCOPUS_API_KEY', needs_setup: true },
      { id: 'ieee', name: 'IEEE Xplore', queried: true, ok: false, count: 0, error: 'ieee: requires IEEE_API_KEY', needs_setup: true },
      { id: 'sljol', name: 'SLJOL', queried: true, ok: false, count: 0, error: 'paused after 2 consecutive failures, retrying in 240s — last error: HTTP 403 Forbidden', cooling_down: true },
      { id: 'pubmed', name: 'PubMed', queried: false, ok: true, count: 0 },
    ],
  }));
  await render(<Explorer initialQuery="quantum" searchNonce={1} port={8795}
    onSavePaper={() => {}} savedPaperIds={new Set()} hideSearchBar />);

  // Only arXiv counts as unresponsive, and only against the two reachable sources.
  expect(host.textContent).toContain('1/2 sources unresponsive');
  expect(host.textContent).toContain('arXiv: HTTP 429');
  expect(host.textContent).toContain('2 sources need setup');
  expect(host.textContent).toContain('Scopus');
  expect(host.textContent).toContain('IEEE Xplore');
  expect(host.textContent).toContain('ACTIVE SOURCES (1/2)');
  // A source in cooldown is reported on its own, never as a live failure.
  expect(host.textContent).toContain('1 source is paused after repeated failures');
  expect(host.textContent).toContain('HTTP 403 Forbidden');
  // A source awaiting credentials must not wear the red failure chip.
  expect(host.textContent).not.toContain('4/5 sources unresponsive');
});
