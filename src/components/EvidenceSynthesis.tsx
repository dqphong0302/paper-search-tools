import React, { useMemo, useState } from 'react';
import { ChevronDown, ChevronUp, FileText, Sparkles } from 'lucide-react';
import { Paper } from '../types';
import { evaluatePaper, getSourceGroup, SOURCE_GROUPS } from '../lib/paperEvaluation';

interface EvidenceSynthesisProps {
  query: string;
  papers: Paper[];
}

const normalize = (value: string) => value.trim().replace(/\s+/g, ' ');

const EXCERPT_MAX = 320;

/**
 * First sentence of an abstract. Many scraped abstracts run sentences together
 * ("…manifestations.BCL11A is…"), so the sentence split can swallow the whole
 * abstract — clamp it rather than dumping a wall of text into the panel.
 */
const firstSentence = (abstract: string) => {
  const text = normalize(abstract).split(/(?<=[.!?])\s+/)[0] ?? '';
  return text.length > EXCERPT_MAX ? `${text.slice(0, EXCERPT_MAX).trimEnd()}…` : text;
};

export const EvidenceSynthesis: React.FC<EvidenceSynthesisProps> = ({ query, papers }) => {
  // Secondary analysis: closed by default so the result list stays above the fold.
  const [expanded, setExpanded] = useState(false);
  const synthesis = useMemo(() => {
    const ranked = [...papers]
      .map((paper) => ({ paper, evaluation: evaluatePaper(paper) }))
      .sort((a, b) => b.evaluation.overall - a.evaluation.overall);
    const evidence = ranked.filter(({ paper }) => paper.abstract || paper.doi).slice(0, 5);
    const recentYear = new Date().getFullYear() - 2;
    const recent = papers.filter((paper) => (paper.year || 0) >= recentYear).length;
    const groups = [...new Set(papers.map(getSourceGroup))];
    const withPdf = papers.filter((paper) => paper.pdf_url).length;
    const leadingClaims = evidence
      .map(({ paper }, index) => ({ index: index + 1, text: firstSentence(paper.abstract || '') }))
      .filter((claim) => claim.text.length >= 40)
      .slice(0, 2);

    return {
      evidence,
      excerpts: leadingClaims,
      coverage: `${papers.length} documents across ${groups.length} source groups; ${recent} published within the last 2 years; ${withPdf} records provide full-text links.`,
      caution:
        groups.length < 2
          ? 'Source coverage is narrow; consider expanding search across multiple database groups before drawing conclusions.'
          : 'Deterministic metadata synthesis based on returned abstracts. Does not replace full-text critical appraisal or risk-of-bias evaluation.',
    };
  }, [papers, query]);

  if (!papers.length) return null;

  return (
    <section className="synthesis-panel" aria-labelledby="evidence-synthesis-title">
      <div className="synthesis-header">
        <div>
          <div className="gap-eyebrow"><Sparkles size={13} /> METADATA AUDIT · DETERMINISTIC / NON-LLM</div>
          <h2 id="evidence-synthesis-title">Document Overview & Abstract Excerpts</h2>
        </div>
        <button id="toggle-evidence-synthesis" className="action-btn" type="button" onClick={() => setExpanded((value) => !value)}>
          {expanded ? <ChevronUp size={14} /> : <ChevronDown size={14} />}
          {expanded ? 'Collapse' : 'Expand overview'}
        </button>
      </div>
      {expanded && (
        <div className="synthesis-body">
          {/* One wrapper per grid column: as bare children these blocks were
              auto-placed across both columns and the excerpts read out of order. */}
          <div>
            <p className="synthesis-summary">Verbatim first sentence of abstracts (when available). Not an AI-generated conclusion or automated clinical recommendation.</p>
            {synthesis.excerpts.map((excerpt) => <blockquote key={excerpt.index}>[{excerpt.index}] {excerpt.text}</blockquote>)}
            {!synthesis.excerpts.length && <p>No abstracts available to extract excerpt findings for “{query}”.</p>}
            <div className="synthesis-coverage">{synthesis.coverage}</div>
            <div className="synthesis-caution">{synthesis.caution}</div>
          </div>
          <div className="synthesis-evidence" aria-label="Evidence documents for synthesis">
            {synthesis.evidence.map(({ paper, evaluation }, index) => (
              <a key={paper.id} href={paper.doi ? `https://doi.org/${paper.doi}` : paper.pdf_url} target="_blank" rel="noreferrer">
                <FileText size={13} />
                <span>[{index + 1}] {paper.title}</span>
                <b>{SOURCE_GROUPS[getSourceGroup(paper)].shortLabel} · {evaluation.overall}/100</b>
              </a>
            ))}
          </div>
        </div>
      )}
    </section>
  );
};
