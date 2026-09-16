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

  it('joins a library export and terminates with a newline', () => {
    const output = risLibrary([paper, { ...paper, id: 'p2' }]);
    expect(output.match(/TY {2}- JOUR/g)).toHaveLength(2);
    expect(output.endsWith('\n')).toBe(true);
  });
});
