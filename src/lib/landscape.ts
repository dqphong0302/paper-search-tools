import { Paper } from '../types';

export interface Counted {
  key: string;
  count: number;
}

export interface YearBucket {
  year: number;
  count: number;
}

export interface QueryCoverage {
  term: string;
  count: number;
}

export interface Landscape {
  total: number;
  years: YearBucket[];
  sources: Counted[];
  venues: Counted[];
  topTerms: Counted[];
  queryCoverage: QueryCoverage[];
  distinctAuthors: number;
  openAccess: number;
  withAbstract: number;
}

const STOPWORDS = new Set([
  // English
  'the', 'and', 'for', 'with', 'from', 'that', 'this', 'are', 'was', 'were', 'has', 'have',
  'not', 'using', 'used', 'use', 'based', 'study', 'review', 'analysis', 'research', 'results',
  'between', 'among', 'into', 'their', 'its', 'our', 'can', 'may', 'also', 'than', 'more', 'less',
  'which', 'these', 'those', 'there', 'been', 'being', 'such', 'both', 'each', 'other', 'however',
  'while', 'after', 'before', 'during', 'within', 'without', 'through', 'over', 'under', 'all',
  'but', 'who', 'how', 'what', 'when', 'where', 'here', 'had', 'did', 'does', 'one', 'two', 'new',
  // Generic scholarly words that describe any paper rather than its subject
  'paper', 'papers', 'article', 'articles', 'result', 'findings', 'finding', 'method', 'methods',
  'methodology', 'approach', 'approaches', 'studies', 'data', 'background', 'objective',
  'objectives', 'aim', 'aims', 'purpose', 'conclusion', 'conclusions', 'introduction',
  'discussion', 'version', 'author', 'authors', 'journal', 'abstract', 'present', 'presents',
  'proposed', 'propose', 'show', 'shows', 'showed', 'shown', 'found', 'performed', 'total',
  'significant', 'significantly', 'including', 'associated', 'compared', 'effect', 'effects',
  'high', 'higher', 'low', 'lower', 'first', 'three', 'various', 'different', 'important',
  'provide', 'provides', 'overall', 'well', 'will', 'number', 'years', 'year', 'patients',
  'case', 'cases', 'group', 'groups', 'report', 'reports', 'systematic', 'literature',
  // Vietnamese
  'của', 'và', 'cho', 'trong', 'một', 'các', 'những', 'với', 'được', 'trên', 'khi', 'này',
  'kết', 'quả', 'nghiên', 'cứu', 'phân', 'tích', 'dựa', 'theo', 'về', 'từ', 'đến', 'hay',
  'là', 'có', 'không', 'người', 'đã', 'để', 'tại', 'như', 'bài', 'báo', 'tạp', 'chí', 'phương',
  'pháp', 'mục', 'tiêu', 'luận', 'đánh', 'giá', 'thực', 'hiện', 'nhóm', 'năm', 'số', 'cao', 'thấp',
]);

/** Lowercased word tokens of length > 2, minus stopwords. */
export function tokenize(text: string): string[] {
  return text
    .toLocaleLowerCase('vi')
    .split(/[^\p{L}\p{N}]+/u)
    .filter((token) => token.length > 2 && !STOPWORDS.has(token) && !/^\d+$/.test(token));
}

function topCounts(values: string[], limit: number): Counted[] {
  const map = new Map<string, number>();
  for (const value of values) {
    const key = value.trim();
    if (!key) continue;
    map.set(key, (map.get(key) ?? 0) + 1);
  }
  return [...map.entries()]
    .map(([key, count]) => ({ key, count }))
    .sort((a, b) => b.count - a.count || a.key.localeCompare(b.key))
    .slice(0, limit);
}

/**
 * Bibliometric landscape of the current result set. These are descriptive counts
 * over what the sources returned — not evidence about the state of the field.
 */
export function analyzeLandscape(
  papers: Paper[],
  query: string,
  currentYear = new Date().getFullYear()
): Landscape {
  const total = papers.length;

  const yearStart = currentYear - 9;
  const yearMap = new Map<number, number>();
  for (let year = yearStart; year <= currentYear; year += 1) yearMap.set(year, 0);
  for (const paper of papers) {
    if (paper.year && paper.year >= yearStart && paper.year <= currentYear) {
      yearMap.set(paper.year, (yearMap.get(paper.year) ?? 0) + 1);
    }
  }
  const years = [...yearMap.entries()]
    .map(([year, count]) => ({ year, count }))
    .sort((a, b) => a.year - b.year);

  const sources = topCounts(papers.map((paper) => paper.source), 8);
  const venues = topCounts(papers.map((paper) => paper.venue || ''), 6);

  // Count each term once per paper, so one long abstract cannot dominate.
  const terms = papers.flatMap((paper) => [
    ...new Set(tokenize(`${paper.title} ${paper.abstract ?? ''}`)),
  ]);
  const topTerms = topCounts(terms, 12);

  const queryTokens = [...new Set(tokenize(query))];
  const queryCoverage = queryTokens
    .map((term) => ({
      term,
      count: papers.filter((paper) =>
        `${paper.title} ${paper.abstract ?? ''}`.toLocaleLowerCase('vi').includes(term)
      ).length,
    }))
    .sort((a, b) => a.count - b.count || a.term.localeCompare(b.term));

  const authors = new Set<string>();
  for (const paper of papers) for (const author of paper.authors) authors.add(author.trim().toLowerCase());

  return {
    total,
    years,
    sources,
    venues,
    topTerms,
    queryCoverage,
    distinctAuthors: authors.size,
    openAccess: papers.filter((paper) => paper.open_access === true).length,
    withAbstract: papers.filter((paper) => paper.abstract?.trim()).length,
  };
}
