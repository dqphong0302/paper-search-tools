import React, { useMemo, useState } from 'react';
import { ChevronDown, ChevronUp, Lightbulb, TrendingUp } from 'lucide-react';
import { Paper } from '../types';
import { analyzeLandscape } from '../lib/landscape';

interface ResearchGapPanelProps {
  query: string;
  papers: Paper[];
}

export function auditResearchSample(papers: Paper[], currentYear = new Date().getFullYear()) {
  const dated = papers.filter((paper) => Number.isInteger(paper.year) && paper.year! >= 1000 && paper.year! <= currentYear);
  const recent = dated.filter((paper) => paper.year! >= currentYear - 1).length;
  const openAccess = papers.filter((paper) => paper.open_access === true).length;
  const abstracts = papers.filter((paper) => paper.abstract?.trim()).length;
  const insights = papers.length === 0 ? ['No papers in current sample to analyze. An empty result set does not prove an absence of research.'] : [
    `${recent}/${dated.length} papers with a valid year fall into ${currentYear - 1}–${currentYear}; ${papers.length - dated.length} papers lack a valid year. Verify year filters, limits, and source errors before assessing recency.`,
    `${openAccess}/${papers.length} papers are marked Open Access by their source; ${abstracts}/${papers.length} papers include an abstract. A PDF link alone does not prove open access rights; missing metadata does not imply low quality.`,
    'No structured data yet for methodology, sample size, or conflicting results. Read primary papers and cross-examine multiple databases before proposing research gaps.',
  ];
  return { recent, dated: dated.length, openAccess, abstracts, insights };
}

export const ResearchGapPanel: React.FC<ResearchGapPanelProps> = ({ query, papers }) => {
  // Secondary analysis: opened on demand so the result list stays the first thing read.
  const [open, setOpen] = useState(false);

  const analysis = useMemo(
    () => ({ ...auditResearchSample(papers), landscape: analyzeLandscape(papers, query) }),
    [papers, query]
  );

  return (
    <section className="gap-panel" aria-labelledby="research-gap-title">
      <div className="gap-panel-header">
        <div>
          <div className="gap-eyebrow"><TrendingUp size={13} /> SAMPLE METADATA • DETERMINISTIC</div>
          <h2 id="research-gap-title">Sample Audit &amp; Landscape Analysis</h2>
          <p>Descriptive statistics over the current search sample. Non-LLM analysis.</p>
        </div>
        <div className="gap-panel-actions">
          <button
            id="toggle-landscape-panel"
            type="button"
            className="action-btn"
            onClick={() => setOpen((value) => !value)}
            aria-expanded={open}
            aria-controls="research-gap-body"
          >
            {open ? <ChevronUp size={14} /> : <ChevronDown size={14} />}
            <span>{open ? 'Collapse' : 'Expand'}</span>
          </button>
        </div>
      </div>

      {open && (
        <div id="research-gap-body">
          <div className="gap-insights">
            <div className="gap-metric-row">
              <div><span>Sample Papers</span><strong>{papers.length}</strong></div>
              <div><span>Recent (1–2 yrs)</span><strong>{analysis.recent}</strong></div>
              <div><span>Open Access</span><strong>{analysis.openAccess}/{papers.length}</strong></div>
            </div>
            {analysis.insights.map((insight) => (
              <div className="gap-insight" key={insight}><Lightbulb size={15} /><span>{insight}</span></div>
            ))}
          </div>

          {analysis.landscape.total > 0 && (
            <div className="gap-subpanel">
              <div className="gap-eyebrow" style={{ marginBottom: 10 }}>
                <TrendingUp size={13} /> RESULT SET LANDSCAPE (Descriptive only, not evidence of field gaps)
              </div>

              <div className="gap-metric-row gap-metric-row-4" style={{ marginBottom: 12 }}>
                <div><span>Reporting Sources</span><strong>{analysis.landscape.sources.length}</strong></div>
                <div><span>Distinct Venues</span><strong>{analysis.landscape.venues.length}</strong></div>
                <div><span>Distinct Authors</span><strong>{analysis.landscape.distinctAuthors}</strong></div>
                <div><span>Open Access</span><strong>{analysis.landscape.openAccess}/{analysis.landscape.total}</strong></div>
              </div>

              <div style={{ display: 'flex', alignItems: 'flex-end', gap: 4, height: 64, marginBottom: 6 }}>
                {analysis.landscape.years.map((bucket) => {
                  const max = Math.max(...analysis.landscape.years.map((item) => item.count), 1);
                  return (
                    <div key={bucket.year} title={`${bucket.year}: ${bucket.count} papers`} style={{ flex: 1, display: 'flex', flexDirection: 'column', alignItems: 'center', gap: 2 }}>
                      <div style={{ width: '100%', height: `${(bucket.count / max) * 46}px`, minHeight: bucket.count ? 2 : 1, background: bucket.count ? 'var(--primary-cyan)' : 'rgba(255,255,255,0.12)', borderRadius: 3 }} />
                      <span style={{ fontSize: 9, color: '#8fa8c1' }}>{String(bucket.year).slice(2)}</span>
                    </div>
                  );
                })}
              </div>

              {analysis.landscape.topTerms.length > 0 && (
                <div style={{ display: 'flex', flexWrap: 'wrap', gap: 6, marginBottom: 10 }}>
                  <span style={{ fontSize: 11, color: '#8fa8c1' }}>Top Terms:</span>
                  {analysis.landscape.topTerms.map((term) => (
                    <span key={term.key} className="cockpit-badge badge-cyan" style={{ fontSize: 10 }}>
                      {term.key} · {term.count}
                    </span>
                  ))}
                </div>
              )}

              {analysis.landscape.queryCoverage.length > 0 && (
                <p style={{ fontSize: 12, margin: 0 }}>
                  Query term coverage:{' '}
                  {analysis.landscape.queryCoverage
                    .map((entry) => `${entry.term} (${entry.count})`)
                    .join(', ')}
                  . Terms with 0 papers indicate opportunities for targeted reading, not absence of research.
                </p>
              )}
            </div>
          )}
        </div>
      )}
    </section>
  );
};
