import React, { useState, useEffect, useCallback, useMemo, useRef } from 'react';
import {
  Search,
  Check,
  Sparkles,
  AlertTriangle,
  Compass,
  ExternalLink,
  Loader2,
  CheckCircle2,
  XCircle,
  Calendar,
  Layers,
  PauseCircle,
  Settings,
  X,
  Bot,
  Filter,
  SlidersHorizontal,
} from 'lucide-react';
import { Paper, SearchResponse } from '../types';
import { SearchScanner } from './SearchScanner';
import { ResearchGapPanel } from './ResearchGapPanel';
import { EvidenceSynthesis } from './EvidenceSynthesis';
import { evaluatePaper, getSourceGroup, SOURCE_GROUPS, SourceGroup } from '../lib/paperEvaluation';
import { apaCitation, bibtexCitation } from '../lib/citation';
import { getPaperKind, KIND_META, PaperKind } from '../lib/paperKind';
import { gatewayFetch } from '../lib/gateway';
import { useSelection } from '../lib/useSelection';
import { bibtexLibrary, risLibrary } from '../lib/citation';
import { SourceLimiterModal } from './SourceLimiterModal';
import { FulltextViewerModal } from './FulltextViewerModal';
import { AiAgentExportModal } from './AiAgentExportModal';

import searchCatalog from '../lib/searchCatalog.json';
import { PaperCard } from './PaperCard';
import {
  CitationDirection, CitationState, isVietnamPaper, originalPaperUrl,
  suggestedKeywords,
} from './Explorer.shared';

// Re-exported: these moved to Explorer.shared so PaperCard can use them without
// importing the component that renders it. Existing importers are unaffected.
export { isVietnamPaper, originalPaperUrl, suggestedKeywords };

/** Source errors already start with the source name; do not print it twice. */
function sourceMessage(name: string, error?: string | null, fallback = 'unknown error'): string {
  const text = (error || fallback).trim();
  const lowered = text.toLowerCase();
  const prefix = name.toLowerCase();
  if (lowered.startsWith(`${prefix}:`)) return `${name}: ${text.slice(prefix.length + 1).trim()}`;
  if (lowered.startsWith(prefix)) return text;
  return `${name}: ${text}`;
}

type Scope = string;
type SourceFilter = 'all' | SourceGroup | 'interested';
type SortKey = 'relevance' | 'evaluation' | 'pdf' | 'year' | 'citations';
type MeshField = 'mh' | 'majr' | 'tiab' | 'ti' | 'all';

interface MeshGroup {
  terms: string;
  field: MeshField;
}

