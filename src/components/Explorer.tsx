import React, { useState, useEffect, useCallback, useMemo, useRef } from 'react';
import {
  Search,
  Download,
  Bookmark,
  Copy,
  ExternalLink,
  ChevronDown,
  ChevronUp,
  Check,
  Sparkles,
  AlertTriangle,
  Compass,
  Loader2,
  CheckCircle2,
  XCircle,
  Award,
  FileCheck2,
  GitBranch,
  Calendar,
  Layers,
  PauseCircle,
  Settings,
  X,
} from 'lucide-react';
import { Paper, SearchResponse } from '../types';
import { SearchScanner } from './SearchScanner';
import { ResearchGapPanel } from './ResearchGapPanel';
import { EvidenceSynthesis } from './EvidenceSynthesis';
import { evaluatePaper, getSourceGroup, SOURCE_GROUPS, SourceGroup } from '../lib/paperEvaluation';
import { apaCitation, bibtexCitation } from '../lib/citation';
import { getPaperKind, KIND_META, PaperKind } from '../lib/paperKind';
import { gatewayFetch } from '../lib/gateway';

import searchCatalog from '../lib/searchCatalog.json';

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
type SourceFilter = 'all' | SourceGroup;
type SortKey = 'relevance' | 'evaluation' | 'pdf' | 'year' | 'citations';
type CitationDirection = 'cited_by' | 'references' | 'related';

interface CitationState {
  direction: CitationDirection;
  loading: boolean;
  error: string | null;
  items: Paper[];
}

// Only presets backed by at least one supported source are selectable.
const AVAILABLE_PRESETS = searchCatalog.presets.filter((p) => p.sources.length > 0);
const SCOPES = [
  { id: 'default', label: 'Per Settings', title: 'Use domains and sources configured in Settings' },
  ...AVAILABLE_PRESETS.map((p) => ({ id: p.id, label: p.label, title: p.description })),
];
const QUICK_DISCIPLINES = ['cs_ai', 'engineering', 'medical', 'natural_sciences', 'economics', 'social_sciences', 'education', 'law', 'environment', 'agriculture', 'vietnam_academic', 'multidisciplinary']
  .map(id => AVAILABLE_PRESETS.find(p => p.id === id)!)
  .filter(Boolean);

// Vietnam papers indexed through the national repository filter
export const isVietnamPaper = (paper: Paper): boolean => getSourceGroup(paper) === 'vietnam';

