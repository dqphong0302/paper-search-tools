import React, { useEffect, useRef, useState } from 'react';
import {
  Bookmark,
  Check,
  CheckCircle2,
  ChevronDown,
  ChevronUp,
  Circle,
  Download,
  ExternalLink,
  FileDown,
  Star,
  Trash2,
  X,
  Bot,
  BookOpen,
  Loader2,
  DownloadCloud,
} from 'lucide-react';
import { Paper, ReadingStatus, WorkspacePaper, WorkspacePaperPatch } from '../types';
import { isVietnamPaper, originalPaperUrl } from './Explorer';
import { bibtexLibrary, risLibrary } from '../lib/citation';
import { useSelection } from '../lib/useSelection';
import { getPaperKind, KIND_META } from '../lib/paperKind';
import { FulltextViewerModal } from './FulltextViewerModal';
import { AiAgentExportModal } from './AiAgentExportModal';
import { canDownloadPdf, requestPdfDownload } from '../lib/pdfDownload';

const ABSTRACT_CLAMP = 280;

const STATUS_META: { id: ReadingStatus; label: string; icon: React.ReactNode }[] = [
  { id: 'unread', label: 'Unread', icon: <Circle size={12} /> },
  { id: 'reading', label: 'Reading', icon: <Bookmark size={12} /> },
  { id: 'read', label: 'Read', icon: <CheckCircle2 size={12} /> },
];

type Filter = 'all' | ReadingStatus | 'favorite';

interface LibraryProps {
  workspacePapers: WorkspacePaper[];
  onRemovePaper: (id: string) => void;
  onUpdatePaper: (id: string, patch: WorkspacePaperPatch) => void;
  workspaceName?: string;
}

