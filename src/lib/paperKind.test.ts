import { describe, expect, it } from 'vitest';
import { Paper } from '../types';
import { getPaperKind, isArticle } from './paperKind';

const paper = (source: string): Paper => ({
  id: 'x',
  title: 'T',
  authors: [],
  source,
  open_access: false,
});

describe('paperKind', () => {
  it('classifies non-article outputs and defaults to article', () => {
    expect(getPaperKind(paper('Dryad'))).toBe('dataset');
    expect(getPaperKind(paper('Figshare'))).toBe('dataset');
    expect(getPaperKind(paper('NTRS'))).toBe('report');
    expect(getPaperKind(paper('NVD CVE'))).toBe('advisory');
    expect(getPaperKind(paper('CISA KEV'))).toBe('advisory');
    expect(getPaperKind(paper('StackExchange'))).toBe('discussion');
    expect(getPaperKind(paper('OpenFDA'))).toBe('record');
    expect(getPaperKind(paper('UniProt'))).toBe('record');
    expect(getPaperKind(paper('OpenAlex'))).toBe('article');
    expect(isArticle(paper('OpenAlex'))).toBe(true);
    expect(isArticle(paper('Dataverse'))).toBe(false);
  });
});
