import React, { useCallback, useEffect, useMemo, useRef, useState } from 'react';
import {
  AlertTriangle, Award, BarChart3, Bot, Check, Copy, DownloadCloud, ExternalLink, FileText, FolderOpen, FolderPlus,
  Layers, ListChecks, Loader2, Pencil, Square, Trash2,
} from 'lucide-react';
import { Workspace, WorkspacePaper } from '../types';
import {
  agentHandoffPrompt, collectionPapers, createCollection, deleteCollection, downloadIndex, exportCollection,
  ExportResult, fulltextIndex, GROUP_LABELS, GroupBy, groupPapers, INTEREST_LIBRARY_ID, listCollections,
  removeFromCollection, renameCollection, runPool, summarizeCollection, summaryMarkdown, updateCollectionPaper,
} from '../lib/collections';
import { RankingsBanner } from './RankingsBanner';
import { Paper } from '../types';
import { bibtexLibrary, risLibrary } from '../lib/citation';
import { requestPdfDownload } from '../lib/pdfDownload';
import { convertDownloadToMarkdown } from '../lib/pdfText';
import { Library } from './Library';
import { QuartileBadge } from './QuartileBadge';

const QUARTILE_COLORS: Record<string, string> = {
  Q1: 'var(--status-emerald)',
  Q2: 'var(--primary-cyan)',
  Q3: 'var(--status-amber)',
  Q4: 'var(--status-rose)',
  Unranked: 'var(--text-dim)',
};

const Stat: React.FC<{ label: string; value: React.ReactNode }> = ({ label, value }) => (
  <div className="collection-stat">
    <div className="collection-stat-value">{value}</div>
    <div className="collection-stat-label">{label}</div>
  </div>
);

/**
 * Collections: gather papers into a named set, collect their PDFs, turn them
 * into Markdown (with OCR), see them grouped by quartile/topic/year, and hand
 * the whole set to an AI agent over MCP or as a folder.
 */
