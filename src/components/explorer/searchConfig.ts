import searchCatalog from '../../lib/searchCatalog.json';
import { SourceGroup } from '../../lib/paperEvaluation';

export type Scope = string;
export type SourceFilter = 'all' | SourceGroup | 'interested';
export type SortKey = 'relevance' | 'evaluation' | 'pdf' | 'year' | 'citations';

export const AVAILABLE_SOURCE_IDS = new Set(searchCatalog.sources.filter((source) => source.available !== false).map((source) => source.id));
// Hide empty/unsearchable groups and count only sources the current build can query.
export const AVAILABLE_PRESETS = searchCatalog.presets
  .map((preset) => ({ ...preset, sources: preset.sources.filter((id) => AVAILABLE_SOURCE_IDS.has(id)) }))
  .filter((preset) => preset.sources.length > 0);
export const SCOPES = [
  { id: 'default', label: 'Default (Settings)', title: 'Use the source selection from Settings' },
  ...AVAILABLE_PRESETS.map((p) => ({ id: p.id, label: p.label, title: p.description })),
];
export const QUICK_DISCIPLINES = ['vietnam', 'biomedical', 'ai_cs', 'stem_nature', 'social_humanities', 'evidence_review', 'patents_gov', 'global_regional', 'open_access', 'preprints']
  .map(id => AVAILABLE_PRESETS.find(p => p.id === id)!)
  .filter(Boolean);

export const isKnownPreset = (id: string) => AVAILABLE_PRESETS.some((preset) => preset.id === id);

/** Source errors already start with the source name; do not print it twice. */
export function sourceMessage(name: string, error?: string | null, fallback = 'unknown error'): string {
  const text = (error || fallback).trim();
  const lowered = text.toLowerCase();
  const prefix = name.toLowerCase();
  if (lowered.startsWith(`${prefix}:`)) return `${name}: ${text.slice(prefix.length + 1).trim()}`;
  if (lowered.startsWith(prefix)) return text;
  return `${name}: ${text}`;
}

/** Filters the user edits in the scope bar and the advanced options. */
export interface SearchFilters {
  scope: Scope;
  customSources: string[];
  yearMin: string;
  yearMax: string;
  resultLimit: string;
}

export const DEFAULT_FILTERS: Omit<SearchFilters, 'scope'> = { customSources: [], yearMin: '', yearMax: '', resultLimit: '' };

export function filtersError(f: SearchFilters): string | null {
  if (
    (f.yearMin && !/^\d{4}$/.test(f.yearMin)) ||
    (f.yearMax && !/^\d{4}$/.test(f.yearMax)) ||
    (f.yearMin && f.yearMax && Number(f.yearMin) > Number(f.yearMax)) ||
    (f.resultLimit &&
      (!Number.isInteger(Number(f.resultLimit)) || Number(f.resultLimit) < 1 || Number(f.resultLimit) > 50))
  ) {
    return 'Enter valid four-digit years (start ≤ end) and a limit between 1 and 50.';
  }
  return null;
}

/** Explicit sources win; otherwise a preset, or nothing for the Settings default. */
export function sourcesFor(scope: Scope, customSources: string[]): string[] | undefined {
  if (customSources.length > 0) return customSources;
  return scope === 'default' ? undefined : [scope];
}
