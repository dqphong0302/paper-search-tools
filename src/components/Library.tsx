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
} from 'lucide-react';
import { ReadingStatus, WorkspacePaper, WorkspacePaperPatch } from '../types';
import { isVietnamPaper } from './Explorer';
import { bibtexLibrary, risLibrary } from '../lib/citation';
import { getPaperKind, KIND_META } from '../lib/paperKind';

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
}

export const Library: React.FC<LibraryProps> = ({ workspacePapers, onRemovePaper, onUpdatePaper }) => {
  const [exported, setExported] = useState(false);
  const [exportedBib, setExportedBib] = useState(false);
  const [filter, setFilter] = useState<Filter>('all');
  const [expanded, setExpanded] = useState<Record<string, boolean>>({});
  const [noteOpen, setNoteOpen] = useState<Record<string, boolean>>({});
  const [noteDrafts, setNoteDrafts] = useState<Record<string, string>>({});
  const [tagDrafts, setTagDrafts] = useState<Record<string, string>>({});
  const [selectedId, setSelectedId] = useState<string | null>(null);
  const detailRef = useRef<HTMLElement>(null);
  const triggerRef = useRef<HTMLButtonElement | null>(null);
  useEffect(() => { if (selectedId) detailRef.current?.focus(); }, [selectedId]);
  const closeDetails = () => { setSelectedId(null); triggerRef.current?.focus(); };

  const download = (content: string, extension: string, mime: string) => {
    const blob = new Blob([content], { type: mime });
    const url = URL.createObjectURL(blob);
    const a = document.createElement('a');
    a.href = url;
    a.download = `scholargateway_${new Date().toISOString().slice(0, 10)}.${extension}`;
    document.body.appendChild(a);
    a.click();
    a.remove();
    URL.revokeObjectURL(url);
  };

  const exportZoteroRis = () => {
    if (workspacePapers.length === 0) return;
    download(risLibrary(workspacePapers.map((wp) => wp.paper)), 'ris', 'application/x-research-info-systems');
    setExported(true);
    setTimeout(() => setExported(false), 2500);
  };

  const exportBibtex = () => {
    if (workspacePapers.length === 0) return;
    download(bibtexLibrary(workspacePapers.map((wp) => wp.paper)), 'bib', 'application/x-bibtex');
    setExportedBib(true);
    setTimeout(() => setExportedBib(false), 2500);
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
    { id: 'unread', label: 'Unread', count: counts.unread },
    { id: 'reading', label: 'Reading', count: counts.reading },
    { id: 'read', label: 'Read', count: counts.read },
    { id: 'favorite', label: '★ Favorites', count: counts.favorite },
  ];

  return (
    <div className="page-container">
      <div className="page-header">
        <div>
          <h2 className="page-title">
            <Bookmark size={17} style={{ color: 'var(--primary-cyan)' }} />
            <span>
              {workspacePapers.length} {workspacePapers.length === 1 ? 'saved paper' : 'saved papers'}
            </span>
          </h2>
        </div>

        {workspacePapers.length > 0 && (
          <div style={{ display: 'flex', gap: 8, flexWrap: 'wrap' }}>
            <button className="action-btn action-btn-primary" onClick={exportZoteroRis}>
              {exported ? <Check size={14} /> : <FileDown size={14} />}
              <span>{exported ? 'Downloaded .RIS' : 'Export Zotero (.RIS)'}</span>
            </button>
            <button className="action-btn" onClick={exportBibtex}>
              {exportedBib ? <Check size={14} /> : <FileDown size={14} />}
              <span>{exportedBib ? 'Downloaded .bib' : 'Export BibTeX (.bib)'}</span>
            </button>
          </div>
        )}
      </div>

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

      {workspacePapers.length === 0 ? (
        <div className="empty-state">
          <div className="empty-state-icon">
            <Bookmark size={24} />
          </div>
          <div className="empty-state-title">No papers saved in this workspace</div>
          <div className="empty-state-text">
            In the <b>Search</b> tab, click <b>“Save”</b> on any paper to add it to the active workspace.
          </div>
        </div>
      ) : displayed.length === 0 ? (
        <div className="empty-state">
          <div className="empty-state-title">No papers match this filter</div>
          <div className="empty-state-text">Switch back to the "All" tab to view all papers in this workspace.</div>
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

            return (
              <article key={paper.id} className={`paper-card compact-paper ${selectedId === paper.id ? 'selected' : ''}`}>
                <div className="paper-header">
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
                      {wp.favorite && <span className="badge badge-violet badge-essential">★ FAVORITE</span>}
                      {status === 'reading' && <span className="badge badge-cyan badge-essential">READING</span>}
                      {status === 'read' && <span className="badge badge-emerald badge-essential">READ</span>}
                      {(wp.tags ?? []).map((tag) => (
                        <span key={tag} className="badge badge-group">#{tag}</span>
                      ))}
                    </div>
                    <h3 className="paper-title"><button className="paper-title-button" aria-expanded={selectedId === paper.id}
                      onClick={event => { triggerRef.current = event.currentTarget; setSelectedId(paper.id); }}>{paper.title}</button></h3>
                  </div>

                  <button
                    className="action-btn action-btn-danger"
                    onClick={() => onRemovePaper(paper.id)}
                    title="Remove from workspace"
                    aria-label="Remove from workspace"
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

                {selectedId === paper.id && <aside className="paper-detail-panel" ref={detailRef} tabIndex={-1}
                  aria-label={`Saved paper: ${paper.title}`} onKeyDown={event => { if (event.key === 'Escape') closeDetails(); }}>
                  <div className="paper-detail-heading"><h2>{paper.title}</h2>
                    <button className="action-btn" aria-label="Close saved paper details" onClick={closeDetails}><X size={16} /></button>
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
                <div style={{ display: 'flex', gap: 8, alignItems: 'center', flexWrap: 'wrap', marginBottom: 10 }}>
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
                    <Star size={13} fill={wp.favorite ? '#f59e0b' : 'none'} color={wp.favorite ? '#f59e0b' : undefined} />
                    <span>Favorite</span>
                  </button>
                </div>

                {/* Tags */}
                <div style={{ display: 'flex', gap: 8, alignItems: 'center', marginBottom: 10, flexWrap: 'wrap' }}>
                  <input
                    className="field-input"
                    aria-label="Paper tags"
                    style={{ flex: 1, minWidth: 180 }}
                    placeholder="tags, comma-separated (e.g. oncology, immunotherapy)"
                    value={tagValue(wp)}
                    onChange={(e) => setTagDrafts((p) => ({ ...p, [paper.id]: e.target.value }))}
                  />
                  <button className="action-btn" onClick={() => saveTags(wp)} style={{ padding: '4px 10px', fontSize: 12 }}>
                    <Check size={13} /> Save tags
                  </button>
                </div>

                {/* Note */}
                <div style={{ marginBottom: 12 }}>
                  <button
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
                        placeholder="Personal notes: rationale for selection, study strengths/limitations, follow-up questions…"
                        value={noteValue(wp)}
                        onChange={(e) => setNoteDrafts((p) => ({ ...p, [paper.id]: e.target.value }))}
                      />
                      <div style={{ display: 'flex', gap: 8 }}>
                        <button
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
                    {(paper.source_url || paper.doi) && (
                      <a
                        className="action-btn"
                        href={paper.source_url || `https://doi.org/${paper.doi}`}
                        target="_blank"
                        rel="noreferrer"
                        style={{ textDecoration: 'none' }}
                      >
                        <ExternalLink size={14} />
                        <span>View at Source</span>
                      </a>
                    )}
                  </div>

                  {paper.pdf_url && (
                    <a
                      className="action-btn action-btn-primary"
                      href={paper.pdf_url}
                      target="_blank"
                      rel="noreferrer"
                      style={{ textDecoration: 'none' }}
                    >
                      <Download size={14} />
                      <span>View Full PDF</span>
                    </a>
                  )}
                </div>
                </aside>}
              </article>
            );
          })}
        </div>
      )}
    </div>
  );
};
