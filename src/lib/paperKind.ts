import { Paper } from '../types';

/**
 * Non-article research outputs (datasets, reports, advisories, discussions) are
 * stored in the same Paper shape so they fuse and save like everything else, but
 * the UI labels them separately instead of calling every record a "paper".
 */
export type PaperKind = 'article' | 'dataset' | 'report' | 'advisory' | 'discussion' | 'software' | 'record';

const KIND_BY_SOURCE: Record<string, PaperKind> = {
  Dryad: 'dataset',
  Dataverse: 'dataset',
  Zenodo: 'dataset',
  'HF Datasets': 'dataset',
  Figshare: 'dataset',
  'NCBI GEO': 'dataset',
  NTRS: 'report',
  'World Bank': 'report',
  'SEC EDGAR': 'report',
  'NVD CVE': 'advisory',
  'CISA KEV': 'advisory',
  StackExchange: 'discussion',
  'Software Heritage': 'software',
  OpenFDA: 'record',
  UniProt: 'record',
  ClinVar: 'record',
};

export const KIND_META: Record<PaperKind, { label: string; badge: string | null }> = {
  article: { label: 'Article', badge: null },
  dataset: { label: 'Dataset', badge: 'badge-emerald' },
  report: { label: 'Report', badge: 'badge-cyan' },
  advisory: { label: 'Advisory', badge: 'badge-vjol' },
  discussion: { label: 'Discussion', badge: 'badge-violet' },
  software: { label: 'Software', badge: 'badge-group' },
  record: { label: 'Record', badge: 'badge-group' },
};

export function getPaperKind(paper: Paper): PaperKind {
  return KIND_BY_SOURCE[paper.source] ?? 'article';
}

export const isArticle = (paper: Paper) => getPaperKind(paper) === 'article';
