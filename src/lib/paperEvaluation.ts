import { Paper } from '../types';

export type SourceGroup =
  | 'biomedical'
  | 'multidisciplinary'
  | 'preprint'
  | 'resolver'
  | 'vietnam'
  | 'meta';

export const SOURCE_GROUPS: Record<SourceGroup, { label: string; shortLabel: string }> = {
  biomedical: { label: 'Curated Biomedical', shortLabel: 'Biomedical' },
  multidisciplinary: { label: 'Multidisciplinary Index', shortLabel: 'Multidisciplinary' },
  preprint: { label: 'Preprints & Early Access', shortLabel: 'Preprint' },
  resolver: { label: 'DOI & Metadata Resolvers', shortLabel: 'DOI' },
  vietnam: { label: 'Vietnam Academic Repositories', shortLabel: 'Vietnam' },
  meta: { label: 'Federated Meta-Search', shortLabel: 'Meta-Search' },
};

export const getSourceGroup = (paper: Paper): SourceGroup => {
  const source = `${paper.source} ${paper.venue || ''}`.toLocaleLowerCase('vi');
  if (source.includes('việt nam') || source.includes('vietnam') || source.includes('vjol') || source.includes('nasati') || source.includes('vast') || source.includes('vnu') || source.includes('hust') || source.includes('medpharmres')) return 'vietnam';
  if (source.includes('pubmed') || source.includes('medline') || source.includes('europe pmc') || source.includes('clinicaltrials') || source.includes('pmc') || source.includes('plos')) return 'biomedical';
  if (source.includes('arxiv') || source.includes('biorxiv') || source.includes('medrxiv') || source.includes('preprint')) return 'preprint';
  if (source.includes('crossref') || source.includes('doi') || source.includes('datacite')) return 'resolver';
  if (source.includes('searx') || source.includes('perplexity')) return 'meta';
  return 'multidisciplinary';
};

export interface PaperEvaluation {
  overall: number;
  label: 'Recommended' | 'Consider' | 'Incomplete';
  relevance: number;
  metadata: number;
  recency: number;
  citation: number;
  access: number;
  pdfScore: number;
  recommendedPdf: boolean;
}

const clamp = (value: number) => Math.max(0, Math.min(100, Math.round(value)));

const hasDirectPdf = (paper: Paper) => {
  // The gateway only populates `pdf_url` from fields explicitly declared as
  // PDF by the upstream source (including Unpaywall's `url_for_pdf`).
  return Boolean(paper.pdf_url);
};

export const evaluatePaper = (paper: Paper): PaperEvaluation => {
  const relevance = clamp((paper.score ?? 0) * 3000);
  const metadata = clamp(
    (paper.title ? 15 : 0) +
      (paper.authors.length ? 15 : 0) +
      (paper.year ? 12 : 0) +
      (paper.venue ? 12 : 0) +
      (paper.doi ? 16 : 0) +
      (paper.abstract ? 30 : 0)
  );
  const age = paper.year ? Math.max(0, new Date().getFullYear() - paper.year) : 20;
  const recency = clamp(100 - age * 8);
  const citation = clamp(paper.citations == null ? 35 : Math.log10(paper.citations + 1) * 35);
  const directPdf = hasDirectPdf(paper);
  const access = directPdf ? 100 : paper.open_access ? 70 : 15;
  // Index membership, publication age and citation volume do not establish study quality.
  // Rank for reading convenience only, without favoring disciplines or indexed providers.
  const overall = clamp(
    relevance * 0.4 + metadata * 0.4 + access * 0.2
  );
  const pdfScore = clamp(
    access * 0.4 + relevance * 0.3 + metadata * 0.3
  );

  return {
    overall,
    label: overall >= 75 ? 'Recommended' : overall >= 58 ? 'Consider' : 'Incomplete',
    relevance,
    metadata,
    recency,
    citation,
    access,
    pdfScore,
    recommendedPdf: directPdf && pdfScore >= 65,
  };
};
