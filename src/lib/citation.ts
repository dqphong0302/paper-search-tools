import { Paper } from '../types';
import { getPaperKind } from './paperKind';

const clean = (value?: string) => (value || '').replace(/\s+/g, ' ').trim();

const bibtexValue = (value: string) =>
  value.replace(/[{}]/g, '').replace(/[\r\n]+/g, ' ').trim();

/** "AI", "J.T.", "TB" — a run of initials rather than a name. */
const looksLikeInitials = (token: string) => /^(?:[A-Z]\.?){1,4}$/.test(token);

/**
 * Splits an author into family and given names.
 *
 * Three shapes reach us and they disagree about order:
 *   "Smith, John"  — explicit, comma-separated
 *   "John Smith"   — given first, family last
 *   "Bhat AI"      — family first, then initials (what PubMed and Europe PMC emit)
 *
 * Taking the last token as the family name is right for the second and wrong
 * for the third, which inverted every PubMed author: "Bhat AI" became given
 * "Bhat", family "AI". Trailing initials are the signal that tells them apart.
 */
const familyGiven = (author: string) => {
  const value = clean(author);
  if (!value) return { family: '', given: '' };
  if (value.includes(',')) {
    const [family, given] = value.split(',');
    return { family: clean(family), given: clean(given) };
  }
  const parts = value.split(' ');
  if (parts.length > 1 && looksLikeInitials(parts[parts.length - 1])) {
    const given = parts.pop() || '';
    return { family: clean(parts.join(' ')), given: clean(given) };
  }
  const family = parts.pop() || '';
  return { family: clean(family), given: clean(parts.join(' ')) };
};

/**
 * Given names as letters.
 *
 * A token that is already a run of initials ("AI", "J.T.") carries one letter
 * per name, so it expands to all of them; anything else is a written-out name
 * and contributes only its first letter.
 */
const initialLetters = (given: string): string[] =>
  given
    .split(/\s+/)
    .filter(Boolean)
    .flatMap((part) =>
      looksLikeInitials(part)
        ? part.replace(/\./g, '').split('')
        : [part[0].toUpperCase()]
    );

const initials = (given: string) =>
  initialLetters(given)
    .map((letter) => `${letter}.`)
    .join(' ');

// Vancouver omits periods and spaces between initials (e.g. "TB").
const vancouverInitials = (given: string) => initialLetters(given).join('');

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

/** RIS wants "Family, Given"; a raw "John Smith" is read as a single-field name. */
const risAuthor = (author: string): string => {
  const { family, given } = familyGiven(author);
  if (!family) return bibtexValue(clean(author));
  return bibtexValue(given ? `${family}, ${given}` : family);
};

/** Non-article records (datasets, reports, …) must not be imported as journal articles. */
const RIS_TYPE: Record<string, string> = { dataset: 'DATA', report: 'RPRT', software: 'COMP' };
const BIBTEX_TYPE: Record<string, string> = { article: 'article', report: 'techreport' };

export function bibtexCitation(paper: Paper, key = bibtexKey(paper)): string {
  const kind = getPaperKind(paper);
  const type = BIBTEX_TYPE[kind] ?? 'misc';
  const lines = [`@${type}{${key},`];
  lines.push(`  title = {${bibtexValue(clean(paper.title))}},`);
  if (paper.authors.length) {
    // "Family, Given" so BibTeX readers don't swap PubMed-style "Bhat AI".
    lines.push(`  author = {${paper.authors.map(risAuthor).join(' and ')}},`);
  }
  if (paper.year) lines.push(`  year = {${paper.year}},`);
  if (paper.venue) {
    const field = type === 'article' ? 'journal' : type === 'techreport' ? 'institution' : 'howpublished';
    lines.push(`  ${field} = {${bibtexValue(clean(paper.venue))}},`);
  }
  if (paper.doi) lines.push(`  doi = {${paper.doi}},`);
  if (paper.source_url) lines.push(`  url = {${paper.source_url}},`);
  lines.push('}');
  return lines.join('\n');
}

/**
 * One RIS record.
 *
 * RIS is the shared interchange format of Zotero, EndNote and Mendeley, so this
 * is the export all three read. `T2` carries the journal title: Zotero, EndNote
 * and Mendeley all map it to the publication, while the older `JO` tag is not
 * handled consistently across them.
 */
export function risEntry(paper: Paper): string {
  const lines = [`TY  - ${RIS_TYPE[getPaperKind(paper)] ?? 'JOUR'}`];
  lines.push(`TI  - ${bibtexValue(clean(paper.title))}`);
  for (const author of paper.authors) {
    lines.push(`AU  - ${risAuthor(author)}`);
  }
  if (paper.year) lines.push(`PY  - ${paper.year}`);
  if (paper.venue) lines.push(`T2  - ${bibtexValue(clean(paper.venue))}`);
  if (paper.doi) lines.push(`DO  - ${paper.doi}`);
  if (paper.abstract) lines.push(`AB  - ${bibtexValue(paper.abstract)}`);
  if (paper.source_url) lines.push(`UR  - ${paper.source_url}`);
  if (paper.pdf_url) lines.push(`L1  - ${paper.pdf_url}`);
  lines.push('ER  - ');
  return lines.join('\n');
}

export const risLibrary = (papers: Paper[]) =>
  papers.map(risEntry).join('\n\n').concat(papers.length ? '\n' : '');

/** Citation keys must be unique within a .bib file, so repeats get a numeric suffix. */
export const bibtexLibrary = (papers: Paper[]) => {
  const seen = new Map<string, number>();
  return papers
    .map((paper) => {
      const base = bibtexKey(paper);
      const count = seen.get(base) ?? 0;
      seen.set(base, count + 1);
      return bibtexCitation(paper, count ? `${base}${count + 1}` : base);
    })
    .join('\n\n')
    .concat(papers.length ? '\n' : '');
};