const ABSTRACT_CLAMP = 280;

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
  /** Active research workspace; queries and downloads are logged against it. */
  workspaceId?: string;
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
  workspaceId,
}) => {
  const [localQuery, setQuery] = useState(initialQuery || '');
  const query = draftQuery ?? localQuery;
  const [loading, setLoading] = useState(false);
  const [error, setError] = useState<string | null>(null);
  const [results, setResults] = useState<SearchResponse | null>(null);
  const [selectedId, setSelectedId] = useState<string | null>(null);
  const detailRef = useRef<HTMLElement>(null);
  const detailTrigger = useRef<HTMLButtonElement | null>(null);
  const closeDetails = () => {
    setSelectedId(null);
    detailTrigger.current?.focus();
  };
  useEffect(() => {
    if (selectedId) detailRef.current?.focus();
  }, [selectedId]);
  const [expandedAbstracts, setExpandedAbstracts] = useState<Record<string, boolean>>({});
  const [copiedId, setCopiedId] = useState<string | null>(null);
  const [downloadingId, setDownloadingId] = useState<string | null>(null);
  const [downloadSuccessId, setDownloadSuccessId] = useState<string | null>(null);
  const [downloadError, setDownloadError] = useState<string | null>(null);
  const [oaOnly, setOaOnly] = useState(false);
  const [recommendedPdfOnly, setRecommendedPdfOnly] = useState(false);
  const [sortKey, setSortKey] = useState<SortKey>('relevance');
  const [citationOpen, setCitationOpen] = useState<Record<string, boolean>>({});
  const [citations, setCitations] = useState<Record<string, CitationState>>({});

  const [sourceFilter, setSourceFilter] = useState<SourceFilter>('all');
  const [kindFilter, setKindFilter] = useState<'all' | PaperKind>('all');
  const [searchScope, setSearchScope] = useState<Scope>('default');
  const [yearMin, setYearMin] = useState('');
  const [yearMax, setYearMax] = useState('');
  const [resultLimit, setResultLimit] = useState('');
  const searchOptionsRef = useRef<HTMLDetailsElement>(null);
  const latestSearchId = useRef(0);
  const completedSearch = useRef<{ id: number; body: string; offset: number } | null>(null);
  const loadingMoreRequest = useRef<number | null>(null);
  const [loadingMore, setLoadingMore] = useState(false);

  useEffect(() => () => { ++latestSearchId.current; }, []);

  const runSearch = useCallback(
    async (searchQuery: string, scope: Scope, openAccessOnly = oaOnly, limitOverride?: number) => {
      const q = searchQuery.trim();
      if (!q) {
        setError('Please enter a query, author name, or DOI before searching.');
        return;
      }
      if ((yearMin && !/^\d{4}$/.test(yearMin)) || (yearMax && !/^\d{4}$/.test(yearMax)) ||
          (yearMin && yearMax && Number(yearMin) > Number(yearMax)) ||
          (resultLimit && (!Number.isInteger(Number(resultLimit)) || Number(resultLimit) < 1 || Number(resultLimit) > 50))) {
        setError('Please enter valid 4-digit years (start year ≤ end year) and a limit between 1 and 50.');
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
        const body = JSON.stringify({
          query: q,
          limit: limitOverride ?? (resultLimit ? Number(resultLimit) : undefined),
          year_min: yearMin ? Number(yearMin) : undefined,
          year_max: yearMax ? Number(yearMax) : undefined,
          open_access_only: openAccessOnly,
          sources: scope === 'default' ? undefined : [scope],
          workspace_id: workspaceId || undefined,
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
            ? `Cannot connect to local gateway at 127.0.0.1:${port}. Please check if the gateway is running.`
            : (err as Error).message || 'Search failed.'
        );
      } finally {
        if (requestId === latestSearchId.current) setLoading(false);
      }
    },
    [port, yearMin, yearMax, resultLimit, oaOnly, workspaceId]
  );

  // A query pushed in from another tab (dashboard, history) always re-runs.
  useEffect(() => {
    if (initialQuery && initialQuery.trim()) {
      setQuery(initialQuery);
      runSearch(initialQuery, searchScope);
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
  };

  const submitSearch = () => {
    if (onSubmitQuery) onSubmitQuery(query);
    else void runSearch(query, searchScope);
  };

  const toggleAbstract = (id: string) =>
    setExpandedAbstracts((prev) => ({ ...prev, [id]: !prev[id] }));

  const copyCitation = (paper: Paper, format: 'apa' | 'bibtex' = 'apa') => {
    const citation = format === 'bibtex' ? bibtexCitation(paper) : apaCitation(paper);
    navigator.clipboard.writeText(citation).then(() => {
      setCopiedId(`${paper.id}:${format}`);
      setTimeout(() => setCopiedId(null), 2000);
    }).catch(() => setError('Unable to copy citation to clipboard.'));
  };

  // OA filtering happens on the server so the returned page is a full page of
  // open-access papers instead of a client-side trim of an already-capped list.
  const handleOaToggle = (next: boolean) => {
    setOaOnly(next);
    if (query.trim()) runSearch(query, searchScope, next);
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
          [paper.id]: { direction, loading: false, error: null, items: Array.isArray(data?.items) ? data.items : [] },
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

  const toggleCitations = (paper: Paper) => {
    const opening = !citationOpen[paper.id];
    setCitationOpen((prev) => ({ ...prev, [paper.id]: opening }));
    if (opening && !citations[paper.id]) void loadCitations(paper, 'cited_by');
  };

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
      setError((e as Error).message || 'Unable to load more results.');
    } finally {
      if (requestId === latestSearchId.current) {
        loadingMoreRequest.current = null;
        setLoadingMore(false);
      }
    }
  }, []);

  const handleDownload = async (paper: Paper) => {
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
          workspace_id: workspaceId || undefined,
        }),
      });
      const json = await res.json().catch(() => null);
      if (!res.ok || (json && json.success === false)) {
        throw new Error(json?.error || `Gateway returned error code ${res.status}`);
      }
      setDownloadSuccessId(paper.id);
      setTimeout(() => setDownloadSuccessId(null), 3000);
    } catch (err) {
      setDownloadError(`PDF download failed: ${(err as Error).message}`);
      setTimeout(() => setDownloadError(null), 6000);
    } finally {
      setDownloadingId(null);
    }
  };

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

  const displayedPapers = useMemo(() => {
    const base = sourceFilter === 'all'
      ? allPapers
      : allPapers.filter((paper) => getSourceGroup(paper) === sourceFilter);
    if (sortKey === 'relevance') return base;
    return [...base].sort((a, b) => {
      if (sortKey === 'year') return (b.year || 0) - (a.year || 0);
      if (sortKey === 'citations') return (b.citations || 0) - (a.citations || 0);
      if (sortKey === 'pdf') return evaluatePaper(b).pdfScore - evaluatePaper(a).pdfScore;
      return evaluatePaper(b).overall - evaluatePaper(a).overall;
    });
  }, [sourceFilter, sortKey, allPapers]);

  const hiddenByOa = (results?.papers || []).filter((paper) => !(paper.open_access || paper.pdf_url)).length;
  const availableTotal = results?.available_total ?? results?.total ?? 0;
  const canLoadMore = !!results && availableTotal > (completedSearch.current?.offset ?? results.papers.length)
    && (completedSearch.current?.offset ?? 0) < 10_000;

  return (
    <div className="page-container">
      {/* ---------------- Search ---------------- */}
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
            placeholder="Search keywords, topics, DOI (10.xxx), or PMID… (⌘K)"
            value={query}
            onChange={(e) => {
              setQuery(e.target.value);
            }}
            aria-label="Search query"
          />
          <button id="paper-search-submit" type="submit" className="search-submit-btn" disabled={loading || !query.trim()}>
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

        {/* Modern Minimalist Filter & Scope Bar */}
        <details ref={searchOptionsRef} id="search-options" className="compact-options">
          <summary>Advanced filters · {SCOPES.find(scope => scope.id === searchScope)?.label || searchScope} · {yearMin || yearMax ? `${yearMin || '…'}–${yearMax || '…'}` : 'All years'}</summary>
          <div className="compact-options-body">
          <section className="discipline-picker" aria-labelledby="discipline-heading">
            <div>
              <h2 id="discipline-heading">Choose a discipline</h2>
              <p>Select a source group, then search. Your keywords stay unchanged.</p>
            </div>
            <div className="discipline-grid" role="group" aria-label="Quick discipline selection">
              {QUICK_DISCIPLINES.map(preset => (
                <button id={`discipline-${preset.id}`} key={preset.id} type="button"
                  className={`discipline-card ${searchScope === preset.id ? 'active' : ''}`}
                  aria-pressed={searchScope === preset.id} title={preset.description}
                  onClick={() => handleScopeChange(preset.id)}>
                  <span>{preset.label}</span>
                  <small>{preset.sources.length} sources {searchScope === preset.id && <Check size={13} aria-hidden="true" />}</small>
                </button>
              ))}
            </div>
          </section>
          <div style={{ display: 'flex', alignItems: 'center', justifyContent: 'space-between', gap: 12, flexWrap: 'wrap' }}>
            <div style={{ display: 'flex', alignItems: 'center', gap: 8, flexWrap: 'wrap' }}>
              <div style={{ display: 'flex', alignItems: 'center', gap: 6, color: 'var(--text-muted)', fontSize: 12, fontWeight: 600 }}>
                <Layers size={14} style={{ color: 'var(--primary-cyan)' }} />
                <label htmlFor="search-scope">All disciplines & collections</label>
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

              <div style={{ display: 'flex', alignItems: 'center', gap: 6, color: 'var(--text-muted)', fontSize: 12, fontWeight: 600 }}>
                <Calendar size={14} style={{ color: 'var(--primary-cyan)' }} />
                <span>Year</span>
              </div>
              <div className="quick-pill-group">
                <button
                  type="button"
                  className={`quick-pill ${!yearMin && !yearMax ? 'active' : ''}`}
                  onClick={() => { setYearMin(''); setYearMax(''); }}
                >
                  All
                </button>
                <button
                  type="button"
                  className={`quick-pill ${yearMin === String(new Date().getFullYear() - 5) && !yearMax ? 'active' : ''}`}
                  onClick={() => { setYearMin(String(new Date().getFullYear() - 5)); setYearMax(''); }}
                >
                  Past 5 Years
                </button>
                <button
                  type="button"
                  className={`quick-pill ${yearMin === String(new Date().getFullYear()) && !yearMax ? 'active' : ''}`}
                  onClick={() => { setYearMin(String(new Date().getFullYear())); setYearMax(''); }}
                >
                  This Year
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
              <span style={{ fontSize: 11.5, color: 'var(--text-muted)' }}>Limit:</span>
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
            <p>{query.trim() ? 'Filters apply when you search.' : 'Enter keywords in the search bar above to begin.'}</p>
            <button id="search-reset-options" type="button" className="action-btn"
              onClick={() => { setSearchScope('default'); setYearMin(''); setYearMax(''); setResultLimit(''); }}>Reset filters</button>
            <button id="search-apply-options" type="button" className="search-submit-btn" disabled={loading || !query.trim()}
              onClick={submitSearch}><Search size={14} /> {loading ? 'Searching…' : 'Search with filters'}</button>
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
          <div>{downloadError}</div>
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
          <div className="segmented source-group-filter" role="tablist" aria-label="Filter by source group">
            {([
              ['all', 'All', allPapers.length],
              ...Object.entries(SOURCE_GROUPS).map(([id, meta]) => [
                id,
                meta.shortLabel,
                sourceCounts[id as SourceGroup],
              ]),
            ] as [SourceFilter, string, number][])
              // An empty group cannot be selected usefully, so it only adds noise —
              // unless it is the filter the user is currently on.
              .filter(([id, , count]) => id === 'all' || count > 0 || sourceFilter === id)
              .map(([id, label, count]) => (
              <button
                key={id}
                id={`source-group-${id}`}
                type="button"
                role="tab"
                aria-selected={sourceFilter === id}
                className={`segmented-item ${sourceFilter === id ? 'active' : ''}`}
                onClick={() => setSourceFilter(id)}
              >
                <span>{label}</span>
                <span className="segmented-count">{count}</span>
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
              <span>Open Access / PDF only</span>
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
              <span>Recommended PDF only</span>
            </label>

            <label style={{ display: 'flex', alignItems: 'center', gap: 6 }}>
              <span>Type</span>
              <select
                id="filter-kind"
                className="field-input"
                style={{ width: 'auto', padding: '4px 8px', fontSize: 12 }}
                value={kindFilter}
                onChange={(e) => setKindFilter(e.target.value as 'all' | PaperKind)}
              >
                <option value="all">All Types</option>
                {(Object.keys(KIND_META) as PaperKind[]).map((kind) => (
                  <option key={kind} value={kind}>{KIND_META[kind].label}</option>
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
                <option value="evaluation">Screening Score</option>
                <option value="pdf">PDF Availability</option>
                <option value="year">Publication Year</option>
                <option value="citations">Citations</option>
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

      {/* ---------------- Source health ---------------- */}
      {results?.sources && results.sources.length > 0 && !loading && (() => {
        const queried = results.sources.filter((s) => s.queried);
        // A source still waiting for an API key or sign-in has not failed; it was
        // never set up. Counting it as unresponsive made every search look broken.
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

      {/* ---------------- Results ---------------- */}
      <div>
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
            <div className="empty-state-title">Find your next paper</div>
            <div className="empty-state-text">
              Enter keywords, a title or DOI above. Open Advanced filters to choose a discipline, then Search.
            </div>
          </div>
        )}

        {!loading && results && displayedPapers.length === 0 && (
          <div className="empty-state">
            <div className="empty-state-icon">
              <Search size={24} />
            </div>
            <div className="empty-state-title">No papers match current filters</div>
            <div className="empty-state-text">
              {oaOnly
                ? 'Try unchecking "Open Access / PDF only", switching to another category tab, or broadening your query.'
                : 'Try switching to the "All" tab, changing search scope, or using broader keywords.'}
            </div>
          </div>
        )}

        {!loading &&
          displayedPapers.map((paper) => {
            const isExpanded = !!expandedAbstracts[paper.id];
            const isSaved = savedPaperIds.has(paper.id);
            const isCopied = copiedId === `${paper.id}:apa`;
            const isCopiedBib = copiedId === `${paper.id}:bibtex`;
            const isDownloading = downloadingId === paper.id;
            const isDownloaded = downloadSuccessId === paper.id;
            const isVn = isVietnamPaper(paper);
            const kind = getPaperKind(paper);
            const sourceGroup = getSourceGroup(paper);
            const groupMeta = SOURCE_GROUPS[sourceGroup];
            const evaluation = evaluatePaper(paper);
            const abstract = paper.abstract || '';
            const needsClamp = abstract.length > ABSTRACT_CLAMP;

            return (
              <article key={paper.id} className={`paper-card compact-paper ${selectedId === paper.id ? 'selected' : ''}`}>
                <div className="paper-header">
                  <div style={{ flex: 1, minWidth: 0 }}>
                    <div className="paper-badges">
                      <span className="badge badge-source badge-essential">{paper.source}</span>
                      {kind !== 'article' && KIND_META[kind].badge && (
                        <span className={`badge badge-essential ${KIND_META[kind].badge}`}>{KIND_META[kind].label}</span>
                      )}
                      <span className="badge badge-group" title={groupMeta.label}>{groupMeta.shortLabel}</span>
                      {isVn && <span className="badge badge-vjol badge-essential">🇻🇳 VIETNAM</span>}
                      {paper.open_access && <span className="badge badge-oa badge-essential">OPEN ACCESS</span>}
                      {evaluation.recommendedPdf && (
                        <span className="badge badge-pdf-recommended"><FileCheck2 size={11} /> RECOMMENDED PDF {evaluation.pdfScore}</span>
                      )}
                      <span title="Heuristic screening aid for reading priority, not a study quality score" className={`badge evidence-${evaluation.label === 'Recommended' ? 'strong' : evaluation.label === 'Consider' ? 'fair' : 'review'}`}>
                        <Award size={11} /> SCREENING {evaluation.overall}/100
                      </span>
                      {paper.quartile && <span className="badge badge-q1">{paper.quartile}</span>}
                      {paper.score !== undefined && (
                        <span
                          className="badge"
                          title="Multi-source fused score (Reciprocal Rank Fusion)"
                          style={{
                            background: '#f1f5f9',
                            color: 'var(--text-muted)',
                            border: '1px solid var(--cockpit-border)',
                          }}
                        >
                          RRF {paper.score.toFixed(3)}
                        </span>
                      )}
                    </div>

                    <h3 className="paper-title">
                      <button className="paper-title-button" aria-expanded={selectedId === paper.id}
                        onClick={(event) => { detailTrigger.current = event.currentTarget; setSelectedId(paper.id); }}>
                        {paper.title}
                      </button>
                    </h3>
                  </div>

                  <button
                    className={`action-btn ${isSaved ? 'action-btn-primary' : ''}`}
                    onClick={() => onSavePaper(paper)}
                    title={isSaved ? 'Remove from workspace' : 'Save to workspace'}
                    style={{ flexShrink: 0 }}
                  >
                    <Bookmark size={14} fill={isSaved ? '#ffffff' : 'none'} />
                    <span>{isSaved ? 'Saved' : 'Save'}</span>
                  </button>
                  {(paper.source_url || paper.doi) && <a className="action-btn"
                    href={paper.source_url || `https://doi.org/${paper.doi}`} target="_blank" rel="noreferrer">Open</a>}
                </div>

                <div className="paper-meta">
                  <span>
                    {paper.authors.slice(0, 3).join(', ')}
                    {paper.authors.length > 3 ? ' et al.' : ''}
                  </span>
                  {paper.year && <span>• {paper.year}</span>}
                  {paper.venue && (
                    <span>
                      • <i>{paper.venue}</i>
                    </span>
                  )}
                  {selectedId === paper.id && paper.citations != null && (
                    <span>
                      • Citations: <b>{paper.citations}</b>
                    </span>
                  )}
                  {selectedId === paper.id && paper.doi && (
                    <span>
                      • DOI:{' '}
                      <a
                        href={`https://doi.org/${paper.doi}`}
                        target="_blank"
                        rel="noreferrer"
                        style={{ color: 'var(--primary-cyan)', textDecoration: 'none' }}
                      >
                        {paper.doi}
                      </a>
                    </span>
                  )}
                </div>

                {selectedId === paper.id && <aside ref={detailRef} tabIndex={-1} className="paper-detail-panel"
                  aria-label={`Details: ${paper.title}`} onKeyDown={(event) => { if (event.key === 'Escape') closeDetails(); }}>
                  <div className="paper-detail-heading">
                    <h2>{paper.title}</h2>
                    <button className="action-btn" aria-label="Close paper details" onClick={closeDetails}><X size={16} /></button>
                  </div>
                  <p className="paper-meta">{[paper.authors.join(', '), paper.year, paper.venue, paper.source].filter(Boolean).join(' · ')}</p>
                {abstract && (
                  <div>
                    <div className="paper-abstract">
                      {isExpanded || !needsClamp
                        ? abstract
                        : `${abstract.slice(0, ABSTRACT_CLAMP).trimEnd()}…`}
                    </div>
                    {needsClamp && (
                      <button
                        onClick={() => toggleAbstract(paper.id)}
                        style={{
                          background: 'none',
                          border: 'none',
                          color: 'var(--primary-cyan)',
                          fontSize: 12,
                          fontFamily: 'inherit',
                          cursor: 'pointer',
                          display: 'flex',
                          alignItems: 'center',
                          gap: 4,
                          marginBottom: 12,
                          padding: 0,
                        }}
                      >
                        {isExpanded ? (
                          <>
                            <span>Collapse</span>
                            <ChevronUp size={14} />
                          </>
                        ) : (
                          <>
                            <span>Read abstract</span>
                            <ChevronDown size={14} />
                          </>
                        )}
                      </button>
                    )}
                  </div>
                )}

                <details className="evidence-details">
                  <summary>Transparent Screening Metrics</summary>
                  <div className="evidence-score-grid">
                    <span>Relevance <b>{evaluation.relevance}</b></span>
                    <span>Metadata <b>{evaluation.metadata}</b></span>
                    <span>Recency <b>{evaluation.recency}</b></span>
                    <span>Citations <b>{evaluation.citation}</b></span>
                    <span>Access <b>{evaluation.access}</b></span>
                  </div>
                  <p>Heuristic score = 40% RRF rank + 40% metadata completeness + 20% access availability. Recency and citations are shown for reference without adding bias. This does not evaluate study methodology, certainty of evidence, or risk of bias; applied uniformly across all disciplines.</p>
                </details>

                <div className="paper-actions">
                  <div style={{ display: 'flex', gap: 8, flexWrap: 'wrap' }}>
                    <button
                      className="action-btn"
                      onClick={() => copyCitation(paper, 'apa')}
                      title="Copy APA Citation"
                    >
                      {isCopied ? <Check size={14} color="var(--status-emerald)" /> : <Copy size={14} />}
                      <span>{isCopied ? 'Copied' : 'Copy APA'}</span>
                    </button>

                    <button
                      className="action-btn"
                      onClick={() => copyCitation(paper, 'bibtex')}
                      title="Copy BibTeX Citation"
                    >
                      {isCopiedBib ? <Check size={14} color="var(--status-emerald)" /> : <Copy size={14} />}
                      <span>{isCopiedBib ? 'Copied' : 'Copy BibTeX'}</span>
                    </button>

                    {(paper.source_url || paper.doi) && (
                      <a
                        className="action-btn"
                        href={paper.source_url || `https://doi.org/${paper.doi}`}
                        target="_blank"
                        rel="noreferrer"
                        title={`Open original record on ${paper.source}`}
                        style={{ textDecoration: 'none' }}
                      >
                        <ExternalLink size={14} />
                        <span>View at Source</span>
                      </a>
                    )}

                    <button
                      className="action-btn"
                      onClick={() => toggleCitations(paper)}
                      title="View citations, references, and related papers"
                      aria-expanded={!!citationOpen[paper.id]}
                    >
                      <GitBranch size={14} />
                      <span>{citationOpen[paper.id] ? 'Hide Citations' : 'Citations & Related'}</span>
                    </button>
                  </div>

                  {paper.pdf_url && (
                    <button
                      className="action-btn action-btn-primary"
                      onClick={() => handleDownload(paper)}
                      disabled={isDownloading || isDownloaded}
                    >
                      {isDownloading ? (
                        <Loader2 size={14} className="animate-spin" />
                      ) : isDownloaded ? (
                        <Check size={14} />
                      ) : (
                        <Download size={14} />
                      )}
                      <span>
                        {isDownloaded
                          ? 'Downloaded'
                          : isDownloading
                          ? 'Downloading PDF…'
                          : 'Download Full PDF'}
                      </span>
                    </button>
                  )}
                </div>

                {citationOpen[paper.id] && (
                  <div style={{ marginTop: 12, borderTop: '1px solid var(--cockpit-border)', paddingTop: 10 }}>
                    <div className="segmented" role="tablist" style={{ alignSelf: 'flex-start', marginBottom: 8 }}>
                      {([
                        ['cited_by', 'Cited By'],
                        ['references', 'References'],
                        ['related', 'Related'],
                      ] as [CitationDirection, string][]).map(([id, label]) => (
                        <button
                          key={id}
                          type="button"
                          role="tab"
                          aria-selected={citations[paper.id]?.direction === id}
                          className={`segmented-item ${citations[paper.id]?.direction === id ? 'active' : ''}`}
                          onClick={() => void loadCitations(paper, id)}
                          style={{ fontSize: 11 }}
                        >
                          {label}
                        </button>
                      ))}
                    </div>

                    {citations[paper.id]?.loading && (
                      <div style={{ fontSize: 12, color: 'var(--text-muted)' }}>Loading citation network from OpenAlex…</div>
                    )}
                    {citations[paper.id]?.error && (
                      <div className="alert alert-warning" style={{ margin: 0 }}>{citations[paper.id]?.error}</div>
                    )}
                    {!citations[paper.id]?.loading &&
                      !citations[paper.id]?.error &&
                      (citations[paper.id]?.items.length ?? 0) === 0 && (
                        <div style={{ fontSize: 12, color: 'var(--text-muted)' }}>
                          No citation graph records found for this direction (requires DOI, PMID, or OpenAlex ID).
                        </div>
                      )}

                    {(citations[paper.id]?.items ?? []).map((item) => (
                      <div
                        key={item.id}
                        style={{
                          display: 'flex',
                          gap: 10,
                          alignItems: 'flex-start',
                          padding: '8px 0',
                          borderBottom: '1px solid var(--cockpit-border)',
                        }}
                      >
                        <div style={{ flex: 1, minWidth: 0 }}>
                          <div style={{ fontSize: 13, fontWeight: 600, color: 'var(--text-main)' }}>{item.title}</div>
                          <div style={{ fontSize: 11, color: 'var(--text-muted)' }}>
                            {[item.authors.slice(0, 3).join(', '), item.year, item.source].filter(Boolean).join(' • ')}
                          </div>
                        </div>
                        <div style={{ display: 'flex', gap: 6, flexShrink: 0 }}>
                          {(item.source_url || item.doi) && (
                            <a
                              className="action-btn"
                              href={item.source_url || `https://doi.org/${item.doi}`}
                              target="_blank"
                              rel="noreferrer"
                              style={{ padding: '3px 8px', fontSize: 11, textDecoration: 'none' }}
                            >
                              Open
                            </a>
                          )}
                          <button
                            className={`action-btn ${savedPaperIds.has(item.id) ? 'action-btn-primary' : ''}`}
                            onClick={() => onSavePaper(item)}
                            title={savedPaperIds.has(item.id) ? 'Remove from workspace' : 'Save to workspace'}
                            style={{ padding: '3px 8px', fontSize: 11 }}
                          >
                            <Bookmark size={12} fill={savedPaperIds.has(item.id) ? '#ffffff' : 'none'} />
                          </button>
                        </div>
                      </div>
                    ))}
                  </div>
                )}
                </aside>}
              </article>
            );
          })}
      </div>

      {/* ------- Secondary analysis, below the results it describes ------- */}
      {results && !loading && results.papers.length > 0 && (
        <>
          <EvidenceSynthesis query={results.query} papers={results.papers} />
          <ResearchGapPanel query={query} papers={results.papers} />
        </>
      )}
    </div>
  );
};
