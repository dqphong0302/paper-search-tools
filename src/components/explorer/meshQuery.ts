export type MeshField = 'mh' | 'majr' | 'tiab' | 'ti' | 'all';

export interface MeshGroup {
  terms: string;
  field: MeshField;
}

export interface QueryPreviewItem {
  id: string;
  mode: 'pubmed_mesh' | 'europe_pmc' | 'arxiv' | 'native_boolean' | 'plain_keywords';
  query: string;
  notes: string;
}

export function buildMeshQuery(
  groups: MeshGroup[],
  operator: 'AND' | 'OR',
  exclusions: string,
  explode: boolean
): string {
  const tagged = (raw: string, field: MeshField) => {
    const value = raw.trim();
    if (!value) return '';
    const term = /\s/.test(value) && !(value.startsWith('"') && value.endsWith('"')) ? `"${value}"` : value;
    if (field === 'all') return term;
    const tag = field === 'mh' && !explode ? 'mh:noexp' : field;
    return `${term}[${tag}]`;
  };
  const clauses = groups
    .map((group) => group.terms.split(/[|\n]/).map((term) => tagged(term, group.field)).filter(Boolean))
    .filter((terms) => terms.length > 0)
    .map((terms) => (terms.length > 1 ? `(${terms.join(' OR ')})` : terms[0]));
  const excluded = exclusions
    .split(/[|\n]/)
    .map((term) => tagged(term, 'all'))
    .filter(Boolean);
  const positive = clauses.join(` ${operator} `);
  if (!positive) return '';
  return excluded.length
    ? `${positive} NOT ${excluded.length > 1 ? `(${excluded.join(' OR ')})` : excluded[0]}`
    : positive;
}
