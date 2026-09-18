import { Paper } from '../types';
import { getSourceGroup } from '../lib/paperEvaluation';

/**
 * Types and pure helpers shared by `Explorer` and the `PaperCard` rows it
 * renders. They live here rather than in `Explorer` so the card does not have
 * to import the component that renders it.
 */

export type CitationDirection = 'cited_by' | 'references' | 'related';

export interface CitationState {
  direction: CitationDirection;
  loading: boolean;
  error: string | null;
  items: Paper[];
}

/** Characters of abstract shown before the row offers to expand. */
export const ABSTRACT_CLAMP = 280;

/** Vietnam papers indexed through the national repository filter. */
export const isVietnamPaper = (paper: Paper): boolean => getSourceGroup(paper) === 'vietnam';

export const originalPaperUrl = (paper: Paper): string | null =>
  paper.source_url || (paper.doi ? `https://doi.org/${paper.doi}` : null) || paper.pdf_url || null;

const KEYWORD_STOPWORDS = new Set([
  'about', 'after', 'among', 'analysis', 'based', 'between', 'from', 'into', 'study', 'using',
  'with', 'without', 'this', 'that', 'these', 'those', 'the', 'and', 'for', 'trong', 'nghiên', 'cứu',
]);

export function suggestedKeywords(paper: Paper): string[] {
  const seen = new Set<string>();
  return `${paper.title} ${paper.abstract || ''}`
    .toLowerCase()
    .match(/[\p{L}\p{N}-]{4,}/gu)
    ?.filter((word) => !KEYWORD_STOPWORDS.has(word) && !seen.has(word) && seen.add(word))
    .slice(0, 6) || [];
}