export const Collections: React.FC = () => {
  const [collections, setCollections] = useState<Workspace[]>([]);
  const [activeId, setActiveId] = useState<string>(() => {
    try { return localStorage.getItem('sg_active_collection') || ''; } catch { return ''; }
  });
  const [items, setItems] = useState<WorkspacePaper[]>([]);
  const [pdfByPaper, setPdfByPaper] = useState<Map<string, string>>(new Map());
  const [markdownIds, setMarkdownIds] = useState<Set<string>>(new Set());
  const [groupBy, setGroupBy] = useState<GroupBy>('quartile');
  const [newName, setNewName] = useState('');
  const [renaming, setRenaming] = useState(false);
  const [renameDraft, setRenameDraft] = useState('');
  const [task, setTask] = useState('');
  const [busy, setBusy] = useState<'pdf' | 'markdown' | 'export' | null>(null);
  const [progress, setProgress] = useState('');
  const [notice, setNotice] = useState('');
  const [error, setError] = useState('');
  const [exported, setExported] = useState<ExportResult | null>(null);
  const [copied, setCopied] = useState<'prompt' | 'path' | 'synthesis' | null>(null);
  const [view, setView] = useState<'overview' | 'papers' | 'agent'>('overview');
  const [job, setJob] = useState<{ done: number; total: number } | null>(null);
  const [failures, setFailures] = useState<{ paper: Paper; reason: string }[]>([]);
  const stopRef = useRef(false);

  const active = collections.find((collection) => collection.id === activeId) ?? null;

  const loadCollections = useCallback(async () => {
    try {
      const list = (await listCollections()).filter((item) => item.id !== INTEREST_LIBRARY_ID);
      setCollections(list);
      setActiveId((current) => (list.some((item) => item.id === current) ? current : list[0]?.id ?? ''));
    } catch (cause) {
      setError(`Could not load collections: ${(cause as Error).message}`);
    }
  }, []);

  const loadStatus = useCallback(async () => {
    const [downloads, fulltexts] = await Promise.all([
      downloadIndex().catch(() => []),
      fulltextIndex().catch(() => []),
    ]);
    setPdfByPaper(new Map(downloads.map((record) => [record.paper_id, record.id])));
    setMarkdownIds(new Set(fulltexts.map((info) => info.paper_id)));
  }, []);

  const loadPapers = useCallback(async (id: string) => {
    if (!id) { setItems([]); return; }
    try {
      setItems(await collectionPapers(id));
    } catch (cause) {
      setError(`Could not load the collection: ${(cause as Error).message}`);
    }
  }, []);

  useEffect(() => { void loadCollections(); void loadStatus(); }, [loadCollections, loadStatus]);
  useEffect(() => {
    void loadPapers(activeId);
    setExported(null);
    try { localStorage.setItem('sg_active_collection', activeId); } catch { /* per-viewer convenience only */ }
  }, [activeId, loadPapers]);

  const groups = useMemo(() => groupPapers(items, groupBy), [items, groupBy]);
  const summary = useMemo(
    () => summarizeCollection(items, new Set(pdfByPaper.keys()), markdownIds),
    [items, pdfByPaper, markdownIds]
  );

  const flash = (message: string) => { setNotice(message); window.setTimeout(() => setNotice(''), 6000); };

  const create = async () => {
    const name = newName.trim();
    if (!name) return;
    try {
      const collection = await createCollection(name);
      setNewName('');
      await loadCollections();
      setActiveId(collection.id);
    } catch (cause) { setError((cause as Error).message); }
  };

  const rename = async () => {
    if (!active || !renameDraft.trim()) return;
    try {
      await renameCollection(active.id, renameDraft.trim(), active.description);
      setRenaming(false);
      await loadCollections();
    } catch (cause) { setError((cause as Error).message); }
  };

  const remove = async () => {
    if (!active || !window.confirm(`Delete the collection “${active.name}”? Papers stay in your library and other collections.`)) return;
    try {
      await deleteCollection(active.id);
      await loadCollections();
    } catch (cause) { setError((cause as Error).message); }
  };

  // Every paper with a DOI is worth a try: the gateway looks up open-access copies itself.
  const missingPdf = items.filter((item) => !pdfByPaper.has(item.paper.id) && (item.paper.pdf_url || item.paper.doi));
  const missingMarkdown = items.filter((item) => pdfByPaper.has(item.paper.id) && !markdownIds.has(item.paper.id));

  const articleUrl = (paper: Paper) => paper.source_url || (paper.doi ? `https://doi.org/${paper.doi}` : undefined);

  const startJob = (kind: 'pdf' | 'markdown', total: number) => {
    stopRef.current = false;
    setBusy(kind); setError(''); setFailures([]); setJob({ done: 0, total });
  };
  const finishJob = async (message: string) => {
    await loadStatus();
    setBusy(null); setProgress(''); setJob(null);
    flash(stopRef.current ? `Stopped. ${message}` : message);
  };
  const tick = () => setJob((current) => (current ? { ...current, done: current.done + 1 } : current));

  // Downloads are network-bound, so three run at once.
  const collectPdfs = async () => {
    if (!active) return;
    const queue = missingPdf;
    startJob('pdf', queue.length);
    let done = 0;
    await runPool(queue, 3, async (item) => {
      setProgress(item.paper.title.slice(0, 80));
      const result = await requestPdfDownload(item.paper, active.id);
      if (result.ok) done += 1;
      else setFailures((list) => [...list, { paper: item.paper, reason: result.error }]);
      tick();
    }, () => stopRef.current);
    await finishJob(`Collected ${done} of ${queue.length} PDFs.`);
  };

  // OCR is CPU-bound and shares one Tesseract worker, so papers go one at a time.
  const convertAll = async () => {
    const queue = missingMarkdown;
    startJob('markdown', queue.length);
    let done = 0;
    await runPool(queue, 1, async (item) => {
      const downloadId = pdfByPaper.get(item.paper.id);
      if (!downloadId) return;
      try {
        await convertDownloadToMarkdown(downloadId, item.paper, {
          ocr: 'auto',
          shouldStop: () => stopRef.current,
          onProgress: ({ page, total, stage }) =>
            setProgress(`${item.paper.title.slice(0, 60)} · ${stage === 'ocr' ? 'OCR' : 'reading'} page ${page}/${total}`),
        });
        done += 1;
      } catch (cause) {
        if ((cause as Error).name !== 'AbortError') {
          setFailures((list) => [...list, { paper: item.paper, reason: (cause as Error).message }]);
        }
      }
      tick();
    }, () => stopRef.current);
    await finishJob(`Converted ${done} of ${queue.length} PDFs to Markdown.`);
  };

  const synthesis = active ? summaryMarkdown(active.name, summary, groups, groupBy) : '';

  const exportBundle = async () => {
    if (!active) return;
    setBusy('export'); setError('');
    try {
      const papers = items.map((item) => item.paper);
      setExported(await exportCollection(active.id, {
        'synthesis.md': synthesis,
        'references.ris': risLibrary(papers),
        'references.bib': bibtexLibrary(papers),
      }));
    } catch (cause) { setError((cause as Error).message); }
    finally { setBusy(null); }
  };

  const copy = (text: string, what: 'prompt' | 'path' | 'synthesis') => {
    void navigator.clipboard.writeText(text).then(() => { setCopied(what); window.setTimeout(() => setCopied(null), 1800); });
  };

  const prompt = active ? agentHandoffPrompt(active, task, exported?.path) : '';

  return (
    <div style={{ display: 'flex', flexDirection: 'column', gap: 14 }}>
      {/* ---- Collection picker ---- */}
      <div className="cockpit-card" style={{ padding: 14, display: 'flex', flexWrap: 'wrap', gap: 10, alignItems: 'center' }}>
        <Layers size={16} style={{ color: 'var(--primary-cyan)' }} />
        {collections.length > 0 && !renaming && (
          <select
            id="collection-select"
            className="field-input"
            aria-label="Collection"
            value={activeId}
            onChange={(event) => setActiveId(event.target.value)}
            style={{ width: 'auto', minWidth: 220, maxWidth: 420, fontWeight: 600 }}
          >
            {collections.map((collection) => (
              <option key={collection.id} value={collection.id}>{collection.name} ({collection.paper_count})</option>
            ))}
          </select>
        )}
        {renaming && active && (
          <form onSubmit={(event) => { event.preventDefault(); void rename(); }} style={{ display: 'flex', gap: 6 }}>
            <input className="field-input" aria-label="Collection name" value={renameDraft} maxLength={120} onChange={(event) => setRenameDraft(event.target.value)} autoFocus />
            <button type="submit" className="action-btn action-btn-primary"><Check size={13} /> Save</button>
            <button type="button" className="action-btn" onClick={() => setRenaming(false)}>Cancel</button>
          </form>
        )}
        {active && !renaming && (
          <>
            <button type="button" className="action-btn" onClick={() => { setRenameDraft(active.name); setRenaming(true); }} aria-label="Rename collection"><Pencil size={13} /></button>
            <button type="button" className="action-btn" onClick={() => void remove()} aria-label="Delete collection" style={{ color: 'var(--status-rose)' }}><Trash2 size={13} /></button>
          </>
        )}
        <form onSubmit={(event) => { event.preventDefault(); void create(); }} style={{ display: 'flex', gap: 6, marginLeft: 'auto' }}>
          <input
            id="new-collection-name"
            className="field-input"
            placeholder="New collection, e.g. “CRISPR review 2025”"
            aria-label="New collection name"
            value={newName}
            maxLength={120}
            onChange={(event) => setNewName(event.target.value)}
            style={{ width: 260, maxWidth: '100%' }}
          />
          <button id="create-collection" type="submit" className="action-btn action-btn-primary" disabled={!newName.trim()}><FolderPlus size={13} /> Create</button>
        </form>
      </div>

      {error && <div className="alert alert-warning" role="alert">{error}</div>}
      {notice && <div className="alert alert-info" role="status">{notice}</div>}

      {!active && (
        <div className="cockpit-card" style={{ padding: 28, textAlign: 'center', color: 'var(--text-muted)' }}>
          <Layers size={28} style={{ color: 'var(--primary-cyan)' }} />
          <h3 style={{ margin: '10px 0 6px', color: 'var(--text-main)' }}>No collections yet</h3>
          <p style={{ margin: 0 }}>Create one above, then tick papers in <b>Search</b> or <b>Papers of interest</b> and choose <b>Add to collection</b>.</p>
        </div>
      )}

      {active && (
        <>
          {/* ---- Pipeline actions ---- */}
          <div className="collection-pipeline">
            <button id="collect-pdfs" type="button" className="action-btn" disabled={busy !== null || !missingPdf.length} onClick={() => void collectPdfs()} title="Download every missing PDF, trying other open-access copies when a link is blocked">
              {busy === 'pdf' ? <Loader2 size={14} className="animate-spin" /> : <DownloadCloud size={14} />}
              <span>1 · Collect PDFs{missingPdf.length ? ` (${missingPdf.length})` : ' ✓'}</span>
            </button>
            <button id="convert-markdown" type="button" className="action-btn" disabled={busy !== null || !missingMarkdown.length} onClick={() => void convertAll()} title="Extract each PDF's text to Markdown; scanned pages are OCR'd (English + Vietnamese)">
              {busy === 'markdown' ? <Loader2 size={14} className="animate-spin" /> : <FileText size={14} />}
              <span>2 · Convert to Markdown{missingMarkdown.length ? ` (${missingMarkdown.length})` : ' ✓'}</span>
            </button>
            <button id="export-collection" type="button" className="action-btn" disabled={busy !== null || !items.length} onClick={() => void exportBundle()} title="Write index.md, synthesis.md, references and pdf/ + markdown/ to a folder">
              {busy === 'export' ? <Loader2 size={14} className="animate-spin" /> : <FolderOpen size={14} />}
              <span>3 · Export folder for AI</span>
            </button>
            <button type="button" className="action-btn action-btn-primary" onClick={() => setView('agent')}>
              <Bot size={14} /><span>4 · Hand off to agent</span>
            </button>
          </div>
          {job && (
            <div className="collection-job" role="status" aria-live="polite">
              <div className="collection-job-head">
                <span>
                  <b>{busy === 'pdf' ? 'Collecting PDFs' : 'Converting to Markdown'}</b> · {job.done}/{job.total}
                  {stopRef.current ? ' · stopping after the current item…' : ''}
                </span>
                <button id="stop-job" type="button" className="action-btn" onClick={() => { stopRef.current = true; setJob((j) => (j ? { ...j } : j)); }} disabled={stopRef.current}>
                  <Square size={12} /> Stop
                </button>
              </div>
              <div className="collection-job-bar"><span style={{ width: `${job.total ? (job.done / job.total) * 100 : 0}%` }} /></div>
              {progress && <div className="collection-job-detail">{progress}</div>}
            </div>
          )}
          {failures.length > 0 && !job && (
            <details className="alert alert-warning collection-failures" open={failures.length <= 5}>
              <summary><AlertTriangle size={14} /> {failures.length} paper{failures.length === 1 ? '' : 's'} could not be processed</summary>
              <ul>
                {failures.map(({ paper, reason }) => (
                  <li key={paper.id}>
                    <b>{paper.title}</b>
                    <span>{reason}</span>
                    {articleUrl(paper) && (
                      <a href={articleUrl(paper)} target="_blank" rel="noreferrer"><ExternalLink size={12} /> Article page</a>
                    )}
                  </li>
                ))}
              </ul>
            </details>
          )}
          {exported && (
            <div className="alert alert-info" role="status" style={{ display: 'flex', gap: 10, alignItems: 'center', flexWrap: 'wrap' }}>
              <span>Exported {exported.papers} papers ({exported.pdfs} PDFs, {exported.markdown} Markdown) to <code>{exported.path}</code></span>
              <button type="button" className="action-btn" onClick={() => copy(exported.path, 'path')}>{copied === 'path' ? <Check size={13} /> : <Copy size={13} />} Copy path</button>
            </div>
          )}

          <div className="segmented" role="tablist" aria-label="Collection views" style={{ alignSelf: 'flex-start' }}>
            {([
              ['overview', 'Overview', <BarChart3 key="o" size={14} />],
              ['papers', 'Papers', <ListChecks key="p" size={14} />],
              ['agent', 'AI agent', <Bot key="a" size={14} />],
            ] as const).map(([id, label, icon]) => (
              <button key={id} id={`collection-view-${id}`} type="button" role="tab" aria-selected={view === id}
                className={`segmented-item ${view === id ? 'active' : ''}`} onClick={() => setView(id)}>
                {icon}<span>{label}</span>{id === 'papers' && <span className="segmented-count">{items.length}</span>}
              </button>
            ))}
          </div>

          {/* ---- Synthesis ---- */}
          {view === 'overview' && <RankingsBanner onLoaded={() => void loadPapers(activeId)} />}
          {view === 'overview' && <section className="cockpit-card" style={{ padding: 16, display: 'flex', flexDirection: 'column', gap: 14 }} aria-labelledby="synthesis-heading">
            <div style={{ display: 'flex', alignItems: 'center', gap: 10, flexWrap: 'wrap' }}>
              <h3 id="synthesis-heading" style={{ margin: 0, fontSize: 15 }}>Synthesis</h3>
              <label style={{ display: 'flex', alignItems: 'center', gap: 6, fontSize: 12, marginLeft: 'auto', whiteSpace: 'nowrap' }}>
                Group by
                <select id="collection-group-by" className="field-input" style={{ width: 'auto' }} value={groupBy} onChange={(event) => setGroupBy(event.target.value as GroupBy)}>
                  {(Object.keys(GROUP_LABELS) as GroupBy[]).map((key) => <option key={key} value={key}>{GROUP_LABELS[key]}</option>)}
                </select>
              </label>
              <button type="button" className="action-btn" onClick={() => copy(synthesis, 'synthesis')}>{copied === 'synthesis' ? <Check size={13} /> : <Copy size={13} />} Copy as Markdown</button>
            </div>

            <div className="collection-stats">
              <Stat label="Papers" value={summary.total} />
              <Stat label="Years" value={summary.yearRange ? `${summary.yearRange[0]}–${summary.yearRange[1]}` : '—'} />
              <Stat label="Open access" value={`${summary.openAccess}/${summary.total}`} />
              <Stat label="PDFs collected" value={`${summary.withPdf}/${summary.total}`} />
              <Stat label="Markdown ready" value={`${summary.withMarkdown}/${summary.total}`} />
            </div>

            <div>
              <div className="collection-subheading"><Award size={13} /> Journal quartile (SCImago SJR)</div>
              <div className="quartile-bar" role="img" aria-label={summary.quartiles.map((q) => `${q.label}: ${q.count}`).join(', ')}>
                {summary.quartiles.filter((q) => q.count).map((q) => (
                  <span key={q.label} style={{ flexGrow: q.count, background: QUARTILE_COLORS[q.label] }} title={`${q.label}: ${q.count}`}>
                    {q.label} · {q.count}
                  </span>
                ))}
              </div>
              {summary.quartiles[4].count === summary.total && summary.total > 0 && (
                <div style={{ fontSize: 12, color: 'var(--text-muted)', marginTop: 6 }}>
                  No quartiles yet — load the SCImago ranking in <b>Settings → Gateway &amp; Security → Journal Rankings</b>.
                </div>
              )}
            </div>

            <div className="collection-facets">
              {[
                { title: 'Topics', rows: summary.topics },
                { title: 'Journals', rows: summary.venues },
                { title: 'Frequent terms', rows: summary.terms.slice(0, 10) },
              ].map((facet) => (
                <div key={facet.title}>
                  <div className="collection-subheading">{facet.title}</div>
                  {facet.rows.length ? (
                    <div style={{ display: 'flex', flexWrap: 'wrap', gap: 6 }}>
                      {facet.rows.map((row) => <span key={row.label} className="collection-chip">{row.label} <b>{row.count}</b></span>)}
                    </div>
                  ) : <div style={{ fontSize: 12, color: 'var(--text-dim)' }}>—</div>}
                </div>
              ))}
            </div>

            {groupBy !== 'none' && (
              <div style={{ display: 'flex', flexDirection: 'column', gap: 6 }}>
                {groups.map((group) => (
                  <details key={group.key} className="collection-group" open={groups.length <= 4}>
                    <summary>
                      <span>{group.label}</span>
                      <span className="segmented-count">{group.items.length}</span>
                    </summary>
                    <ul>
                      {group.items.map(({ paper }) => (
                        <li key={paper.id}>
                          <span className="collection-paper-title">{paper.title}</span>
                          <span className="collection-paper-meta">
                            {[paper.year, paper.venue].filter(Boolean).join(' · ')}
                            <QuartileBadge quartile={paper.quartile} style={{ marginLeft: 6 }} />
                            {pdfByPaper.has(paper.id) && <span className="badge badge-emerald" style={{ marginLeft: 6 }}>PDF</span>}
                            {markdownIds.has(paper.id) && <span className="badge badge-cyan" style={{ marginLeft: 6 }}>MD</span>}
                          </span>
                        </li>
                      ))}
                    </ul>
                  </details>
                ))}
              </div>
            )}
          </section>}

          {/* ---- Agent hand-off ---- */}
          {view === 'agent' && <section id="collection-handoff" className="cockpit-card" style={{ padding: 16, display: 'flex', flexDirection: 'column', gap: 10 }} aria-labelledby="handoff-heading">
            <h3 id="handoff-heading" style={{ margin: 0, fontSize: 15 }}><Bot size={15} style={{ verticalAlign: -2 }} /> Hand off to an AI agent</h3>
            <p style={{ margin: 0, fontSize: 12, color: 'var(--text-muted)' }}>
              Agents connected to ScholarGate over MCP (Claude, Codex, Cursor… — set up in <b>Settings → AI Clients</b>) read this collection with
              {' '}<code>get_collection</code> and each paper&apos;s Markdown with <code>get_paper_fulltext</code>. Describe the task, then paste the prompt into the agent.
            </p>
            <textarea
              id="handoff-task"
              className="field-input"
              rows={3}
              placeholder="e.g. Write a narrative review of the efficacy and safety evidence, grouped by topic, and list research gaps."
              value={task}
              onChange={(event) => setTask(event.target.value)}
            />
            <pre className="collection-prompt">{prompt}</pre>
            <div>
              <button id="copy-handoff-prompt" type="button" className="action-btn action-btn-primary" onClick={() => copy(prompt, 'prompt')}>
                {copied === 'prompt' ? <Check size={13} /> : <Copy size={13} />} {copied === 'prompt' ? 'Copied' : 'Copy prompt'}
              </button>
            </div>
          </section>}

          {/* ---- Papers ---- */}
          {view === 'papers' && <Library
            workspacePapers={items}
            workspaceName={active.name}
            workspaceId={active.id}
            onRemovePaper={(paperId) => {
              void removeFromCollection(active.id, paperId).then(() => loadPapers(active.id)).then(loadCollections)
                .catch((cause) => setError((cause as Error).message));
            }}
            onUpdatePaper={(paperId, patch) => {
              void updateCollectionPaper(active.id, paperId, patch)
                .then(() => setItems((current) => current.map((item) => (item.paper.id === paperId ? { ...item, ...patch } : item))))
                .catch((cause) => setError((cause as Error).message));
            }}
          />}
        </>
      )}
    </div>
  );
};
