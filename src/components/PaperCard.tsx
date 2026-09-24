import React from 'react';
import {
  Award, Bookmark, BookOpen, Bot, Check, ChevronDown, ChevronUp, Copy, Download,
  ExternalLink, FileCheck2, GitBranch, Loader2, X,
} from 'lucide-react';
import { Paper } from '../types';
import { evaluatePaper, getSourceGroup, SOURCE_GROUPS } from '../lib/paperEvaluation';
import { getPaperKind, KIND_META } from '../lib/paperKind';
import {
  ABSTRACT_CLAMP, CitationDirection, CitationState, isVietnamPaper, originalPaperUrl,
  suggestedKeywords,
} from './Explorer.shared';

export interface PaperCardProps {
  paper: Paper;
  /** Per-card flags rather than the parent's maps: a card only re-renders when
   *  its own state changes, so typing in a filter no longer re-renders the
   *  whole result list. */
  isExpanded: boolean;
  isSaved: boolean;
  isCopied: boolean;
  isCopiedBib: boolean;
  isDownloading: boolean;
  isDownloaded: boolean;
  isSelected: boolean;
  isCitationOpen: boolean;
  /** Ticked for bulk reference export. */
  isChecked: boolean;
  citationState?: CitationState;
  /** Stable across renders (memoised in App), so `memo` below actually holds. */
  savedPaperIds: Set<string>;
  detailRef: React.RefObject<HTMLElement | null>;
  detailTrigger: React.MutableRefObject<HTMLButtonElement | null>;
  onToggleChecked: (id: string) => void;
  onSelect: (id: string) => void;
  onCloseDetails: () => void;
  onToggleAbstract: (id: string) => void;
  onSavePaper: (paper: Paper) => void;
  onCopyCitation: (paper: Paper, format?: 'apa' | 'bibtex') => void;
  onToggleCitations: (paper: Paper) => void;
  onLoadCitations: (paper: Paper, direction: CitationDirection) => void;
  onDownload: (paper: Paper) => void;
  onOpenFulltext: (paper: Paper) => void;
  onOpenAgentExport: (paper: Paper) => void;
}

/**
 * One result row, extracted from `Explorer` so it can be memoised.
 *
 * `Explorer` holds around thirty pieces of state; before this split every one
 * of them re-rendered all four hundred lines of every card, so adjusting a year
 * filter re-rendered the entire list.
 */