export const Library: React.FC<LibraryProps> = ({
  workspacePapers,
  onRemovePaper,
  onUpdatePaper,
  workspaceName = 'Interest Library',
}) => {
  const [exported, setExported] = useState(false);
  const [exportedBib, setExportedBib] = useState(false);
  const [exportedOneId, setExportedOneId] = useState<string | null>(null);
  const [filter, setFilter] = useState<Filter>('all');
  const [expanded, setExpanded] = useState<Record<string, boolean>>({});
  const [noteOpen, setNoteOpen] = useState<Record<string, boolean>>({});
  const [noteDrafts, setNoteDrafts] = useState<Record<string, string>>({});
  const [tagDrafts, setTagDrafts] = useState<Record<string, string>>({});
  const [selectedId, setSelectedId] = useState<string | null>(null);
  const detailRef = useRef<HTMLElement>(null);
  const triggerRef = useRef<HTMLButtonElement | null>(null);

  // Fulltext Viewer & AI Agent Export Modal states
  const [readerPaper, setReaderPaper] = useState<Paper | null>(null);
  const [readerOpen, setReaderOpen] = useState(false);
  const [agentExportOpen, setAgentExportOpen] = useState(false);
  const [agentExportPapers, setAgentExportPapers] = useState<Paper[]>([]);

  // Batch download state
  const [batchDownloading, setBatchDownloading] = useState(false);
  const [batchDownloadSuccess, setBatchDownloadSuccess] = useState<{ done: number; total: number } | null>(null);
  const [downloadError, setDownloadError] = useState<string | null>(null);
  const [downloadingIds, setDownloadingIds] = useState<Set<string>>(new Set());

  useEffect(() => {
    if (selectedId) detailRef.current?.focus();
  }, [selectedId]);

  const closeDetails = () => {
    setSelectedId(null);
    triggerRef.current?.focus();
  };

  const download = (content: string, extension: string, mime: string, name?: string) => {
    const blob = new Blob([content], { type: mime });
    const url = URL.createObjectURL(blob);
    const a = document.createElement('a');
    a.href = url;
    a.download = `${name ?? `scholargate_${new Date().toISOString().slice(0, 10)}`}.${extension}`;
    document.body.appendChild(a);
    a.click();
    a.remove();
    URL.revokeObjectURL(url);
  };

  // One reference, named after the paper rather than the date, so a folder of
  // single exports stays readable.
  const exportOneRis = (paper: Paper) => {
    const slug =
      paper.title
        .toLowerCase()
        .replace(/[^a-z0-9]+/g, '-')
        .replace(/^-|-$/g, '')
        .slice(0, 60) || 'reference';
    download(risLibrary([paper]), 'ris', 'application/x-research-info-systems', slug);
    setExportedOneId(paper.id);
    setTimeout(() => setExportedOneId(null), 2500);
  };

  const exportRis = () => {
    const papers = papersToExport();
    if (papers.length === 0) return;
    download(risLibrary(papers), 'ris', 'application/x-research-info-systems');
    setExported(true);
    setTimeout(() => setExported(false), 2500);
  };

  const exportBibtex = () => {
    const papers = papersToExport();
    if (papers.length === 0) return;
    download(bibtexLibrary(papers), 'bib', 'application/x-bibtex');
    setExportedBib(true);
    setTimeout(() => setExportedBib(false), 2500);
  };

  const handleDownloadPaperPdf = async (paper: Paper): Promise<boolean> => {
    if (!canDownloadPdf(paper)) return false;
    setDownloadingIds((prev) => new Set(prev).add(paper.id));
    const result = await requestPdfDownload(paper);
    setDownloadingIds((prev) => {
      const next = new Set(prev);
      next.delete(paper.id);
      return next;
    });
    if (!result.ok) {
      setDownloadError(`${paper.title}: ${result.error}`);
      setTimeout(() => setDownloadError(null), 12000);
    }
    return result.ok;
  };

  const handleBatchDownload = async () => {
    const papersWithPdf = workspacePapers.map((wp) => wp.paper).filter(canDownloadPdf);
    if (papersWithPdf.length === 0) return;

    setBatchDownloading(true);
    let count = 0;
    for (const paper of papersWithPdf) {
      if (await handleDownloadPaperPdf(paper)) count++;
    }
    setBatchDownloading(false);
    setBatchDownloadSuccess({ done: count, total: papersWithPdf.length });
    setTimeout(() => setBatchDownloadSuccess(null), 6000);
  };

  const openExportAllToAgent = () => {
    setAgentExportPapers(workspacePapers.map((wp) => wp.paper));
    setAgentExportOpen(true);
  };

  const openExportSingleToAgent = (paper: Paper) => {
    setAgentExportPapers([paper]);
    setAgentExportOpen(true);
  };

  const counts = {
    all: workspacePapers.length,
    unread: workspacePapers.filter((wp) => (wp.status ?? 'unread') === 'unread').length,
    reading: workspacePapers.filter((wp) => wp.status === 'reading').length,
    read: workspacePapers.filter((wp) => wp.status === 'read').length,
    favorite: workspacePapers.filter((wp) => wp.favorite).length,
  };

  const displayed =
    filter === 'all'
      ? workspacePapers
      : filter === 'favorite'
      ? workspacePapers.filter((wp) => wp.favorite)
      : workspacePapers.filter((wp) => (wp.status ?? 'unread') === filter);

  // Selection follows the visible rows, and the export buttons act on it when
  // anything is ticked. With nothing ticked they keep their old meaning —
  // export everything — so the common case still takes one click.
  const visibleIds = React.useMemo(() => displayed.map((wp) => wp.paper.id), [displayed]);
  const selection = useSelection(visibleIds);
  const papersToExport = (): Paper[] =>
    selection.count > 0
      ? workspacePapers.filter((wp) => selection.isSelected(wp.paper.id)).map((wp) => wp.paper)
      : workspacePapers.map((wp) => wp.paper);

  const noteValue = (wp: WorkspacePaper) => noteDrafts[wp.paper.id] ?? wp.note ?? '';
  const tagValue = (wp: WorkspacePaper) => tagDrafts[wp.paper.id] ?? (wp.tags ?? []).join(', ');

  const saveTags = (wp: WorkspacePaper) => {
    const tags = tagValue(wp)
      .split(',')
      .map((tag) => tag.trim())
      .filter(Boolean);
    onUpdatePaper(wp.paper.id, { tags });
  };

  const filters: { id: Filter; label: string; count: number }[] = [
    { id: 'all', label: 'All', count: counts.all },
    { id: 'favorite', label: '★ Favourites', count: counts.favorite },
    { id: 'reading', label: 'Reading', count: counts.reading },
    { id: 'unread', label: 'Unread', count: counts.unread },
    { id: 'read', label: 'Read', count: counts.read },
  ];

  const totalPdfs = workspacePapers.filter((wp) => canDownloadPdf(wp.paper)).length;

  return (
    <div className="page-container">
      <div className="page-header" style={{ flexWrap: 'wrap', gap: 12 }}>
        <div>
          <h2 className="page-title">
            <Bookmark size={17} style={{ color: 'var(--primary-cyan)' }} />
            <span>
              {workspacePapers.length} papers of interest
            </span>
          </h2>
        </div>

        {workspacePapers.length > 0 && (
          <div style={{ display: 'flex', gap: 8, flexWrap: 'wrap' }}>
            <button
              type="button"
              className="action-btn action-btn-primary"
              onClick={openExportAllToAgent}
              title="Export the whole interest library to Google Antigravity, OpenAI Codex, Claude or Obsidian"
            >
              <Bot size={14} />
              <span>Send to AI agent ({workspacePapers.length})</span>
            </button>

            {totalPdfs > 0 && (
              <button
                type="button"
                className="action-btn"
                onClick={handleBatchDownload}
                disabled={batchDownloading}
                title="Download every available PDF in the interest library"
              >
                {batchDownloading ? (
                  <Loader2 size={14} className="animate-spin" />
                ) : batchDownloadSuccess !== null ? (
                  <Check size={14} color="var(--status-emerald)" />
                ) : (
                  <DownloadCloud size={14} />
                )}
                <span>
                  {batchDownloading
                    ? 'Batch downloading…'
                    : batchDownloadSuccess !== null
                    ? `Downloaded ${batchDownloadSuccess.done}/${batchDownloadSuccess.total} PDFs`
                    : `Download all PDFs (${totalPdfs})`}
                </span>
              </button>
            )}

            <button
              type="button"
              id="export-ris"
              className="action-btn"
              onClick={exportRis}
              title="RIS is the shared import format of Zotero, EndNote and Mendeley"
            >
              {exported ? <Check size={14} /> : <FileDown size={14} />}
              <span>
                {exported
                  ? '.RIS downloaded'
                  : `Export .RIS — Zotero / EndNote (${papersToExport().length})`}
              </span>
            </button>
            <button type="button" id="export-bibtex" className="action-btn" onClick={exportBibtex}>
              {exportedBib ? <Check size={14} /> : <FileDown size={14} />}
              <span>
                {exportedBib ? '.bib downloaded' : `Export BibTeX (${papersToExport().length})`}
              </span>
            </button>
          </div>
        )}
      </div>

      {downloadError && (
        <div className="alert alert-warning floating-alert" role="alert">
          {downloadError}
        </div>
      )}

      {workspacePapers.length > 0 && (
        <div className="segmented" style={{ alignSelf: 'flex-start' }} role="tablist">
          {filters
            .filter((item) => item.id === 'all' || item.count > 0 || filter === item.id)
            .map((item) => (
              <button
                key={item.id}
                role="tab"
                aria-selected={filter === item.id}
                className={`segmented-item ${filter === item.id ? 'active' : ''}`}
                onClick={() => setFilter(item.id)}
              >
                <span>{item.label}</span>
                <span className="segmented-count">{item.count}</span>
              </button>
            ))}
        </div>
      )}

      {displayed.length > 0 && (
        <div
          style={{
            display: 'flex',
            alignItems: 'center',
            gap: 12,
            padding: '8px 12px',
            border: '1px solid var(--cockpit-border)',
            borderRadius: 'var(--radius-sm)',
            background: 'var(--cockpit-card)',
            fontSize: 12,
          }}
        >
          <label style={{ display: 'flex', alignItems: 'center', gap: 8, cursor: 'pointer' }}>
            <input
              id="select-all-papers"
              type="checkbox"
              checked={selection.allVisibleSelected}
              ref={(node) => {
                // Partly-selected reads as neither on nor off.
                if (node) {
                  node.indeterminate =
                    selection.visibleSelectedCount > 0 && !selection.allVisibleSelected;
                }
              }}
              onChange={selection.toggleAllVisible}
              aria-label="Select all visible papers"
              style={{ accentColor: 'var(--primary-cyan)' }}
            />
            <span>Select all ({displayed.length})</span>
          </label>

          {selection.count > 0 ? (
            <>
              <span style={{ color: 'var(--primary-cyan)', fontWeight: 600 }}>
                {selection.count} selected
              </span>
              <button type="button" className="action-btn" onClick={selection.clear} style={{ padding: '4px 10px' }}>
                Clear selection
              </button>
              <span style={{ color: 'var(--text-dim)' }}>
                Export buttons above apply to the selection.
              </span>
            </>
          ) : (
            <span style={{ color: 'var(--text-dim)' }}>
              Tick papers to export just those; with none ticked the export covers the whole library.
            </span>
          )}
        </div>
      )}

      {workspacePapers.length === 0 ? (
        <div className="empty-state">
          <div className="empty-state-icon">
            <Bookmark size={24} />
          </div>
          <div className="empty-state-title">No papers marked as interesting yet</div>
          <div className="empty-state-text">
            In the <b>Search</b> tab, click <b>“Interest”</b> on a paper you want to revisit.
          </div>
        </div>
      ) : displayed.length === 0 ? (
        <div className="empty-state">
          <div className="empty-state-title">No papers match the filters</div>
          <div className="empty-state-text">Switch to the "All" tab to see every paper of interest.</div>
        </div>
      ) : (
        <div>
          {displayed.map((wp) => {
            const paper = wp.paper;
            const status: ReadingStatus = wp.status ?? 'unread';
            const abstract = paper.abstract || '';
            const needsClamp = abstract.length > ABSTRACT_CLAMP;
            const isExpanded = !!expanded[paper.id];
            const isNoteOpen = !!noteOpen[paper.id];
            const isDownloading = downloadingIds.has(paper.id);

            return (
              <article
                key={paper.id}
                className={`paper-card compact-paper ${selectedId === paper.id ? 'selected' : ''}`}
              >
                <div className="paper-header">
                  <input
                    type="checkbox"
                    checked={selection.isSelected(paper.id)}
                    onChange={() => selection.toggle(paper.id)}
                    aria-label={`Select ${paper.title}`}
                    style={{ accentColor: 'var(--primary-cyan)', marginTop: 4, flexShrink: 0 }}
                  />
                  <div style={{ flex: 1, minWidth: 0 }}>
                    <div className="paper-badges">
                      <span className="badge badge-source badge-essential">{paper.source}</span>
                      {getPaperKind(paper) !== 'article' && KIND_META[getPaperKind(paper)].badge && (
                        <span className={`badge badge-essential ${KIND_META[getPaperKind(paper)].badge}`}>
                          {KIND_META[getPaperKind(paper)].label}
                        </span>
                      )}
                      {isVietnamPaper(paper) && <span className="badge badge-vjol badge-essential">🇻🇳 VIETNAM</span>}
                      {paper.open_access && <span className="badge badge-oa badge-essential">OPEN ACCESS</span>}
                      {wp.favorite && <span className="badge badge-violet badge-essential">★ FAVOURITE</span>}
                      {status === 'reading' && <span className="badge badge-cyan badge-essential">READING</span>}
                      {status === 'read' && <span className="badge badge-emerald badge-essential">READ</span>}
                      {(wp.tags ?? []).map((tag) => (
                        <span key={tag} className="badge badge-group">
                          #{tag}
                        </span>
                      ))}
                    </div>

                    <h3 className="paper-title">
                      <button
                        className="paper-title-button"
                        aria-expanded={selectedId === paper.id}
                        onClick={(event) => {
                          triggerRef.current = event.currentTarget;
                          setSelectedId(paper.id);
                        }}
                      >
                        {paper.title}
                      </button>
                    </h3>
                  </div>

                  {/* Favorite Toggle Button */}
                  <button
                    type="button"
                    className={`action-btn ${wp.favorite ? 'action-btn-primary' : ''}`}
                    onClick={() => onUpdatePaper(paper.id, { favorite: !wp.favorite })}
                    title={wp.favorite ? 'Remove from favourites' : 'Mark as favourite'}
                    style={{ flexShrink: 0, padding: '5px 8px' }}
                  >
                    <Star
                      size={14}
                      fill={wp.favorite ? '#f59e0b' : 'none'}
                      color={wp.favorite ? '#f59e0b' : undefined}
                    />
                  </button>

                  <button
                    type="button"
                    className="action-btn action-btn-danger"
                    onClick={() => onRemovePaper(paper.id)}
                    title="Remove from the interest list"
                    aria-label="Remove from the interest list"
                    style={{ flexShrink: 0 }}
                  >
                    <Trash2 size={14} />
                  </button>
                </div>

                <div className="paper-meta">
                  <span>{paper.authors.join(', ')}</span>
                  {paper.venue && (
                    <span>
                      • <i>{paper.venue}</i>
                    </span>
                  )}
                  {paper.year && <span>• {paper.year}</span>}
                  {selectedId === paper.id && paper.citations != null && (
                    <span>
                      • Citations: <b>{paper.citations}</b>
                    </span>
                  )}
                </div>

                {selectedId === paper.id && (
                  <aside
                    className="paper-detail-panel"
                    ref={detailRef}
                    tabIndex={-1}
                    aria-label={`Paper of interest: ${paper.title}`}
                    onKeyDown={(event) => {
                      if (event.key === 'Escape') closeDetails();
                    }}
                  >
                    <div className="paper-detail-heading">
                      <h2>{paper.title}</h2>
                      <button className="action-btn" aria-label="Close saved paper details" onClick={closeDetails}>
                        <X size={16} />
                      </button>
                    </div>

                    {abstract && (
                      <div>
                        <div className="paper-abstract">
                          {isExpanded || !needsClamp
                            ? abstract
                            : `${abstract.slice(0, ABSTRACT_CLAMP).trimEnd()}…`}
                        </div>
                        {needsClamp && (
                          <button
                            onClick={() => setExpanded((p) => ({ ...p, [paper.id]: !isExpanded }))}
                            style={{
                              background: 'none',
                              border: 'none',
                              color: 'var(--primary-cyan)',
                              fontSize: 12,
                              fontFamily: 'inherit',
                              cursor: 'pointer',
                              display: 'flex',
                              alignItems: 'center',
                              gap: 4,
                              marginBottom: 12,
                              padding: 0,
                            }}
                          >
                            <span>{isExpanded ? 'Collapse' : 'Read abstract'}</span>
                            {isExpanded ? <ChevronUp size={14} /> : <ChevronDown size={14} />}
                          </button>
                        )}
                      </div>
                    )}

                    {/* Reading status + favorite */}
                    <div
                      style={{
                        display: 'flex',
                        gap: 8,
                        alignItems: 'center',
                        flexWrap: 'wrap',
                        marginBottom: 10,
                      }}
                    >
                      <div className="segmented" role="group" aria-label="Reading status">
                        {STATUS_META.map((meta) => (
                          <button
                            key={meta.id}
                            type="button"
                            aria-pressed={status === meta.id}
                            className={`segmented-item ${status === meta.id ? 'active' : ''}`}
                            onClick={() => onUpdatePaper(paper.id, { status: meta.id })}
                            style={{ fontSize: 11 }}
                          >
                            {meta.icon}
                            <span>{meta.label}</span>
                          </button>
                        ))}
                      </div>
                      <button
                        type="button"
                        className={`action-btn ${wp.favorite ? 'action-btn-primary' : ''}`}
                        onClick={() => onUpdatePaper(paper.id, { favorite: !wp.favorite })}
                        title={wp.favorite ? 'Remove favorite' : 'Mark as favorite'}
                        style={{ padding: '4px 10px', fontSize: 12 }}
                      >
                        <Star
                          size={13}
                          fill={wp.favorite ? '#f59e0b' : 'none'}
                          color={wp.favorite ? '#f59e0b' : undefined}
                        />
                        <span>Favourite</span>
                      </button>
                    </div>

                    {/* Tags */}
                    <div
                      style={{
                        display: 'flex',
                        gap: 8,
                        alignItems: 'center',
                        marginBottom: 10,
                        flexWrap: 'wrap',
                      }}
                    >
                      <input
                        className="field-input"
                        aria-label="Paper tags"
                        style={{ flex: 1, minWidth: 180 }}
                        placeholder="Tags, comma separated (e.g. biomedicine, AI, deep learning)"
                        value={tagValue(wp)}
                        onChange={(e) => setTagDrafts((p) => ({ ...p, [paper.id]: e.target.value }))}
                      />
                      <button
                        type="button"
                        className="action-btn"
                        onClick={() => saveTags(wp)}
                        style={{ padding: '4px 10px', fontSize: 12 }}
                      >
                        <Check size={13} /> Save tags
                      </button>
                    </div>

                    {/* Note */}
                    <div style={{ marginBottom: 12 }}>
                      <button
                        type="button"
                        className="action-btn"
                        onClick={() => setNoteOpen((p) => ({ ...p, [paper.id]: !isNoteOpen }))}
                        style={{ padding: '4px 10px', fontSize: 12 }}
                      >
                        <Bookmark size={13} />
                        <span>{isNoteOpen ? 'Hide Note' : wp.note ? 'Edit Note' : 'Add Note'}</span>
                      </button>
                      {isNoteOpen && (
                        <div style={{ marginTop: 8, display: 'flex', flexDirection: 'column', gap: 6 }}>
                          <textarea
                            className="field-input"
                            aria-label="Paper note"
                            rows={3}
                            placeholder="Personal notes: method, findings, relevance to your project…"
                            value={noteValue(wp)}
                            onChange={(e) => setNoteDrafts((p) => ({ ...p, [paper.id]: e.target.value }))}
                          />
                          <div style={{ display: 'flex', gap: 8 }}>
                            <button
                              type="button"
                              className="action-btn action-btn-primary"
                              onClick={() => onUpdatePaper(paper.id, { note: noteValue(wp) })}
                              style={{ padding: '4px 12px', fontSize: 12 }}
                            >
                              <Check size={13} /> Save Note
                            </button>
                          </div>
                        </div>
                      )}
                    </div>

                    <div className="paper-actions">
                      <div style={{ display: 'flex', gap: 8, flexWrap: 'wrap' }}>
                        <button
                          type="button"
                          className="action-btn action-btn-primary"
                          onClick={() => {
                            setReaderPaper(paper);
                            setReaderOpen(true);
                          }}
                        >
                          <BookOpen size={14} />
                          <span>Read full text</span>
                        </button>

                        <button
                          type="button"
                          className="action-btn"
                          onClick={() => openExportSingleToAgent(paper)}
                        >
                          <Bot size={14} />
                          <span>Send to AI agent</span>
                        </button>

                        <button
                          type="button"
                          className="action-btn"
                          onClick={() => exportOneRis(paper)}
                          title="Download this one reference as .RIS (Zotero, EndNote, Mendeley)"
                        >
                          <FileDown size={14} />
                          <span>{exportedOneId === paper.id ? '.RIS downloaded' : 'Export .RIS'}</span>
                        </button>

                        {originalPaperUrl(paper) && (
                          <a
                            className="action-btn"
                            href={originalPaperUrl(paper)!}
                            target="_blank"
                            rel="noreferrer"
                            style={{ textDecoration: 'none' }}
                          >
                            <ExternalLink size={14} />
                            <span>Original page · {paper.source}</span>
                          </a>
                        )}
                      </div>

                      {canDownloadPdf(paper) && (
                        <button
                          type="button"
                          className="action-btn action-btn-primary"
                          onClick={() => handleDownloadPaperPdf(paper)}
                          disabled={isDownloading}
                        >
                          {isDownloading ? (
                            <Loader2 size={14} className="animate-spin" />
                          ) : (
                            <Download size={14} />
                          )}
                          <span>{isDownloading ? 'Loading…' : 'Download full PDF'}</span>
                        </button>
                      )}
                    </div>
                  </aside>
                )}
              </article>
            );
          })}
        </div>
      )}

      {/* Modals */}
      <FulltextViewerModal
        isOpen={readerOpen}
        onClose={() => setReaderOpen(false)}
        paper={readerPaper}
        isSaved={readerPaper ? workspacePapers.some((wp) => wp.paper.id === readerPaper.id) : false}
        isFavorite={
          readerPaper
            ? workspacePapers.some((wp) => wp.paper.id === readerPaper.id && wp.favorite)
            : false
        }
        onToggleFavorite={(paper) => {
          const wp = workspacePapers.find((item) => item.paper.id === paper.id);
          if (wp) {
            onUpdatePaper(paper.id, { favorite: !wp.favorite });
          }
        }}
        onDownloadPdf={handleDownloadPaperPdf}
      />

      <AiAgentExportModal
        isOpen={agentExportOpen}
        onClose={() => setAgentExportOpen(false)}
        papers={agentExportPapers}
        workspaceName={workspaceName}
      />
    </div>
  );
};
