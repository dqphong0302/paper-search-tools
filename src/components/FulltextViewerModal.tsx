import React, { useState } from 'react';
import {
  X,
  BookOpen,
  Download,
  Copy,
  Check,
  ExternalLink,
  Star,
  Bookmark,
  Award,
  FileText,
  FileCheck2,
  Calendar,
  Users,
  Building,
  Hash,
  Share2,
} from 'lucide-react';
import { Paper } from '../types';
import { apaCitation, bibtexCitation } from '../lib/citation';
import { evaluatePaper, getSourceGroup, SOURCE_GROUPS } from '../lib/paperEvaluation';
import { getPaperKind, KIND_META } from '../lib/paperKind';

export interface FulltextViewerModalProps {
  paper: Paper | null;
  isOpen: boolean;
  onClose: () => void;
  onSavePaper?: (paper: Paper) => void;
  isSaved?: boolean;
  isFavorite?: boolean;
  onToggleFavorite?: (paper: Paper) => void;
  onDownloadPdf?: (paper: Paper) => void;
  isDownloadingPdf?: boolean;
  isDownloadedPdf?: boolean;
}

export const FulltextViewerModal: React.FC<FulltextViewerModalProps> = ({
  paper,
  isOpen,
  onClose,
  onSavePaper,
  isSaved = false,
  isFavorite = false,
  onToggleFavorite,
  onDownloadPdf,
  isDownloadingPdf = false,
  isDownloadedPdf = false,
}) => {
  const [copiedFormat, setCopiedFormat] = useState<string | null>(null);

  if (!isOpen || !paper) return null;

  const evaluation = evaluatePaper(paper);
  const sourceGroup = getSourceGroup(paper);
  const groupMeta = SOURCE_GROUPS[sourceGroup];
  const kind = getPaperKind(paper);

  const copyToClipboard = (text: string, formatName: string) => {
    navigator.clipboard.writeText(text).then(() => {
      setCopiedFormat(formatName);
      setTimeout(() => setCopiedFormat(null), 2000);
    });
  };

  const generateMarkdownSummary = () => {
    return [
      `# ${paper.title}`,
      '',
      `**Authors**: ${paper.authors.join(', ') || 'Unknown'}`,
      `**Year**: ${paper.year || 'N/A'} | **Venue**: ${paper.venue || 'N/A'} | **Source**: ${paper.source}`,
      paper.doi ? `**DOI**: [${paper.doi}](https://doi.org/${paper.doi})` : '',
      paper.citations !== undefined ? `**Citations**: ${paper.citations}` : '',
      `**Open Access**: ${paper.open_access ? 'Yes' : 'No'}`,
      paper.pdf_url ? `**PDF URL**: ${paper.pdf_url}` : '',
      '',
      '## Abstract',
      paper.abstract || '*(No abstract available)*',
      '',
      '## BibTeX',
      '```bibtex',
      bibtexCitation(paper),
      '```',
    ].filter(Boolean).join('\n');
  };

  return (
    <div className="modal-overlay" onClick={onClose} role="dialog" aria-modal="true">
      <div
        className="modal-dialog"
        onClick={(e) => e.stopPropagation()}
        style={{
          maxWidth: 860,
          width: '94vw',
          maxHeight: '90vh',
          display: 'flex',
          flexDirection: 'column',
          padding: 0,
          overflow: 'hidden',
        }}
      >
        {/* Modal Header */}
        <div
          className="modal-header"
          style={{
            padding: '16px 24px',
            borderBottom: '1px solid var(--cockpit-border)',
            background: 'var(--cockpit-card)',
          }}
        >
          <div style={{ display: 'flex', alignItems: 'center', gap: 10, flex: 1, minWidth: 0 }}>
            <BookOpen size={20} style={{ color: 'var(--primary-cyan)', flexShrink: 0 }} />
            <div style={{ minWidth: 0 }}>
              <div style={{ fontSize: 11, fontWeight: 600, color: 'var(--primary-cyan)', textTransform: 'uppercase', letterSpacing: '0.05em' }}>
                Fulltext & Paper Reader
              </div>
              <div
                style={{
                  fontSize: 15,
                  fontWeight: 700,
                  color: 'var(--text-main)',
                  whiteSpace: 'nowrap',
                  overflow: 'hidden',
                  textOverflow: 'ellipsis',
                }}
              >
                {paper.title}
              </div>
            </div>
          </div>
          <button className="modal-close-btn" onClick={onClose} aria-label="Close reader">
            <X size={18} />
          </button>
        </div>

        {/* Action Toolbar */}
        <div
          style={{
            padding: '10px 24px',
            background: 'var(--cockpit-card-hover)',
            borderBottom: '1px solid var(--cockpit-border)',
            display: 'flex',
            alignItems: 'center',
            justifyContent: 'space-between',
            flexWrap: 'wrap',
            gap: 8,
          }}
        >
          <div style={{ display: 'flex', gap: 6, flexWrap: 'wrap', alignItems: 'center' }}>
            {onToggleFavorite && (
              <button
                type="button"
                className={`action-btn ${isFavorite ? 'action-btn-primary' : ''}`}
                onClick={() => onToggleFavorite(paper)}
                style={{ fontSize: 12 }}
              >
                <Star size={13} fill={isFavorite ? '#f59e0b' : 'none'} color={isFavorite ? '#f59e0b' : undefined} />
                <span>{isFavorite ? 'Favourite' : 'Add to favourites'}</span>
              </button>
            )}

            {onSavePaper && (
              <button
                type="button"
                className={`action-btn ${isSaved ? 'action-btn-primary' : ''}`}
                onClick={() => onSavePaper(paper)}
                style={{ fontSize: 12 }}
              >
                <Bookmark size={13} fill={isSaved ? '#ffffff' : 'none'} />
                <span>{isSaved ? 'In interest list' : 'Add to interest list'}</span>
              </button>
            )}

            <button
              type="button"
              className="action-btn"
              onClick={() => copyToClipboard(apaCitation(paper), 'apa')}
              style={{ fontSize: 12 }}
            >
              {copiedFormat === 'apa' ? <Check size={13} color="var(--status-emerald)" /> : <Copy size={13} />}
              <span>{copiedFormat === 'apa' ? 'APA copied' : 'Copy APA'}</span>
            </button>

            <button
              type="button"
              className="action-btn"
              onClick={() => copyToClipboard(bibtexCitation(paper), 'bibtex')}
              style={{ fontSize: 12 }}
            >
              {copiedFormat === 'bibtex' ? <Check size={13} color="var(--status-emerald)" /> : <Copy size={13} />}
              <span>{copiedFormat === 'bibtex' ? 'BibTeX copied' : 'Copy BibTeX'}</span>
            </button>

            <button
              type="button"
              className="action-btn"
              onClick={() => copyToClipboard(generateMarkdownSummary(), 'markdown')}
              style={{ fontSize: 12 }}
            >
              {copiedFormat === 'markdown' ? <Check size={13} color="var(--status-emerald)" /> : <Share2 size={13} />}
              <span>{copiedFormat === 'markdown' ? 'Markdown copied' : 'Copy Markdown'}</span>
            </button>
          </div>

          <div style={{ display: 'flex', gap: 6, alignItems: 'center' }}>
            {(paper.source_url || paper.doi) && (
              <a
                className="action-btn"
                href={paper.source_url || `https://doi.org/${paper.doi}`}
                target="_blank"
                rel="noreferrer"
                style={{ textDecoration: 'none', fontSize: 12 }}
              >
                <ExternalLink size={13} />
                <span>View on {paper.source}</span>
              </a>
            )}

            {paper.pdf_url && onDownloadPdf && (
              <button
                type="button"
                className="action-btn action-btn-primary"
                onClick={() => onDownloadPdf(paper)}
                disabled={isDownloadingPdf || isDownloadedPdf}
                style={{ fontSize: 12 }}
              >
                {isDownloadedPdf ? <Check size={13} /> : <Download size={13} />}
                <span>
                  {isDownloadedPdf ? 'Downloaded' : isDownloadingPdf ? 'Downloading PDF…' : 'Download full PDF'}
                </span>
              </button>
            )}
          </div>
        </div>

        {/* Scrollable Content Body */}
        <div style={{ flex: 1, overflowY: 'auto', padding: '24px', display: 'flex', flexDirection: 'column', gap: 20 }}>
          {/* Metadata Header Box */}
          <div style={{ display: 'flex', flexDirection: 'column', gap: 10 }}>
            <div className="paper-badges" style={{ flexWrap: 'wrap' }}>
              <span className="badge badge-source badge-essential">{paper.source}</span>
              {kind !== 'article' && KIND_META[kind].badge && (
                <span className={`badge badge-essential ${KIND_META[kind].badge}`}>{KIND_META[kind].label}</span>
              )}
              <span className="badge badge-group" title={groupMeta.label}>
                {groupMeta.shortLabel}
              </span>
              {paper.open_access && <span className="badge badge-oa badge-essential">OPEN ACCESS</span>}
              {evaluation.recommendedPdf && (
                <span className="badge badge-pdf-recommended">
                  <FileCheck2 size={11} /> RECOMMENDED PDF {evaluation.pdfScore}
                </span>
              )}
              <span
                className={`badge evidence-${
                  evaluation.label === 'Recommended' ? 'strong' : evaluation.label === 'Consider' ? 'fair' : 'review'
                }`}
              >
                <Award size={11} /> SCREENING {evaluation.overall}/100
              </span>
              {paper.quartile && <span className="badge badge-q1">{paper.quartile}</span>}
            </div>

            <h2 style={{ fontSize: 20, fontWeight: 700, lineHeight: 1.35, color: 'var(--text-main)' }}>
              {paper.title}
            </h2>

            <div
              style={{
                display: 'grid',
                gridTemplateColumns: 'repeat(auto-fit, minmax(220px, 1fr))',
                gap: 12,
                padding: '14px 16px',
                background: 'var(--cockpit-card-hover)',
                borderRadius: 'var(--radius-md)',
                fontSize: 12.5,
              }}
            >
              <div style={{ display: 'flex', alignItems: 'flex-start', gap: 8 }}>
                <Users size={15} style={{ color: 'var(--primary-cyan)', marginTop: 2, flexShrink: 0 }} />
                <div>
                  <div style={{ fontWeight: 600, color: 'var(--text-dim)', fontSize: 11 }}>AUTHORS</div>
                  <div style={{ color: 'var(--text-main)' }}>{paper.authors.join(', ') || 'Unknown'}</div>
                </div>
              </div>

              <div style={{ display: 'flex', alignItems: 'flex-start', gap: 8 }}>
                <Building size={15} style={{ color: 'var(--primary-cyan)', marginTop: 2, flexShrink: 0 }} />
                <div>
                  <div style={{ fontWeight: 600, color: 'var(--text-dim)', fontSize: 11 }}>JOURNAL / VENUE</div>
                  <div style={{ color: 'var(--text-main)', fontStyle: 'italic' }}>{paper.venue || 'N/A'}</div>
                </div>
              </div>

              <div style={{ display: 'flex', alignItems: 'flex-start', gap: 8 }}>
                <Calendar size={15} style={{ color: 'var(--primary-cyan)', marginTop: 2, flexShrink: 0 }} />
                <div>
                  <div style={{ fontWeight: 600, color: 'var(--text-dim)', fontSize: 11 }}>PUBLICATION YEAR & CITATIONS</div>
                  <div style={{ color: 'var(--text-main)' }}>
                    {paper.year || 'N/A'} • Citations: <b>{paper.citations ?? 'N/A'}</b>
                  </div>
                </div>
              </div>

              {paper.doi && (
                <div style={{ display: 'flex', alignItems: 'flex-start', gap: 8 }}>
                  <Hash size={15} style={{ color: 'var(--primary-cyan)', marginTop: 2, flexShrink: 0 }} />
                  <div>
                    <div style={{ fontWeight: 600, color: 'var(--text-dim)', fontSize: 11 }}>DOI / IDENTIFIER</div>
                    <div>
                      <a
                        href={`https://doi.org/${paper.doi}`}
                        target="_blank"
                        rel="noreferrer"
                        style={{ color: 'var(--primary-cyan)', textDecoration: 'none' }}
                      >
                        {paper.doi}
                      </a>
                    </div>
                  </div>
                </div>
              )}
            </div>
          </div>

          {/* Abstract / Full Text Section */}
          <div style={{ display: 'flex', flexDirection: 'column', gap: 8 }}>
            <div style={{ display: 'flex', alignItems: 'center', gap: 6, fontSize: 14, fontWeight: 700, color: 'var(--text-main)' }}>
              <FileText size={16} style={{ color: 'var(--primary-cyan)' }} />
              <span>Abstract & full-text brief</span>
            </div>

            <div
              style={{
                lineHeight: 1.7,
                fontSize: 14,
                color: 'var(--text-main)',
                background: 'var(--cockpit-card)',
                border: '1px solid var(--cockpit-border)',
                borderRadius: 'var(--radius-md)',
                padding: '18px 20px',
                textAlign: 'justify',
              }}
            >
              {paper.abstract ? (
                paper.abstract.split('\n\n').map((para, i) => (
                  <p key={i} style={{ marginBottom: 12 }}>
                    {para}
                  </p>
                ))
              ) : (
                <div style={{ color: 'var(--text-dim)', fontStyle: 'italic' }}>
                  No abstract was found in the metadata. You can open the original record on{' '}
                  <b>{paper.source}</b> or download the PDF to read the full text.
                </div>
              )}
            </div>
          </div>

          {/* Transparent Screening Breakdown */}
          <div
            style={{
              padding: '14px 16px',
              borderRadius: 'var(--radius-md)',
              border: '1px solid var(--cockpit-border)',
              background: 'var(--cockpit-card-hover)',
              fontSize: 12,
            }}
          >
            <div style={{ fontWeight: 600, marginBottom: 8, color: 'var(--text-main)' }}>
              Preliminary screening breakdown
            </div>
            <div
              style={{
                display: 'grid',
                gridTemplateColumns: 'repeat(auto-fit, minmax(130px, 1fr))',
                gap: 8,
                marginBottom: 8,
              }}
            >
              <div>Relevance: <b>{evaluation.relevance}</b></div>
              <div>Metadata completeness: <b>{evaluation.metadata}</b></div>
              <div>Recency: <b>{evaluation.recency}</b></div>
              <div>Citations: <b>{evaluation.citation}</b></div>
              <div>Access: <b>{evaluation.access}</b></div>
            </div>
            <div style={{ color: 'var(--text-dim)', fontSize: 11 }}>
              Screening score = 40% RRF rank + 40% metadata completeness + 20% access availability.
            </div>
          </div>
        </div>

        {/* Modal Footer */}
        <div
          className="modal-footer"
          style={{
            padding: '12px 24px',
            borderTop: '1px solid var(--cockpit-border)',
            display: 'flex',
            justifyContent: 'flex-end',
          }}
        >
          <button type="button" className="action-btn" onClick={onClose}>
            Close
          </button>
        </div>
      </div>
    </div>
  );
};
