import { Paper } from '../types';
import { getPaperKind } from './paperKind';

const clean = (value?: string) => (value || '').replace(/\s+/g, ' ').trim();

/** "123-130" / "123–130" → ["123", "130"]; a single page or article number stays alone. */
const pageRange = (pages?: string): [string, string?] | null => {
  const value = clean(pages);
  if (!value) return null;
  const [start, end] = value.split(/\s*[-–—]+\s*/);
  return end ? [start, end] : [start];
};

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
  const { volume, issue, pages } = paper.biblio ?? {};
  const locator = [
    volume ? `${clean(volume)}${issue ? `(${clean(issue)})` : ''}` : '',
    pages ? clean(pages).replace(/\s*-+\s*/, '–') : '',
  ]
    .filter(Boolean)
    .join(', ');
  const venue = paper.venue ? ` ${clean(paper.venue)}${locator ? `, ${locator}` : ''}.` : '';
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
  const { volume, issue, pages } = paper.biblio ?? {};
  const year = paper.year
    ? ` ${paper.year}${volume ? `;${clean(volume)}` : ''}${volume && issue ? `(${clean(issue)})` : ''}${pages ? `:${clean(pages)}` : ''}`
    : '';
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
  const biblio = paper.biblio ?? {};
  if (biblio.volume) lines.push(`  volume = {${bibtexValue(biblio.volume)}},`);
  if (biblio.issue) lines.push(`  number = {${bibtexValue(biblio.issue)}},`);
  const pages = pageRange(biblio.pages);
  if (pages) lines.push(`  pages = {${bibtexValue(pages.filter(Boolean).join('--'))}},`);
  if (biblio.publisher) lines.push(`  publisher = {${bibtexValue(biblio.publisher)}},`);
  if (biblio.issn) lines.push(`  issn = {${bibtexValue(biblio.issn)}},`);
  if (biblio.keywords?.length) lines.push(`  keywords = {${bibtexValue(biblio.keywords.join(', '))}},`);
  if (paper.doi) lines.push(`  doi = {${paper.doi}},`);
  if (paper.source_url) lines.push(`  url = {${paper.source_url}},`);
  if (paper.abstract) lines.push(`  abstract = {${bibtexValue(paper.abstract)}},`);
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
  const biblio = paper.biblio ?? {};
  if (biblio.volume) lines.push(`VL  - ${bibtexValue(biblio.volume)}`);
  if (biblio.issue) lines.push(`IS  - ${bibtexValue(biblio.issue)}`);
  const pages = pageRange(biblio.pages);
  if (pages) {
    lines.push(`SP  - ${bibtexValue(pages[0])}`);
    if (pages[1]) lines.push(`EP  - ${bibtexValue(pages[1])}`);
  }
  if (biblio.issn) lines.push(`SN  - ${bibtexValue(biblio.issn)}`);
  if (biblio.publisher) lines.push(`PB  - ${bibtexValue(biblio.publisher)}`);
  for (const keyword of biblio.keywords ?? []) lines.push(`KW  - ${bibtexValue(keyword)}`);
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
