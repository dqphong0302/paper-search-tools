import { Paper } from '../types';

const clean = (value?: string) => (value || '').replace(/\s+/g, ' ').trim();

const bibtexValue = (value: string) =>
  value.replace(/[{}]/g, '').replace(/[\r\n]+/g, ' ').trim();

// "Smith, J." from "John Smith" / "Smith, John" / full names.
const familyGiven = (author: string) => {
  const value = clean(author);
  if (!value) return { family: '', given: '' };
  if (value.includes(',')) {
    const [family, given] = value.split(',');
    return { family: clean(family), given: clean(given) };
  }
  const parts = value.split(' ');
  const family = parts.pop() || '';
  return { family: clean(family), given: clean(parts.join(' ')) };
};

const initials = (given: string) =>
  given
    .split(/\s+/)
    .filter(Boolean)
    .map((part) => `${part[0].toUpperCase()}.`)
    .join(' ');

// Vancouver omits periods and spaces between initials (e.g. "TB").
const vancouverInitials = (given: string) =>
  given
    .split(/\s+/)
    .filter(Boolean)
    .map((part) => part[0].toUpperCase())
    .join('');

export function apaCitation(paper: Paper): string {
  const authors = paper.authors.slice(0, 20).map((a) => {
    const { family, given } = familyGiven(a);
    return `${family}, ${initials(given)}`.trim().replace(/,\s*$/, '');
  });
  const authorText = authors.length
    ? `${authors.slice(0, 3).join(', ')}${authors.length > 3 ? ', et al.' : ''}`
    : 'N.d.';
  const year = paper.year ? `(${paper.year}).` : '(n.d.).';
  const venue = paper.venue ? ` ${clean(paper.venue)}.` : '';
  const doi = paper.doi ? ` https://doi.org/${paper.doi}` : paper.source_url ? ` ${paper.source_url}` : '';
  return `${authorText} ${year} ${clean(paper.title)}.${venue}${doi}`.replace(/\s+/g, ' ').trim();
}

export function vancouverCitation(paper: Paper): string {
  const authors = paper.authors.slice(0, 6).map((a) => {
    const { family, given } = familyGiven(a);
    return `${family} ${vancouverInitials(given)}`.trim();
  });
  const authorText = authors.length
    ? `${authors.join(', ')}${paper.authors.length > 6 ? ', et al' : ''}.`
    : '';
  const venue = paper.venue ? ` ${clean(paper.venue)}.` : '';
  const year = paper.year ? ` ${paper.year}` : '';
  const locator = paper.doi ? ` doi:${paper.doi}` : paper.source_url ? ` ${paper.source_url}` : '';
  return `${authorText} ${clean(paper.title)}.${venue}${year}.${locator}`.replace(/\s+/g, ' ').trim();
}

export function bibtexKey(paper: Paper): string {
  const first = paper.authors[0] ? familyGiven(paper.authors[0]).family : 'paper';
  const word = clean(paper.title).split(/\s+/)[0] || 'untitled';
  const slug = `${first}${paper.year ?? ''}${word}`.replace(/[^a-zA-Z0-9]/g, '');
  return slug || 'paper';
}

export function bibtexCitation(paper: Paper, key = bibtexKey(paper)): string {
  const lines = [`@article{${key},`];
  lines.push(`  title = {${bibtexValue(clean(paper.title))}},`);
  if (paper.authors.length) {
    lines.push(`  author = {${bibtexValue(paper.authors.join(' and '))}},`);
  }
  if (paper.year) lines.push(`  year = {${paper.year}},`);
  if (paper.venue) lines.push(`  journal = {${bibtexValue(clean(paper.venue))}},`);
  if (paper.doi) lines.push(`  doi = {${paper.doi}},`);
  if (paper.source_url) lines.push(`  url = {${paper.source_url}},`);
  lines.push('}');
  return lines.join('\n');
}

export function risEntry(paper: Paper): string {
  const lines = ['TY  - JOUR'];
  lines.push(`TI  - ${bibtexValue(clean(paper.title))}`);
  for (const author of paper.authors) {
    lines.push(`AU  - ${bibtexValue(clean(author))}`);
  }
  if (paper.year) lines.push(`PY  - ${paper.year}`);
  if (paper.venue) lines.push(`JO  - ${bibtexValue(clean(paper.venue))}`);
  if (paper.doi) lines.push(`DO  - ${paper.doi}`);
  if (paper.abstract) lines.push(`AB  - ${bibtexValue(paper.abstract)}`);
  if (paper.source_url) lines.push(`UR  - ${paper.source_url}`);
  if (paper.pdf_url) lines.push(`L1  - ${paper.pdf_url}`);
  lines.push('ER  - ');
  return lines.join('\n');
}

export const risLibrary = (papers: Paper[]) =>
  papers.map(risEntry).join('\n\n').concat(papers.length ? '\n' : '');

export const bibtexLibrary = (papers: Paper[]) =>
  papers.map((paper) => bibtexCitation(paper)).join('\n\n').concat(papers.length ? '\n' : '');