interface QueryPreviewItem {
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

const AVAILABLE_SOURCE_IDS = new Set(searchCatalog.sources.filter((source) => source.available !== false).map((source) => source.id));
// Hide empty/unsearchable groups and count only sources the current build can query.
const AVAILABLE_PRESETS = searchCatalog.presets
  .map((preset) => ({ ...preset, sources: preset.sources.filter((id) => AVAILABLE_SOURCE_IDS.has(id)) }))
  .filter((preset) => preset.sources.length > 0);
const SCOPES = [
  { id: 'default', label: 'Default (Settings)', title: 'Use the source selection from Settings' },
  ...AVAILABLE_PRESETS.map((p) => ({ id: p.id, label: p.label, title: p.description })),
];
const QUICK_DISCIPLINES = ['vietnam', 'biomedical', 'ai_cs', 'stem_nature', 'social_humanities', 'evidence_review', 'patents_gov', 'global_regional', 'open_access', 'preprints']
  .map(id => AVAILABLE_PRESETS.find(p => p.id === id)!)
  .filter(Boolean);

interface ExplorerProps {
  onSavePaper: (paper: Paper) => void;
  savedPaperIds: Set<string>;
  initialQuery?: string;
  draftQuery?: string;
  onSubmitQuery?: (query: string) => void;
  searchNonce?: number;
  port: number;
  /** The app-shell omnibox is the single search entry, so hide the in-page one. */
  hideSearchBar?: boolean;
  initialScope?: string;
}

export const Explorer: React.FC<ExplorerProps> = ({
  onSavePaper,
  savedPaperIds,
  initialQuery,
  draftQuery,
  onSubmitQuery,
  searchNonce = 0,
  port,
  hideSearchBar = false,
  initialScope = 'default',
}) => {
  const [localQuery, setQuery] = useState(initialQuery || '');
  const query = draftQuery ?? localQuery;
  const [loading, setLoading] = useState(false);
  const [error, setError] = useState<string | null>(null);
  const [results, setResults] = useState<SearchResponse | null>(null);
  const [selectedId, setSelectedId] = useState<string | null>(null);
  const detailRef = useRef<HTMLElement>(null);
  const detailTrigger = useRef<HTMLButtonElement | null>(null);
  // Every handler a PaperCard receives is wrapped so its identity is stable:
  // otherwise a new closure on each render would defeat the card's `memo` and
  // the extraction would buy nothing.
  const closeDetails = useCallback(() => {
    setSelectedId(null);
    detailTrigger.current?.focus();
  }, []);
  useEffect(() => {
    if (selectedId) detailRef.current?.focus();
  }, [selectedId]);
  const [expandedAbstracts, setExpandedAbstracts] = useState<Record<string, boolean>>({});
  const [copiedId, setCopiedId] = useState<string | null>(null);
  const [downloadingId, setDownloadingId] = useState<string | null>(null);
  const [downloadSuccessId, setDownloadSuccessId] = useState<string | null>(null);
  // A blocked download is not a dead end, so the banner carries the article it
  // failed on and offers its page.
  const [downloadError, setDownloadError] = useState<{ message: string; paper?: Paper } | null>(null);
  const [oaOnly, setOaOnly] = useState(false);
  const [recommendedPdfOnly, setRecommendedPdfOnly] = useState(false);
  const [sortKey, setSortKey] = useState<SortKey>('relevance');
  const [citationOpen, setCitationOpen] = useState<Record<string, boolean>>({});
  const [citations, setCitations] = useState<Record<string, CitationState>>({});

  // Source Limiter Modal State
  const [sourceLimiterOpen, setSourceLimiterOpen] = useState(false);
  const [customSources, setCustomSources] = useState<string[]>([]);

  // Fulltext Viewer Modal State
  const [readerPaper, setReaderPaper] = useState<Paper | null>(null);
  const [readerOpen, setReaderOpen] = useState(false);

  // AI Agent Export Modal State
  const [agentExportModalOpen, setAgentExportModalOpen] = useState(false);
  const [agentExportPapers, setAgentExportPapers] = useState<Paper[]>([]);

  const [sourceFilter, setSourceFilter] = useState<SourceFilter>('all');
  const [kindFilter, setKindFilter] = useState<'all' | PaperKind>('all');
  const [searchScope, setSearchScope] = useState<Scope>(AVAILABLE_PRESETS.some((preset) => preset.id === initialScope) ? initialScope : 'default');
  const [yearMin, setYearMin] = useState('');
  const [yearMax, setYearMax] = useState('');
  const [resultLimit, setResultLimit] = useState('');
  const [meshGroups, setMeshGroups] = useState<MeshGroup[]>([
    { terms: '', field: 'mh' },
    { terms: '', field: 'tiab' },
  ]);
  const [meshOperator, setMeshOperator] = useState<'AND' | 'OR'>('AND');
  const [meshExclusions, setMeshExclusions] = useState('');
  const [meshExplode, setMeshExplode] = useState(true);
  const [queryPreview, setQueryPreview] = useState<QueryPreviewItem[]>([]);
  const searchOptionsRef = useRef<HTMLDetailsElement>(null);
  const latestSearchId = useRef(0);
  const completedSearch = useRef<{ id: number; body: string; offset: number } | null>(null);
  const loadingMoreRequest = useRef<number | null>(null);
  const [loadingMore, setLoadingMore] = useState(false);
  const generatedMeshQuery = useMemo(
    () => buildMeshQuery(meshGroups, meshOperator, meshExclusions, meshExplode),
    [meshGroups, meshOperator, meshExclusions, meshExplode]
  );

  useEffect(() => {
    if (!generatedMeshQuery) {
      setQueryPreview([]);
      return;
    }
    const timer = window.setTimeout(() => {
      const sources = customSources.length
        ? customSources
        : searchScope === 'default'
        ? undefined
        : [searchScope];
      void gatewayFetch('/api/query/preview', {
        method: 'POST',
        headers: { 'Content-Type': 'application/json' },
        body: JSON.stringify({ query: generatedMeshQuery, sources }),
      })
        .then((response) => (response.ok ? response.json() : Promise.reject()))
        .then((data) => setQueryPreview(Array.isArray(data?.sources) ? data.sources : []))
        .catch(() => setQueryPreview([]));
    }, 250);
    return () => window.clearTimeout(timer);
  }, [generatedMeshQuery, customSources, searchScope]);

  useEffect(() => {
    if (!results && customSources.length === 0 && AVAILABLE_PRESETS.some((preset) => preset.id === initialScope)) {
      setSearchScope(initialScope);
    }
  }, [initialScope, results, customSources.length]);

  useEffect(() => () => { ++latestSearchId.current; }, []);

  const runSearch = useCallback(
    async (
      searchQuery: string,
      scope: Scope,
      openAccessOnly = oaOnly,
      limitOverride?: number,
      explicitSources = customSources
    ) => {
      const q = searchQuery.trim();
      if (!q) {
        setError('Enter a keyword, author name or DOI before searching.');
        return;
      }
      if (
        (yearMin && !/^\d{4}$/.test(yearMin)) ||
        (yearMax && !/^\d{4}$/.test(yearMax)) ||
        (yearMin && yearMax && Number(yearMin) > Number(yearMax)) ||
        (resultLimit &&
          (!Number.isInteger(Number(resultLimit)) || Number(resultLimit) < 1 || Number(resultLimit) > 50))
      ) {
        setError('Enter valid four-digit years (start ≤ end) and a limit between 1 and 50.');
        return;
      }
      const requestId = ++latestSearchId.current;
      if (searchOptionsRef.current) searchOptionsRef.current.open = false;
      completedSearch.current = null;
      loadingMoreRequest.current = null;
      setLoadingMore(false);
      setResults(null);
      setSelectedId(null);
      setLoading(true);
      setError(null);

      try {
        const sourcesParam =
          explicitSources.length > 0
            ? explicitSources
            : scope === 'default'
            ? undefined
            : [scope];

        const body = JSON.stringify({
          query: q,
          limit: limitOverride ?? (resultLimit ? Number(resultLimit) : undefined),
          year_min: yearMin ? Number(yearMin) : undefined,
          year_max: yearMax ? Number(yearMax) : undefined,
          open_access_only: openAccessOnly,
          sources: sourcesParam,
        });
        const res = await gatewayFetch('/api/search', {
          method: 'POST',
          headers: { 'Content-Type': 'application/json' },
          body,
        });

        if (!res.ok) throw new Error(`Gateway returned error code ${res.status}`);

        const data: SearchResponse = await res.json();
        if (requestId !== latestSearchId.current) return;
        completedSearch.current = { id: requestId, body, offset: data.papers.length };
        setResults(data);
        setSourceFilter(scope === 'vietnam' ? 'vietnam' : 'all');
      } catch (err) {
        if (requestId !== latestSearchId.current) return;
        setResults(null);
        setError(
          err instanceof TypeError
            ? `Could not reach the gateway at 127.0.0.1:${port}. Check that it is running.`
            : (err as Error).message || 'Search failed.'
        );
      } finally {
        if (requestId === latestSearchId.current) setLoading(false);
      }
    },
    [port, yearMin, yearMax, resultLimit, oaOnly, customSources]
  );

  // A query pushed in from another tab (dashboard, history) always re-runs.
  useEffect(() => {
    if (initialQuery && initialQuery.trim()) {
      setQuery(initialQuery);
      runSearch(initialQuery, searchScope, oaOnly, undefined, customSources);
    } else {
      ++latestSearchId.current;
      completedSearch.current = null;
      setResults(null);
      setError(null);
      setLoading(false);
    }
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [initialQuery, searchNonce]);

  const handleScopeChange = (scope: Scope) => {
    setSearchScope(scope);
    // Clear explicit custom source overrides when user selects a preset directly
    setCustomSources([]);
  };

  const handleApplyCustomSources = (sources: string[]) => {
    setCustomSources(sources);
    if (query.trim()) {
      void runSearch(query, searchScope, oaOnly, undefined, sources);
    }
  };

  const submitSearch = () => {
    if (onSubmitQuery) onSubmitQuery(query);
    else void runSearch(query, searchScope, oaOnly, undefined, customSources);
  };

  const applyMeshSearch = () => {
    if (!generatedMeshQuery) return;
    if (onSubmitQuery) onSubmitQuery(generatedMeshQuery);
    else {
      setQuery(generatedMeshQuery);
      void runSearch(generatedMeshQuery, searchScope, oaOnly, undefined, customSources);
    }
  };

  const toggleAbstract = useCallback(
    (id: string) => setExpandedAbstracts((prev) => ({ ...prev, [id]: !prev[id] })),
    []
  );

  const copyCitation = useCallback((paper: Paper, format: 'apa' | 'bibtex' = 'apa') => {
    const citation = format === 'bibtex' ? bibtexCitation(paper) : apaCitation(paper);
    navigator.clipboard
      .writeText(citation)
      .then(() => {
        setCopiedId(`${paper.id}:${format}`);
        setTimeout(() => setCopiedId(null), 2000);
      })
      .catch(() => setError('Could not copy the citation to the clipboard.'));
  }, []);

  const handleOaToggle = (next: boolean) => {
    setOaOnly(next);
    if (query.trim()) runSearch(query, searchScope, next, undefined, customSources);
  };

  const loadCitations = useCallback(
    async (paper: Paper, direction: CitationDirection) => {
      setCitations((prev) => ({
        ...prev,
        [paper.id]: { direction, loading: true, error: null, items: prev[paper.id]?.items ?? [] },
      }));
      try {
        const res = await gatewayFetch(
          `/api/citations?id=${encodeURIComponent(paper.id)}&direction=${direction}&limit=15`
        );
        const data = await res.json().catch(() => null);
        if (!res.ok) throw new Error(data?.error || `HTTP ${res.status}`);
        setCitations((prev) => ({
          ...prev,
          [paper.id]: {
            direction,
            loading: false,
            error: null,
            items: Array.isArray(data?.items) ? data.items : [],
          },
        }));
      } catch (e) {
        setCitations((prev) => ({
          ...prev,
          [paper.id]: { direction, loading: false, error: (e as Error).message, items: [] },
        }));
      }
    },
    []
  );

  // Read through a ref so the callback does not have to depend on the citation
  // maps, which would invalidate every card's `memo` whenever any one row
  // loaded its citations.
  const citationsRef = useRef(citations);
  citationsRef.current = citations;
  const citationOpenRef = useRef(citationOpen);
  citationOpenRef.current = citationOpen;

  const toggleCitations = useCallback(
    (paper: Paper) => {
      const opening = !citationOpenRef.current[paper.id];
      setCitationOpen((prev) => ({ ...prev, [paper.id]: opening }));
      if (opening && !citationsRef.current[paper.id]) void loadCitations(paper, 'cited_by');
    },
    [loadCitations]
  );

  const handleLoadMore = useCallback(async () => {
    const search = completedSearch.current;
    if (!search || search.id !== latestSearchId.current || loadingMoreRequest.current !== null) return;
    const requestId = search.id;
    const offset = search.offset;
    loadingMoreRequest.current = requestId;
    setLoadingMore(true);
    setError(null);
    try {
      const res = await gatewayFetch('/api/search', {
        method: 'POST',
        headers: { 'Content-Type': 'application/json' },
        body: JSON.stringify({
          ...JSON.parse(search.body),
          limit: 15,
          offset,
        }),
      });
      if (!res.ok) throw new Error(`Gateway returned error code ${res.status}`);
      const data: SearchResponse = await res.json();
      if (requestId !== latestSearchId.current) return;
      search.offset += data.papers.length;
      setResults((prev) => {
        if (!prev || requestId !== latestSearchId.current) return prev;
        const seen = new Set(prev.papers.map((p) => p.id));
        const merged = [...prev.papers, ...data.papers.filter((p) => !seen.has(p.id))];
        return {
          ...prev,
          papers: merged,
          total: merged.length,
          available_total: data.papers.length ? data.available_total ?? prev.available_total : merged.length,
          sources: data.sources ?? prev.sources,
        };
      });
    } catch (e) {
      if (requestId !== latestSearchId.current) return;
      setError((e as Error).message || 'Could not load more results.');
    } finally {
      if (requestId === latestSearchId.current) {
        loadingMoreRequest.current = null;
        setLoadingMore(false);
      }
    }
  }, []);

  const handleDownload = useCallback(async (paper: Paper) => {
    if (!paper.pdf_url) return;
    setDownloadingId(paper.id);
    setDownloadError(null);
    try {
      const res = await gatewayFetch('/api/download', {
        method: 'POST',
        headers: { 'Content-Type': 'application/json' },
        body: JSON.stringify({
          paper_id: paper.id,
          title: paper.title,
          pdf_url: paper.pdf_url,
          source: paper.source,
          year: paper.year,
        }),
      });
      const json = await res.json().catch(() => null);
      if (!res.ok || (json && json.success === false)) {
        setDownloadError({
          message: json?.error || `The gateway returned status ${res.status}`,
          paper,
        });
        setTimeout(() => setDownloadError(null), 12000);
        return;
      }
      setDownloadSuccessId(paper.id);
      setTimeout(() => setDownloadSuccessId(null), 3000);
    } catch (err) {
      setDownloadError({ message: `PDF download failed: ${(err as Error).message}`, paper });
      setTimeout(() => setDownloadError(null), 12000);
    } finally {
      setDownloadingId(null);
    }
  }, []);

  const openFulltextReader = useCallback((paper: Paper) => {
    setReaderPaper(paper);
    setReaderOpen(true);
  }, []);

  const openAgentExport = useCallback((paper: Paper) => {
    setAgentExportPapers([paper]);
    setAgentExportModalOpen(true);
  }, []);

  // --- Derived lists -------------------------------------------------------
  const allPapers = useMemo(() => {
    const papers = results?.papers || [];
    return papers.filter((paper) => {
      if (oaOnly && !(paper.open_access || paper.pdf_url)) return false;
      if (recommendedPdfOnly && !evaluatePaper(paper).recommendedPdf) return false;
      if (kindFilter !== 'all' && getPaperKind(paper) !== kindFilter) return false;
      return true;
    });
  }, [results, oaOnly, recommendedPdfOnly, kindFilter]);

  const sourceCounts = useMemo(() => {
    const counts = Object.fromEntries(
      Object.keys(SOURCE_GROUPS).map((group) => [group, 0])
    ) as Record<SourceGroup, number>;
    allPapers.forEach((paper) => {
      counts[getSourceGroup(paper)] += 1;
    });
    return counts;
  }, [allPapers]);

  const interestedCount = useMemo(() => {
    return allPapers.filter((paper) => savedPaperIds.has(paper.id)).length;
  }, [allPapers, savedPaperIds]);

  const displayedPapers = useMemo(() => {
    const base =
      sourceFilter === 'all'
        ? allPapers
        : sourceFilter === 'interested'
        ? allPapers.filter((paper) => savedPaperIds.has(paper.id))
        : allPapers.filter((paper) => getSourceGroup(paper) === sourceFilter);

    if (sortKey === 'relevance') return base;
    return [...base].sort((a, b) => {
      if (sortKey === 'year') return (b.year || 0) - (a.year || 0);
      if (sortKey === 'citations') return (b.citations || 0) - (a.citations || 0);
      if (sortKey === 'pdf') return evaluatePaper(b).pdfScore - evaluatePaper(a).pdfScore;
      return evaluatePaper(b).overall - evaluatePaper(a).overall;
    });
  }, [sourceFilter, sortKey, allPapers, savedPaperIds]);

  const hiddenByOa = (results?.papers || []).filter((paper) => !(paper.open_access || paper.pdf_url)).length;
  const availableTotal = results?.available_total ?? results?.total ?? 0;
  const canLoadMore =
    !!results &&
    availableTotal > (completedSearch.current?.offset ?? results.papers.length) &&
    (completedSearch.current?.offset ?? 0) < 10_000;

  const currentPresetMeta = AVAILABLE_PRESETS.find((p) => p.id === searchScope);

  // Selection over the rows currently shown, so papers can be gathered straight
  // from the results and exported without saving them first.
  const visibleIds = useMemo(() => displayedPapers.map((paper) => paper.id), [displayedPapers]);
  const selection = useSelection(visibleIds);
  const selectedPapers = useMemo(
    () => allPapers.filter((paper) => selection.isSelected(paper.id)),
    [allPapers, selection]
  );

  const downloadFile = (content: string, extension: string, mime: string) => {
    const blob = new Blob([content], { type: mime });
    const url = URL.createObjectURL(blob);
    const anchor = document.createElement('a');
    anchor.href = url;
    anchor.download = `scholargate_${new Date().toISOString().slice(0, 10)}.${extension}`;
    document.body.appendChild(anchor);
    anchor.click();
    anchor.remove();
    URL.revokeObjectURL(url);
  };

  const exportSelectedRis = () =>
    downloadFile(risLibrary(selectedPapers), 'ris', 'application/x-research-info-systems');
  const exportSelectedBibtex = () =>
    downloadFile(bibtexLibrary(selectedPapers), 'bib', 'application/x-bibtex');

  return (
    <div className="page-container">
      {/* ---------------- Search & Scope Bar ---------------- */}
      <div style={{ display: 'flex', flexDirection: 'column', gap: 12 }}>
        {!hideSearchBar && (
          <form
            id="paper-search"
            className="search-omnibox"
            onSubmit={(e) => {
              e.preventDefault();
              submitSearch();
            }}
          >
            <Search size={18} style={{ color: 'var(--primary-cyan)', marginLeft: 4 }} />
            <input
              id="paper-search-input"
              type="text"
              className="search-input"
              placeholder="Search keywords, authors, topics, DOI (10.xxx) or PMID… (⌘K)"
              value={query}
              onChange={(e) => {
                setQuery(e.target.value);
              }}
              aria-label="Search query"
            />
            <button
              id="paper-search-submit"
              type="submit"
              className="search-submit-btn"
              disabled={loading || !query.trim()}
            >
              {loading ? (
                <>
                  <Loader2 size={14} className="animate-spin" />
                  <span>Scanning…</span>
                </>
              ) : (
                <>
                  <Sparkles size={14} />
                  <span>Search</span>
                </>
              )}
            </button>
          </form>
        )}

        {/* Quick Domain Presets & Source Limiter Bar */}
        <div
          style={{
            display: 'flex',
            alignItems: 'center',
            justifyContent: 'space-between',
            gap: 10,
            flexWrap: 'wrap',
            padding: '10px 14px',
            background: 'var(--cockpit-card)',
            border: '1px solid var(--cockpit-border)',
            borderRadius: 'var(--radius-md)',
          }}
        >
          {/* Quick Domain Pills */}
          <div style={{ display: 'flex', alignItems: 'center', gap: 6, flexWrap: 'wrap', flex: 1 }}>
            <span style={{ fontSize: 11.5, fontWeight: 700, color: 'var(--text-dim)', textTransform: 'uppercase' }}>
              Discipline:
            </span>
            <button
              type="button"
              className={`quick-pill ${searchScope === 'default' && customSources.length === 0 ? 'active' : ''}`}
              onClick={() => handleScopeChange('default')}
              title="Use the default source selection from Settings"
            >
              Default
            </button>
            {QUICK_DISCIPLINES.slice(0, 6).map((preset) => (
              <button
                key={preset.id}
                type="button"
                className={`quick-pill ${searchScope === preset.id && customSources.length === 0 ? 'active' : ''}`}
                onClick={() => handleScopeChange(preset.id)}
                title={preset.description}
              >
                <span>{preset.label}</span>
              </button>
            ))}
          </div>

          {/* Source Limiter Popup Button */}
          <div style={{ display: 'flex', gap: 8, alignItems: 'center' }}>
            <button
              type="button"
              className={`action-btn ${customSources.length > 0 ? 'action-btn-primary' : ''}`}
              onClick={() => setSourceLimiterOpen(true)}
              title={`Open the limiter to customise the ${AVAILABLE_SOURCE_IDS.size} active sources`}
              style={{ fontSize: 12, padding: '5px 12px' }}
            >
              <Filter size={13} />
              <span>
                {customSources.length > 0
                  ? `Custom (${customSources.length} sources)`
                  : currentPresetMeta
                  ? `Sources: ${currentPresetMeta.label}`
                  : `Limit sources (${AVAILABLE_SOURCE_IDS.size})`}
              </span>
            </button>
          </div>
        </div>

        {/* Modern Minimalist Filter Details Bar */}
        <details ref={searchOptionsRef} id="search-options" className="compact-options">
          <summary style={{ display: 'flex', alignItems: 'center', gap: 6 }}>
            <SlidersHorizontal size={14} style={{ color: 'var(--primary-cyan)' }} />
            <span>
              Advanced filters · {currentPresetMeta?.label || 'Default'} ·{' '}
              {yearMin || yearMax ? `${yearMin || '…'}–${yearMax || '…'}` : 'All years'}
              {customSources.length > 0 ? ` · ${customSources.length} custom sources` : ''}
            </span>
          </summary>
          <div className="compact-options-body">
            <section className="discipline-picker" aria-labelledby="discipline-heading">
              <div>
                <h2 id="discipline-heading">8 standardised discipline groups & 4 modes</h2>
                <p>Pick a discipline group to tune the sources. Your keywords are left untouched.</p>
              </div>
              <div className="discipline-grid" role="group" aria-label="Discipline selection grid">
                {QUICK_DISCIPLINES.map((preset) => (
                  <button
                    id={`discipline-${preset.id}`}
                    key={preset.id}
                    type="button"
                    className={`discipline-card ${searchScope === preset.id && customSources.length === 0 ? 'active' : ''}`}
                    aria-pressed={searchScope === preset.id}
                    title={preset.description}
                    onClick={() => handleScopeChange(preset.id)}
                  >
                    <span>{preset.label}</span>
                    <small>
                      {preset.sources.length} sources{' '}
                      {searchScope === preset.id && customSources.length === 0 && (
                        <Check size={13} aria-hidden="true" />
                      )}
                    </small>
                  </button>
                ))}
              </div>
            </section>

            <section
              aria-labelledby="mesh-builder-heading"
              style={{ border: '1px solid var(--cockpit-border)', borderRadius: 10, padding: 14 }}
            >
              <div style={{ display: 'flex', justifyContent: 'space-between', gap: 12, flexWrap: 'wrap' }}>
                <div>
                  <h2 id="mesh-builder-heading" style={{ margin: 0, fontSize: 14 }}>MeSH & Boolean</h2>
                  <p style={{ margin: '4px 0 0', color: 'var(--text-muted)', fontSize: 12 }}>
                    Each line is a synonym (OR); groups combine with AND/OR. PubMed uses raw MeSH,
                    and other sources are translated into their own syntax automatically.
                  </p>
                </div>
                <label style={{ display: 'inline-flex', alignItems: 'center', gap: 6, fontSize: 12 }}>
                  <input
                    id="mesh-explode"
                    type="checkbox"
                    checked={meshExplode}
                    onChange={(event) => setMeshExplode(event.target.checked)}
                  />
                  Explode the MeSH tree
                </label>
              </div>

              <div style={{ display: 'grid', gap: 8, marginTop: 12 }}>
                {meshGroups.map((group, index) => (
                  <div key={index} style={{ display: 'grid', gridTemplateColumns: 'minmax(180px, 1fr) 170px auto', gap: 8 }}>
                    <textarea
                      id={`mesh-group-${index}`}
                      aria-label={`Concept group ${index + 1}`}
                      className="field-input"
                      rows={2}
                      placeholder={index === 0 ? 'heart failure\ncardiac failure' : 'drug therapy\ntreatment'}
                      value={group.terms}
                      onChange={(event) => setMeshGroups((current) => current.map((item, itemIndex) =>
                        itemIndex === index ? { ...item, terms: event.target.value } : item
                      ))}
                    />
                    <select
                      aria-label={`Search field for group ${index + 1}`}
                      className="field-input"
                      value={group.field}
                      onChange={(event) => setMeshGroups((current) => current.map((item, itemIndex) =>
                        itemIndex === index ? { ...item, field: event.target.value as MeshField } : item
                      ))}
                    >
                      <option value="mh">MeSH Terms</option>
                      <option value="majr">MeSH Major Topic</option>
                      <option value="tiab">Title / Abstract</option>
                      <option value="ti">Title</option>
                      <option value="all">All Fields</option>
                    </select>
                    <button
                      type="button"
                      className="action-btn"
                      aria-label={`Remove group ${index + 1}`}
                      disabled={meshGroups.length === 1}
                      onClick={() => setMeshGroups((current) => current.filter((_, itemIndex) => itemIndex !== index))}
                    >
                      <X size={14} />
                    </button>
                  </div>
                ))}
              </div>

              <div style={{ display: 'flex', gap: 8, alignItems: 'center', flexWrap: 'wrap', marginTop: 8 }}>
                <button
                  id="mesh-add-group"
                  type="button"
                  className="action-btn"
                  onClick={() => setMeshGroups((current) => [...current, { terms: '', field: 'tiab' }])}
                >
                  Add group
                </button>
                <label htmlFor="mesh-operator" style={{ fontSize: 12, color: 'var(--text-muted)' }}>Combine groups</label>
                <select
                  id="mesh-operator"
                  className="field-input"
                  style={{ width: 80 }}
                  value={meshOperator}
                  onChange={(event) => setMeshOperator(event.target.value as 'AND' | 'OR')}
                >
                  <option value="AND">AND</option>
                  <option value="OR">OR</option>
                </select>
                <input
                  id="mesh-exclusions"
                  className="field-input"
                  style={{ flex: 1, minWidth: 190 }}
                  placeholder="Exclusions (one per line, or separated by |)"
                  value={meshExclusions}
                  onChange={(event) => setMeshExclusions(event.target.value)}
                />
              </div>

              {generatedMeshQuery && (
                <div style={{ marginTop: 10, display: 'grid', gap: 8 }}>
                  <code
                    id="mesh-query-preview"
                    style={{ whiteSpace: 'pre-wrap', overflowWrap: 'anywhere', padding: 10, borderRadius: 8, background: 'var(--cockpit-card)' }}
                  >
                    {generatedMeshQuery}
                  </code>
                  <div aria-label="Syntax compatibility per source" style={{ display: 'flex', gap: 6, flexWrap: 'wrap', fontSize: 11 }}>
                    {([
                      ['pubmed_mesh', 'Raw MeSH'],
                      ['europe_pmc', 'Europe PMC translation'],
                      ['arxiv', 'arXiv translation'],
                      ['native_boolean', 'Raw Boolean'],
                      ['plain_keywords', 'Safe keywords'],
                    ] as const).map(([mode, label]) => {
                      const matching = queryPreview.filter((item) => item.mode === mode);
                      return matching.length ? (
                        <span key={mode} className="quick-pill" title={matching.map((item) => item.id).join(', ')}>
                          {label}: {matching.length}
                        </span>
                      ) : null;
                    })}
                  </div>
                  <button
                    id="mesh-apply-search"
                    type="button"
                    className="search-submit-btn"
                    disabled={loading}
                    onClick={applyMeshSearch}
                    style={{ justifySelf: 'start' }}
                  >
                    <Search size={14} /> Apply & search
                  </button>
                </div>
              )}
            </section>

            <div
              style={{
                display: 'flex',
                alignItems: 'center',
                justifyContent: 'space-between',
                gap: 12,
                flexWrap: 'wrap',
              }}
            >
              <div style={{ display: 'flex', alignItems: 'center', gap: 8, flexWrap: 'wrap' }}>
                <div
                  style={{
                    display: 'flex',
                    alignItems: 'center',
                    gap: 6,
                    color: 'var(--text-muted)',
                    fontSize: 12,
                    fontWeight: 600,
                  }}
                >
                  <Layers size={14} style={{ color: 'var(--primary-cyan)' }} />
                  <label htmlFor="search-scope">All source groups</label>
                </div>
                <select
                  id="search-scope"
                  aria-label="Search scope"
                  className="field-input"
                  style={{ width: 'auto', maxWidth: 220, fontSize: 12, padding: '5px 10px' }}
                  value={searchScope}
                  onChange={(e) => handleScopeChange(e.target.value)}
                >
                  {SCOPES.map((scope) => (
                    <option key={scope.id} value={scope.id}>
                      {scope.label}
                    </option>
                  ))}
                </select>

                <div style={{ width: 1, height: 16, background: 'var(--cockpit-border)', margin: '0 4px' }} />

                <div
                  style={{
                    display: 'flex',
                    alignItems: 'center',
                    gap: 6,
                    color: 'var(--text-muted)',
                    fontSize: 12,
                    fontWeight: 600,
                  }}
                >
                  <Calendar size={14} style={{ color: 'var(--primary-cyan)' }} />
                  <span>Year</span>
                </div>
                <div className="quick-pill-group">
                  <button
                    type="button"
                    className={`quick-pill ${!yearMin && !yearMax ? 'active' : ''}`}
                    onClick={() => {
                      setYearMin('');
                      setYearMax('');
                    }}
                  >
                    All
                  </button>
                  <button
                    type="button"
                    className={`quick-pill ${
                      yearMin === String(new Date().getFullYear() - 5) && !yearMax ? 'active' : ''
                    }`}
                    onClick={() => {
                      setYearMin(String(new Date().getFullYear() - 5));
                      setYearMax('');
                    }}
                  >
                    Last 5 years
                  </button>
                  <button
                    type="button"
                    className={`quick-pill ${
                      yearMin === String(new Date().getFullYear() - 3) && !yearMax ? 'active' : ''
                    }`}
                    onClick={() => {
                      setYearMin(String(new Date().getFullYear() - 3));
                      setYearMax('');
                    }}
                  >
                    Last 3 years
                  </button>
                  <button
                    type="button"
                    className={`quick-pill ${
                      yearMin === String(new Date().getFullYear()) && !yearMax ? 'active' : ''
                    }`}
                    onClick={() => {
                      setYearMin(String(new Date().getFullYear()));
                      setYearMax('');
                    }}
                  >
                    This year
                  </button>
                </div>

                <div style={{ display: 'inline-flex', alignItems: 'center', gap: 4, marginLeft: 2 }}>
                  <input
                    id="search-year-min"
                    aria-label="From year"
                    className="field-input"
                    type="number"
                    min="1000"
                    max="9999"
                    placeholder="From"
                    style={{ width: 68, fontSize: 11.5, padding: '4px 6px' }}
                    value={yearMin}
                    onChange={(e) => setYearMin(e.target.value)}
                  />
                  <span style={{ color: 'var(--text-dim)', fontSize: 11 }}>-</span>
                  <input
                    id="search-year-max"
                    aria-label="To year"
                    className="field-input"
                    type="number"
                    min="1000"
                    max="9999"
                    placeholder="To"
                    style={{ width: 68, fontSize: 11.5, padding: '4px 6px' }}
                    value={yearMax}
                    onChange={(e) => setYearMax(e.target.value)}
                  />
                </div>
              </div>

              <div style={{ display: 'flex', alignItems: 'center', gap: 6 }}>
                <span style={{ fontSize: 11.5, color: 'var(--text-muted)' }}>Result limit:</span>
                <input
                  id="search-limit"
                  aria-label="Result limit"
                  className="field-input"
                  type="number"
                  min="1"
                  max="50"
                  placeholder="20"
                  style={{ width: 56, fontSize: 11.5, padding: '4px 6px', textAlign: 'center' }}
                  value={resultLimit}
                  onChange={(e) => setResultLimit(e.target.value)}
                />
              </div>
            </div>

            <div className="search-options-footer">
              <p>
                {query.trim()
                  ? 'Filters apply as soon as you press Search.'
                  : 'Type a keyword in the search bar above to begin.'}
              </p>
              <button
                id="search-reset-options"
                type="button"
                className="action-btn"
                onClick={() => {
                  setSearchScope('default');
                  setCustomSources([]);
                  setYearMin('');
                  setYearMax('');
                  setResultLimit('');
                }}
              >
                Reset filters
              </button>
              <button
                id="search-apply-options"
                type="button"
                className="search-submit-btn"
                disabled={loading || !query.trim()}
                onClick={submitSearch}
              >
                <Search size={14} /> {loading ? 'Searching…' : 'Search with filters'}
              </button>
            </div>
          </div>
        </details>
      </div>

      {error && (
        <div className="alert alert-danger">
          <AlertTriangle size={16} style={{ flexShrink: 0, marginTop: 1 }} />
          <div>{error}</div>
        </div>
      )}

      {downloadError && (
        <div className="alert alert-warning">
          <AlertTriangle size={16} style={{ flexShrink: 0, marginTop: 1 }} />
          <div style={{ display: 'flex', alignItems: 'center', gap: 12, flexWrap: 'wrap' }}>
            <span>{downloadError.message}</span>
            {downloadError.paper && originalPaperUrl(downloadError.paper) && (
              <a
                className="action-btn"
                href={originalPaperUrl(downloadError.paper)!}
                target="_blank"
                rel="noreferrer"
                style={{ textDecoration: 'none' }}
              >
                <ExternalLink size={13} />
                <span>Open the article page</span>
              </a>
            )}
          </div>
        </div>
      )}

      {/* ---------------- Result toolbar ---------------- */}
      {results && !loading && (
        <div
          style={{
            display: 'flex',
            alignItems: 'center',
            justifyContent: 'space-between',
            gap: 12,
            flexWrap: 'wrap',
            borderBottom: '1px solid var(--cockpit-border)',
            paddingBottom: 12,
          }}
        >
          <div className="segmented source-group-filter" role="tablist" aria-label="Filter by source group or interest list">
            <button
              id="source-group-all"
              type="button"
              role="tab"
              aria-selected={sourceFilter === 'all'}
              className={`segmented-item ${sourceFilter === 'all' ? 'active' : ''}`}
              onClick={() => setSourceFilter('all')}
            >
              <span>All</span>
              <span className="segmented-count">{allPapers.length}</span>
            </button>

            {interestedCount > 0 && (
              <button
                id="source-group-interested"
                type="button"
                role="tab"
                aria-selected={sourceFilter === 'interested'}
                className={`segmented-item ${sourceFilter === 'interested' ? 'active' : ''}`}
                onClick={() => setSourceFilter('interested')}
              >
                <span>Interest</span>
                <span className="segmented-count">{interestedCount}</span>
              </button>
            )}

            {Object.entries(SOURCE_GROUPS)
              .filter(([id]) => sourceCounts[id as SourceGroup] > 0 || sourceFilter === id)
              .map(([id, meta]) => (
                <button
                  key={id}
                  id={`source-group-${id}`}
                  type="button"
                  role="tab"
                  aria-selected={sourceFilter === id}
                  className={`segmented-item ${sourceFilter === id ? 'active' : ''}`}
                  onClick={() => setSourceFilter(id as SourceGroup)}
                >
                  <span>{meta.shortLabel}</span>
                  <span className="segmented-count">{sourceCounts[id as SourceGroup]}</span>
                </button>
              ))}
          </div>

          <div
            style={{
              display: 'flex',
              alignItems: 'center',
              gap: 14,
              fontSize: 12,
              color: 'var(--text-muted)',
              flexWrap: 'wrap',
            }}
          >
            <label style={{ display: 'flex', alignItems: 'center', gap: 6, cursor: 'pointer' }}>
              <input
                id="filter-open-access"
                type="checkbox"
                checked={oaOnly}
                onChange={(e) => handleOaToggle(e.target.checked)}
                style={{ accentColor: 'var(--primary-cyan)' }}
              />
              <span>Open access / has PDF</span>
              {oaOnly && hiddenByOa > 0 && (
                <span style={{ color: 'var(--text-dim)' }}>(hidden {hiddenByOa})</span>
              )}
            </label>

            <label style={{ display: 'flex', alignItems: 'center', gap: 6, cursor: 'pointer' }}>
              <input
                id="filter-recommended-pdf"
                type="checkbox"
                checked={recommendedPdfOnly}
                onChange={(e) => setRecommendedPdfOnly(e.target.checked)}
                style={{ accentColor: 'var(--primary-cyan)' }}
              />
              <span>Prefer PDF</span>
            </label>

            <label style={{ display: 'flex', alignItems: 'center', gap: 6 }}>
              <span>Kind</span>
              <select
                id="filter-kind"
                className="field-input"
                style={{ width: 'auto', padding: '4px 8px', fontSize: 12 }}
                value={kindFilter}
                onChange={(e) => setKindFilter(e.target.value as 'all' | PaperKind)}
              >
                <option value="all">All kinds</option>
                {(Object.keys(KIND_META) as PaperKind[]).map((kind) => (
                  <option key={kind} value={kind}>
                    {KIND_META[kind].label}
                  </option>
                ))}
              </select>
            </label>

            <label style={{ display: 'flex', alignItems: 'center', gap: 6 }}>
              <span>Sort</span>
              <select
                id="sort-results"
                className="field-input"
                style={{ width: 'auto', padding: '4px 8px', fontSize: 12 }}
                value={sortKey}
                onChange={(e) => setSortKey(e.target.value as SortKey)}
              >
                <option value="relevance">Relevance (RRF)</option>
                <option value="evaluation">Screening score</option>
                <option value="pdf">PDF availability</option>
                <option value="year">Most recent publication year</option>
                <option value="citations">Citation count</option>
              </select>
            </label>

            <span style={{ fontFamily: 'var(--font-mono)', fontSize: 11, color: 'var(--text-dim)' }}>
              {results.total}
              {availableTotal > results.total ? ` / ${availableTotal}` : ''} results
              {' · '}
              {results.elapsed_ms >= 1000 ? `${(results.elapsed_ms / 1000).toFixed(1)}s` : `${results.elapsed_ms}ms`}
            </span>
            {canLoadMore && (
              <button
                id="search-load-more"
                type="button"
                className="action-btn"
                onClick={() => void handleLoadMore()}
                disabled={loadingMore}
                title="Load more results"
                style={{ padding: '3px 10px', fontSize: 11 }}
              >
                {loadingMore ? <Loader2 size={12} className="animate-spin" /> : null}
                {loadingMore ? 'Loading…' : 'Load more'}
              </button>
            )}
          </div>
        </div>
      )}

      {/* -------- Selection & reference export -------- */}
      {results && !loading && displayedPapers.length > 0 && (
        <div
          style={{
            display: 'flex',
            alignItems: 'center',
            gap: 12,
            flexWrap: 'wrap',
            padding: '8px 12px',
            border: '1px solid var(--cockpit-border)',
            borderRadius: 'var(--radius-sm)',
            background: 'var(--cockpit-card)',
            fontSize: 12,
            // Keep the export actions reachable while scrolling through ticked results.
            position: 'sticky',
            top: 0,
            zIndex: 5,
          }}
        >
          <label style={{ display: 'flex', alignItems: 'center', gap: 8, cursor: 'pointer' }}>
            <input
              id="select-all-results"
              type="checkbox"
              checked={selection.allVisibleSelected}
              ref={(node) => {
                if (node) {
                  node.indeterminate =
                    selection.visibleSelectedCount > 0 && !selection.allVisibleSelected;
                }
              }}
              onChange={selection.toggleAllVisible}
              aria-label="Select all shown results"
              style={{ accentColor: 'var(--primary-cyan)' }}
            />
            <span>Select all ({displayedPapers.length})</span>
          </label>

          {selection.count > 0 ? (
            <>
              <span style={{ color: 'var(--primary-cyan)', fontWeight: 600 }}>
                {selection.count} selected
              </span>
              <button
                id="export-selected-ris"
                type="button"
                className="action-btn"
                onClick={exportSelectedRis}
                title="RIS is the shared import format of Zotero, EndNote and Mendeley"
                style={{ padding: '4px 10px' }}
              >
                Export .RIS — Zotero / EndNote
              </button>
              <button
                id="export-selected-bibtex"
                type="button"
                className="action-btn"
                onClick={exportSelectedBibtex}
                style={{ padding: '4px 10px' }}
              >
                Export BibTeX
              </button>
              <button
                type="button"
                className="action-btn"
                onClick={() => {
                  setAgentExportPapers(selectedPapers);
                  setAgentExportModalOpen(true);
                }}
                style={{ padding: '4px 10px' }}
              >
                <Bot size={13} /> Send to AI agent
              </button>
              <button type="button" className="action-btn" onClick={selection.clear} style={{ padding: '4px 10px' }}>
                Clear
              </button>
            </>
          ) : (
            <span style={{ color: 'var(--text-dim)' }}>
              Tick results to export them as references, without saving them first.
            </span>
          )}
        </div>
      )}

      {/* ---------------- Source health ---------------- */}
      {results?.sources && results.sources.length > 0 && !loading && (() => {
        const queried = results.sources.filter((s) => s.queried);
        const needsSetup = queried.filter((s) => !s.ok && s.needs_setup);
        const cooling = queried.filter((s) => !s.ok && s.cooling_down);
        const failed = queried.filter((s) => !s.ok && !s.needs_setup && !s.cooling_down);
        const reachable = queried.length - needsSetup.length - cooling.length;
        return (
          <div style={{ display: 'flex', flexDirection: 'column', gap: 10 }}>
            {failed.length > 0 && (
              <div className="alert alert-warning">
                <AlertTriangle size={16} style={{ flexShrink: 0, marginTop: 1 }} />
                <div>
                  <div className="alert-title">
                    {failed.length}/{reachable} sources unresponsive — results may be partial
                  </div>
                  <div>
                    {failed.map((s) => sourceMessage(s.name, s.error)).join(' • ')}
                  </div>
                </div>
              </div>
            )}

            {cooling.length > 0 && (
              <div className="alert" style={{ alignItems: 'flex-start' }}>
                <PauseCircle size={16} style={{ flexShrink: 0, marginTop: 1 }} />
                <div>
                  <div className="alert-title">
                    {cooling.length} {cooling.length === 1 ? 'source is' : 'sources are'} paused after repeated failures
                  </div>
                  <div>
                    {cooling.map((s) => sourceMessage(s.name, s.error, 'cooling down')).join(' • ')}
                  </div>
                </div>
              </div>
            )}

            {needsSetup.length > 0 && (
              <div className="alert" style={{ alignItems: 'flex-start' }}>
                <Settings size={16} style={{ flexShrink: 0, marginTop: 1 }} />
                <div>
                  <div className="alert-title">
                    {needsSetup.length} {needsSetup.length === 1 ? 'source needs' : 'sources need'} setup — skipped, not failed
                  </div>
                  <div>
                    {needsSetup.map((s) => s.name).join(' • ')} — add the key or sign in from Settings.
                  </div>
                </div>
              </div>
            )}

            <div style={{ display: 'flex', flexWrap: 'wrap', gap: 6, alignItems: 'center' }}>
              <span style={{ fontSize: 11, color: 'var(--text-dim)', fontWeight: 600 }}>
                ACTIVE SOURCES ({queried.filter((s) => s.count > 0).length}/{reachable}):
              </span>
              {results.sources
                .filter((s) => s.queried && (s.count > 0 || (!s.ok && !s.needs_setup && !s.cooling_down)))
                .map((s) => {
                  const tone = !s.ok
                    ? { color: 'var(--status-rose)', bg: 'var(--status-rose-bg)', border: 'var(--status-rose-border)' }
                    : { color: 'var(--status-emerald)', bg: 'var(--status-emerald-bg)', border: 'var(--status-emerald-border)' };
                  const Icon = s.ok ? CheckCircle2 : XCircle;
                  return (
                    <span
                      key={s.id}
                      title={s.ok ? `${s.count} results` : s.error || 'Unknown error'}
                      style={{
                        display: 'inline-flex',
                        alignItems: 'center',
                        gap: 5,
                        padding: '3px 9px',
                        borderRadius: 20,
                        fontSize: 11,
                        color: tone.color,
                        background: tone.bg,
                        border: `1px solid ${tone.border}`,
                      }}
                    >
                      <Icon size={12} />
                      <span>{s.name}</span>
                      {s.ok && <b style={{ fontFamily: 'var(--font-mono)' }}>{s.count}</b>}
                    </span>
                  );
                })}
            </div>
          </div>
        );
      })()}

      {/* ---------------- Results List ---------------- */}
      <div className="paper-list">
        {loading && (
          <div style={{ display: 'flex', flexDirection: 'column', gap: 14 }}>
            <SearchScanner query={query || 'All topics'} scope={searchScope} />
            {[0, 1].map((i) => (
              <div key={i} className="cockpit-card" style={{ padding: 16, opacity: 0.6 }}>
                <div className="skeleton" style={{ width: 140, height: 12, marginBottom: 10 }} />
                <div className="skeleton" style={{ width: '80%', height: 16, marginBottom: 8 }} />
                <div className="skeleton" style={{ width: '50%', height: 12, marginBottom: 12 }} />
                <div className="skeleton" style={{ width: '100%', height: 36 }} />
              </div>
            ))}
          </div>
        )}

        {!loading && !results && !error && (
          <div className="empty-state">
            <div className="empty-state-icon">
              <Compass size={24} />
            </div>
            <div className="empty-state-title">Explore academic research papers</div>
            <div className="empty-state-text">
              Type a keyword, paper title or DOI above. You can pick a discipline quickly, or open <b>Limit sources</b> to narrow the search.
            </div>
          </div>
        )}

        {!loading && results && displayedPapers.length === 0 && (
          <div className="empty-state">
            <div className="empty-state-icon">
              <Search size={24} />
            </div>
            <div className="empty-state-title">No papers match the current filters</div>
            <div className="empty-state-text">
              {oaOnly
                ? 'Try clearing "Open access / has PDF", switching discipline, or broadening your keywords.'
                : 'Try the "All" tab, a different discipline group, or broader keywords.'}
            </div>
          </div>
        )}

        {!loading &&
          displayedPapers.map((paper) => (
            <PaperCard
              key={paper.id}
              paper={paper}
              isExpanded={!!expandedAbstracts[paper.id]}
              isSaved={savedPaperIds.has(paper.id)}
              isCopied={copiedId === `${paper.id}:apa`}
              isCopiedBib={copiedId === `${paper.id}:bibtex`}
              isDownloading={downloadingId === paper.id}
              isDownloaded={downloadSuccessId === paper.id}
              isSelected={selectedId === paper.id}
              isCitationOpen={!!citationOpen[paper.id]}
              isChecked={selection.isSelected(paper.id)}
              onToggleChecked={selection.toggle}
              citationState={citations[paper.id]}
              savedPaperIds={savedPaperIds}
              detailRef={detailRef}
              detailTrigger={detailTrigger}
              onSelect={setSelectedId}
              onCloseDetails={closeDetails}
              onToggleAbstract={toggleAbstract}
              onSavePaper={onSavePaper}
              onCopyCitation={copyCitation}
              onToggleCitations={toggleCitations}
              onLoadCitations={loadCitations}
              onDownload={handleDownload}
              onOpenFulltext={openFulltextReader}
              onOpenAgentExport={openAgentExport}
            />
          ))}
      </div>

      {/* ------- Secondary analysis, below the results it describes ------- */}
      {results && !loading && results.papers.length > 0 && (
        <>
          <EvidenceSynthesis query={results.query} papers={results.papers} />
          <ResearchGapPanel query={query} papers={results.papers} />
        </>
      )}

      {/* ---------------- Modals & Drawers ---------------- */}
      <SourceLimiterModal
        isOpen={sourceLimiterOpen}
        onClose={() => setSourceLimiterOpen(false)}
        activeScope={searchScope}
        selectedSources={customSources}
        onApplySources={handleApplyCustomSources}
      />

      <FulltextViewerModal
        isOpen={readerOpen}
        onClose={() => setReaderOpen(false)}
        paper={readerPaper}
        isSaved={readerPaper ? savedPaperIds.has(readerPaper.id) : false}
        isFavorite={readerPaper ? savedPaperIds.has(readerPaper.id) : false}
        onSavePaper={onSavePaper}
        onToggleFavorite={onSavePaper}
        onDownloadPdf={handleDownload}
        isDownloadingPdf={readerPaper ? downloadingId === readerPaper.id : false}
        isDownloadedPdf={readerPaper ? downloadSuccessId === readerPaper.id : false}
      />

      <AiAgentExportModal
        isOpen={agentExportModalOpen}
        onClose={() => setAgentExportModalOpen(false)}
        papers={agentExportPapers}
        workspaceName="ScholarGate Discovery"
      />
    </div>
  );
};
