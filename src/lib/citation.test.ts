import { describe, expect, it } from 'vitest';
import { Paper } from '../types';
import {
  apaCitation,
  bibtexCitation,
  bibtexKey,
  risEntry,
  risLibrary,
  vancouverCitation,
} from './citation';

const paper: Paper = {
  id: 'doi:10.1000/xyz',
  title: 'Deep learning for health',
  authors: ['John Smith', 'Tran, Thi B'],
  year: 2024,
  venue: 'Journal of AI',
  doi: '10.1000/xyz',
  source_url: 'https://doi.org/10.1000/xyz',
  source: 'OpenAlex',
  open_access: true,
};

describe('citation formats', () => {
  it('builds an APA citation', () => {
    expect(apaCitation(paper)).toBe(
      'Smith, J., Tran, T. B. (2024). Deep learning for health. Journal of AI. https://doi.org/10.1000/xyz'
    );
  });

  it('builds a Vancouver citation', () => {
    expect(vancouverCitation(paper)).toBe(
      'Smith J, Tran TB. Deep learning for health. Journal of AI. 2024. doi:10.1000/xyz'
    );
  });

  it('builds a BibTeX entry with a stable key', () => {
    const entry = bibtexCitation(paper);
    expect(entry).toContain('@article{Smith2024Deep,');
    expect(entry).toContain('author = {John Smith and Tran, Thi B},');
    expect(entry).toContain('doi = {10.1000/xyz},');
    expect(bibtexKey(paper)).toBe('Smith2024Deep');
  });

  it('keeps RIS line-based even with newlines in metadata', () => {
    const risky: Paper = { ...paper, title: 'Line one\nER  - injected', abstract: 'a\r\nb' };
    const entry = risEntry(risky);
    expect(entry).not.toContain('\nER  - injected');
    expect(entry.split('\n').filter((line) => line.startsWith('ER'))).toHaveLength(1);
    expect(entry).toContain('AB  - a b');
  });

  // RIS is what Zotero, EndNote and Mendeley all import, and all three parse
  // `AU` as "Family, Given". Emitting the raw display name made every
  // single-word-surname author import wrong.
  it('writes RIS authors as Family, Given regardless of how the source spells them', () => {
    const entry = risEntry(paper);
    expect(entry).toContain('AU  - Smith, John');
    expect(entry).toContain('AU  - Tran, Thi B');
    expect(entry).not.toContain('AU  - John Smith');
  });

  it('carries the journal in T2, which all three reference managers map', () => {
    const entry = risEntry(paper);
    expect(entry).toContain('T2  - Journal of AI');
    expect(entry).not.toContain('JO  -');
  });

  // PubMed and Europe PMC emit "Family Initials", the opposite order to a
  // display name. Reading the last token as the family name inverted every one
  // of them, in RIS and in the copied APA/Vancouver citations alike.
  it('reads PubMed-style "Family Initials" authors in the right order', () => {
    const pubmed: Paper = { ...paper, authors: ['Bhat AI', 'Greeshma M'] };
    const entry = risEntry(pubmed);
    expect(entry).toContain('AU  - Bhat, AI');
    expect(entry).toContain('AU  - Greeshma, M');
    expect(apaCitation(pubmed)).toContain('Bhat, A. I.');
    expect(vancouverCitation(pubmed)).toContain('Bhat AI');
  });

  it('keeps a single-name author usable', () => {
    const entry = risEntry({ ...paper, authors: ['Plato'] });
    expect(entry).toContain('AU  - Plato');
    expect(entry).not.toContain('AU  - Plato,');
  });

  it('joins a library export and terminates with a newline', () => {
    const output = risLibrary([paper, { ...paper, id: 'p2' }]);
    expect(output.match(/TY {2}- JOUR/g)).toHaveLength(2);
    expect(output.endsWith('\n')).toBe(true);
  });
});