function PaperCardImpl({
  paper, isExpanded, isSaved, isCopied, isCopiedBib, isDownloading, isDownloaded,
  isSelected, isCitationOpen, isChecked, citationState, savedPaperIds, detailRef, detailTrigger,
  onToggleChecked, onSelect, onCloseDetails, onToggleAbstract, onSavePaper, onCopyCitation,
  onToggleCitations, onLoadCitations, onDownload, onOpenFulltext, onOpenAgentExport,
}: PaperCardProps) {
    const isVn = isVietnamPaper(paper);
    const kind = getPaperKind(paper);
    const sourceGroup = getSourceGroup(paper);
    const groupMeta = SOURCE_GROUPS[sourceGroup];
    const evaluation = evaluatePaper(paper);
    const abstract = paper.abstract || '';
    const needsClamp = abstract.length > ABSTRACT_CLAMP;
    const keywords = suggestedKeywords(paper);

    return (
      <article
        key={paper.id}
        className={`paper-card compact-paper ${isSelected ? 'selected' : ''}`}
      >
        <div className="paper-row">
          <input
            type="checkbox"
            className="paper-check"
            checked={isChecked}
            onChange={() => onToggleChecked(paper.id)}
            aria-label={`Select ${paper.title}`}
          />
          <div className="paper-body">
            {/* Provenance only: where this result came from and how reachable it
                is. Scores moved to the footer so they stop competing with it. */}
            <div className="paper-badges">
              <span className="badge badge-source badge-essential">{paper.source}</span>
              {kind !== 'article' && KIND_META[kind].badge && (
                <span className={`badge badge-essential ${KIND_META[kind].badge}`}>
                  {KIND_META[kind].label}
                </span>
              )}
              {isVn && <span className="badge badge-vjol badge-essential">🇻🇳 VIETNAM</span>}
              {paper.open_access && <span className="badge badge-oa badge-essential">OPEN ACCESS</span>}
              {evaluation.recommendedPdf && (
                <span className="badge badge-pdf-recommended badge-essential">
                  <FileCheck2 size={11} /> RECOMMENDED PDF
                </span>
              )}
            </div>

            <h3 className="paper-title">
              <button
                className="paper-title-button"
                aria-expanded={isSelected}
                onClick={(event) => {
                  detailTrigger.current = event.currentTarget;
                  onSelect(paper.id);
                }}
              >
                {paper.title}
              </button>
            </h3>

            <div className="paper-meta">
              <span>
                {paper.authors.slice(0, 3).join(', ')}
                {paper.authors.length > 3 ? ' et al.' : ''}
              </span>
              {paper.year && <span>· {paper.year}</span>}
              {paper.venue && (
                <span>
                  · <i>{paper.venue}</i>
                </span>
              )}
              {isSelected && paper.citations != null && (
                <span>
                  · Citations: <b>{paper.citations}</b>
                </span>
              )}
              {isSelected && paper.doi && (
                <span>
                  · DOI:{' '}
                  <a
                    href={`https://doi.org/${paper.doi}`}
                    target="_blank"
                    rel="noreferrer"
                    style={{ color: 'var(--primary-cyan)', textDecoration: 'none' }}
                  >
                    {paper.doi}
                  </a>
                </span>
              )}
            </div>

            <p className="paper-abstract" style={{ margin: 0 }}>
              <b>Abstract: </b>
              {abstract
                ? `${abstract.slice(0, 240).trimEnd()}${abstract.length > 240 ? '…' : ''}`
                : 'The source provided no abstract.'}
            </p>

            {keywords.length > 0 && (
              <div
                className="paper-keywords"
                title="Keywords suggested from the title and abstract"
              >
                {keywords.map((keyword) => (
                  <span key={keyword} className="paper-keyword">
                    {keyword}
                  </span>
                ))}
              </div>
            )}
          </div>
        </div>

        <div className="paper-footer">
          <div className="paper-metrics">
            <span className="badge badge-group" title={groupMeta.label}>
              {groupMeta.shortLabel}
            </span>
            <span
              title="Reading-priority screening score (not a judgement of quality)"
              className={`badge evidence-${
                evaluation.label === 'Recommended'
                  ? 'strong'
                  : evaluation.label === 'Consider'
                  ? 'fair'
                  : 'review'
              }`}
            >
              <Award size={11} /> SCREENING {evaluation.overall}/100
            </span>
            {paper.quartile && <span className="badge badge-q1">{paper.quartile}</span>}
            {paper.score !== undefined && (
              <span
                className="badge"
                title="Multi-source fusion score (Reciprocal Rank Fusion)"
                style={{
                  background: '#f1f5f9',
                  color: 'var(--text-dim)',
                  border: '1px solid var(--cockpit-border)',
                }}
              >
                RRF {paper.score.toFixed(3)}
              </span>
            )}
          </div>

          <div className="paper-cta">
            <button
              className={`action-btn ${isSaved ? 'action-btn-primary' : ''}`}
              onClick={() => onSavePaper(paper)}
              title={isSaved ? 'Remove from interest list' : 'Add this paper to the interest list'}
              aria-label={isSaved ? 'Remove from interest list' : 'Add this paper to the interest list'}
            >
              <Bookmark size={14} fill={isSaved ? '#ffffff' : 'none'} />
              <span>{isSaved ? 'In interest list' : 'Interest'}</span>
            </button>

            {paper.pdf_url && !isSelected && (
              <button
                className="action-btn"
                onClick={() => onDownload(paper)}
                disabled={isDownloading || isDownloaded}
                title="Download the full-text PDF"
              >
                {isDownloading ? (
                  <Loader2 size={14} className="animate-spin" />
                ) : isDownloaded ? (
                  <Check size={14} />
                ) : (
                  <Download size={14} />
                )}
                <span>{isDownloaded ? 'Downloaded' : isDownloading ? 'Downloading…' : 'PDF'}</span>
              </button>
            )}

            {originalPaperUrl(paper) && (
              <a
                className="action-btn"
                href={originalPaperUrl(paper)!}
                target="_blank"
                rel="noreferrer"
                title="Open the paper at its original page"
              >
                <ExternalLink size={13} />
                <span>Original page</span>
              </a>
            )}
          </div>
        </div>

        {/* Inline detail panel on click */}
        {isSelected && (
          <aside
            ref={detailRef}
            tabIndex={-1}
            className="paper-detail-panel"
            aria-label={`Details: ${paper.title}`}
            onKeyDown={(event) => {
              if (event.key === 'Escape') onCloseDetails();
            }}
          >
            <div className="paper-detail-heading">
              <h2>{paper.title}</h2>
              <button className="action-btn" aria-label="Close details" onClick={onCloseDetails}>
                <X size={16} />
              </button>
            </div>
            <p className="paper-meta">
              {[paper.authors.join(', '), paper.year, paper.venue, paper.source]
                .filter(Boolean)
                .join(' · ')}
            </p>

            {abstract && (
              <div>
                <div className="paper-abstract">
                  {isExpanded || !needsClamp
                    ? abstract
                    : `${abstract.slice(0, ABSTRACT_CLAMP).trimEnd()}…`}
                </div>
                {needsClamp && (
                  <button
                    onClick={() => onToggleAbstract(paper.id)}
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
                    {isExpanded ? (
                      <>
                        <span>Collapse</span>
                        <ChevronUp size={14} />
                      </>
                    ) : (
                      <>
                        <span>Read the full abstract</span>
                        <ChevronDown size={14} />
                      </>
                    )}
                  </button>
                )}
              </div>
            )}

            <details className="evidence-details">
              <summary>Preliminary screening (transparent)</summary>
              <div className="evidence-score-grid">
                <span>Relevance <b>{evaluation.relevance}</b></span>
                <span>Metadata completeness <b>{evaluation.metadata}</b></span>
                <span>Recency <b>{evaluation.recency}</b></span>
                <span>Citations <b>{evaluation.citation}</b></span>
                <span>Access <b>{evaluation.access}</b></span>
              </div>
              <p>
                Heuristic score = 40% RRF rank + 40% metadata completeness + 20% access availability. Recency and citations are shown for reference and do not bias it.
              </p>
            </details>

            <div className="paper-actions">
              <div style={{ display: 'flex', gap: 8, flexWrap: 'wrap' }}>
                <button
                  type="button"
                  className="action-btn action-btn-primary"
                  onClick={() => onOpenFulltext(paper)}
                  title="Open the full text in the reader"
                >
                  <BookOpen size={14} />
                  <span>Read full text</span>
                </button>

                <button
                  type="button"
                  className="action-btn"
                  onClick={() => onOpenAgentExport(paper)}
                  title="Export this paper to Google Antigravity, OpenAI Codex, Claude or Obsidian"
                >
                  <Bot size={14} />
                  <span>Send to AI agent</span>
                </button>

                <button
                  className="action-btn"
                  onClick={() => onCopyCitation(paper, 'apa')}
                  title="Copy the APA citation"
                >
                  {isCopied ? <Check size={14} color="var(--status-emerald)" /> : <Copy size={14} />}
                  <span>{isCopied ? 'Copied' : 'Copy APA'}</span>
                </button>

                <button
                  className="action-btn"
                  onClick={() => onCopyCitation(paper, 'bibtex')}
                  title="Copy the BibTeX citation"
                >
                  {isCopiedBib ? <Check size={14} color="var(--status-emerald)" /> : <Copy size={14} />}
                  <span>{isCopiedBib ? 'Copied' : 'Copy BibTeX'}</span>
                </button>

                <button
                  className="action-btn"
                  onClick={() => onToggleCitations(paper)}
                  title="View the citation graph and related papers"
                  aria-expanded={!!isCitationOpen}
                >
                  <GitBranch size={14} />
                  <span>{isCitationOpen ? 'Hide citations' : 'Citations & related'}</span>
                </button>
              </div>

              {paper.pdf_url && (
                <button
                  className="action-btn action-btn-primary"
                  onClick={() => onDownload(paper)}
                  disabled={isDownloading || isDownloaded}
                >
                  {isDownloading ? (
                    <Loader2 size={14} className="animate-spin" />
                  ) : isDownloaded ? (
                    <Check size={14} />
                  ) : (
                    <Download size={14} />
                  )}
                  <span>
                    {isDownloaded
                      ? 'Downloaded'
                      : isDownloading
                      ? 'Downloading PDF…'
                      : 'Download full PDF'}
                  </span>
                </button>
              )}
            </div>

            {isCitationOpen && (
              <div style={{ marginTop: 12, borderTop: '1px solid var(--cockpit-border)', paddingTop: 10 }}>
                <div className="segmented" role="tablist" style={{ alignSelf: 'flex-start', marginBottom: 8 }}>
                  {([
                    ['cited_by', 'Cited by'],
                    ['references', 'References'],
                    ['related', 'Related'],
                  ] as [CitationDirection, string][]).map(([id, label]) => (
                    <button
                      key={id}
                      type="button"
                      role="tab"
                      aria-selected={citationState?.direction === id}
                      className={`segmented-item ${
                        citationState?.direction === id ? 'active' : ''
                      }`}
                      onClick={() => onLoadCitations(paper, id)}
                      style={{ fontSize: 11 }}
                    >
                      {label}
                    </button>
                  ))}
                </div>

                {citationState?.loading && (
                  <div style={{ fontSize: 12, color: 'var(--text-muted)' }}>
                    Loading the citation graph from OpenAlex…
                  </div>
                )}
                {citationState?.error && (
                  <div className="alert alert-warning" style={{ margin: 0 }}>
                    {citationState?.error}
                  </div>
                )}
                {!citationState?.loading &&
                  !citationState?.error &&
                  (citationState?.items.length ?? 0) === 0 && (
                    <div style={{ fontSize: 12, color: 'var(--text-muted)' }}>
                      No citation-graph data found (requires a DOI, PMID or OpenAlex ID).
                    </div>
                  )}

                {(citationState?.items ?? []).map((item) => (
                  <div
                    key={item.id}
                    style={{
                      display: 'flex',
                      gap: 10,
                      alignItems: 'flex-start',
                      padding: '8px 0',
                      borderBottom: '1px solid var(--cockpit-border)',
                    }}
                  >
                    <div style={{ flex: 1, minWidth: 0 }}>
                      <div style={{ fontSize: 13, fontWeight: 600, color: 'var(--text-main)' }}>
                        {item.title}
                      </div>
                      <div style={{ fontSize: 11, color: 'var(--text-muted)' }}>
                        {[item.authors.slice(0, 3).join(', '), item.year, item.source]
                          .filter(Boolean)
                          .join(' • ')}
                      </div>
                    </div>
                    <div style={{ display: 'flex', gap: 6, flexShrink: 0 }}>
                      {originalPaperUrl(item) && (
                        <a
                          className="action-btn"
                          href={originalPaperUrl(item)!}
                          target="_blank"
                          rel="noreferrer"
                          style={{ padding: '3px 8px', fontSize: 11, textDecoration: 'none' }}
                        >
                          Open
                        </a>
                      )}
                      <button
                        className={`action-btn ${savedPaperIds.has(item.id) ? 'action-btn-primary' : ''}`}
                        onClick={() => onSavePaper(item)}
                        title={savedPaperIds.has(item.id) ? 'Remove from interest list' : 'Add to interest list'}
                        style={{ padding: '3px 8px', fontSize: 11 }}
                      >
                        <Bookmark
                          size={12}
                          fill={savedPaperIds.has(item.id) ? '#ffffff' : 'none'}
                        />
                      </button>
                    </div>
                  </div>
                ))}
              </div>
            )}
          </aside>
        )}
      </article>
    );
}

export const PaperCard = React.memo(PaperCardImpl);
