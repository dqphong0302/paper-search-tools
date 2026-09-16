import { describe, expect, it } from 'vitest';
import { Paper } from '../types';
import { evaluatePaper, getSourceGroup } from './paperEvaluation';

const base: Paper = {
  id: 'p1',
  title: 'A study',
  authors: ['A Author'],
  year: new Date().getFullYear(),
  venue: 'Journal',
  abstract: 'text',
  doi: '10.1/x',
  source: 'OpenAlex',
  open_access: true,
};

describe('evaluatePaper', () => {
  it('scores within bounds and flags a direct PDF as recommended', () => {
    const result = evaluatePaper({ ...base, pdf_url: 'https://example.org/a.pdf' });
    for (const value of [result.overall, result.relevance, result.metadata, result.access, result.pdfScore]) {
      expect(value).toBeGreaterThanOrEqual(0);
      expect(value).toBeLessThanOrEqual(100);
    }
    expect(result.recommendedPdf).toBe(true);
  });

  it('does not recommend a PDF when none is linked', () => {
    expect(evaluatePaper({ ...base, pdf_url: undefined }).recommendedPdf).toBe(false);
  });

  it('maps sources to discipline-neutral groups', () => {
    expect(getSourceGroup({ ...base, source: 'PubMed' })).toBe('biomedical');
    expect(getSourceGroup({ ...base, source: 'arXiv' })).toBe('preprint');
    expect(getSourceGroup({ ...base, source: 'OpenAlex Việt Nam' })).toBe('vietnam');
  });
});
