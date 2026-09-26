import { describe, expect, it } from 'vitest';
import { Paper } from '../types';
import { analyzeLandscape, tokenize } from './landscape';

const paper = (overrides: Partial<Paper>): Paper => ({
  id: Math.random().toString(36).slice(2),
  title: 'Title',
  authors: ['A'],
  source: 'OpenAlex',
  open_access: false,
  ...overrides,
});

describe('tokenize', () => {
  it('drops stopwords, punctuation and short tokens', () => {
    expect(tokenize('The Role of RNA in cancer, and of AI')).toEqual(['role', 'rna', 'cancer']);
  });
});

describe('analyzeLandscape', () => {
  it('counts sources, coverage and years honestly', () => {
    const year = new Date().getFullYear();
    const landscape = analyzeLandscape(
      [
        paper({ title: 'Deep learning cancer', abstract: 'cancer', source: 'OpenAlex', year, open_access: true }),
        paper({ title: 'Cancer therapy', source: 'PubMed', year }),
        paper({ title: 'Unrelated', source: 'PubMed' }),
      ],
      'cancer therapy'
    );

    expect(landscape.total).toBe(3);
    expect(landscape.sources.find((s) => s.key === 'PubMed')?.count).toBe(2);
    expect(landscape.years.find((b) => b.year === year)?.count).toBe(2);
    expect(landscape.openAccess).toBe(1);
    // "cancer" appears in 2 papers, "therapy" in 1, and missing terms are explicit.
    expect(landscape.queryCoverage.find((q) => q.term === 'cancer')?.count).toBe(2);
    expect(landscape.queryCoverage.find((q) => q.term === 'therapy')?.count).toBe(1);
  });

  it('ignores generic scholarly words and counts a term once per paper', () => {
    const landscape = analyzeLandscape([
      paper({ title: 'Paper version results', abstract: 'CRISPR CRISPR CRISPR methods 2024' }),
      paper({ title: 'Base editing', abstract: 'editing of hemoglobin' }),
      paper({ title: 'CRISPR screening' }),
    ], '');
    const terms = Object.fromEntries(landscape.topTerms.map((t) => [t.key, t.count]));
    expect(terms.crispr).toBe(2);
    expect(terms.editing).toBe(1);
    for (const noise of ['paper', 'version', 'results', 'methods', '2024']) expect(terms[noise]).toBeUndefined();
  });
});
