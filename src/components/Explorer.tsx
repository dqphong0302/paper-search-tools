import React, { useState, useEffect, useCallback, useMemo, useRef } from 'react';
import { Search, Sparkles, AlertTriangle, Compass, ExternalLink, Loader2 } from 'lucide-react';
import { Paper } from '../types';
import { SearchScanner } from './SearchScanner';
import { ResearchGapPanel } from './ResearchGapPanel';
import { EvidenceSynthesis } from './EvidenceSynthesis';
import { evaluatePaper, getSourceGroup, SOURCE_GROUPS, SourceGroup } from '../lib/paperEvaluation';
import { apaCitation, bibtexCitation, bibtexLibrary, risLibrary } from '../lib/citation';
import { getPaperKind, PaperKind } from '../lib/paperKind';
import { canDownloadPdf, requestPdfDownload } from '../lib/pdfDownload';
import { downloadTextFile } from '../lib/download';
import { useSelection } from '../lib/useSelection';
import { useWorkspace } from '../state/WorkspaceContext';
import { usePaperSearch } from '../hooks/usePaperSearch';
import { useCitations } from '../hooks/useCitations';
import { SourceLimiterModal } from './SourceLimiterModal';
import { FulltextViewerModal } from './FulltextViewerModal';
import { AiAgentExportModal } from './AiAgentExportModal';
import { PaperCard } from './PaperCard';
import { isVietnamPaper, originalPaperUrl, suggestedKeywords } from './Explorer.shared';
import { ScopeBar } from './explorer/ScopeBar';
import { SearchOptions } from './explorer/SearchOptions';
import { ResultToolbar } from './explorer/ResultToolbar';
import { SelectionBar } from './explorer/SelectionBar';
import { SourceHealth } from './explorer/SourceHealth';
import {
  DEFAULT_FILTERS, filtersError, isKnownPreset, Scope, SearchFilters, SortKey, SourceFilter, sourcesFor,
} from './explorer/searchConfig';
import { buildMeshQuery } from './explorer/meshQuery';

// Re-exported so existing importers (Library, tests) keep working.
export { buildMeshQuery, isVietnamPaper, originalPaperUrl, suggestedKeywords };

interface ExplorerProps {
  onSavePaper: (paper: Paper) => void;
  savedPaperIds: Set<string>;
  initialQuery?: string;
  draftQuery?: string;
  onSubmitQuery?: (query: string) => void;
  searchNonce?: number;
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
  hideSearchBar = false,
  initialScope = 'default',
}) => {
  const { scopeId: workspaceScope } = useWorkspace();
  const search = usePaperSearch();
  const { results, loading, error, setError } = search;
  const citations = useCitations();

  const [localQuery, setQuery] = useState(initialQuery || '');
  const query = draftQuery ?? localQuery;
  const [filters, setFilters] = useState<SearchFilters>(() => ({
    ...DEFAULT_FILTERS,
    scope: isKnownPreset(initialScope) ? initialScope : 'default',
  }));
  const patchFilters = useCallback((patch: Partial<SearchFilters>) => setFilters((f) => ({ ...f, ...patch })), []);
  const [oaOnly, setOaOnly] = useState(false);
  const [recommendedPdfOnly, setRecommendedPdfOnly] = useState(false);
  const [sortKey, setSortKey] = useState<SortKey>('relevance');
  const [sourceFilter, setSourceFilter] = useState<SourceFilter>('all');
  const [kindFilter, setKindFilter] = useState<'all' | PaperKind>('all');
  const searchOptionsRef = useRef<HTMLDetailsElement>(null);

  const [selectedId, setSelectedId] = useState<string | null>(null);
  const detailRef = useRef<HTMLElement>(null);
  const detailTrigger = useRef<HTMLButtonElement | null>(null);
  // Every handler a PaperCard receives is wrapped so its identity is stable:
  // otherwise a new closure on each render would defeat the card's `memo`.
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

  const [sourceLimiterOpen, setSourceLimiterOpen] = useState(false);
  const [readerPaper, setReaderPaper] = useState<Paper | null>(null);
  const [readerOpen, setReaderOpen] = useState(false);
  const [agentExportModalOpen, setAgentExportModalOpen] = useState(false);
  const [agentExportPapers, setAgentExportPapers] = useState<Paper[]>([]);

  // Follow the Settings topic until the user picks sources for this search.
  useEffect(() => {
    if (!results && filters.customSources.length === 0 && isKnownPreset(initialScope)) {
      patchFilters({ scope: initialScope });
    }
  }, [initialScope, results, filters.customSources.length, patchFilters]);

  const runSearch = useCallback(
    async (searchQuery: string, f: SearchFilters, openAccessOnly: boolean) => {
      const q = searchQuery.trim();
      if (!q) {
        setError('Enter a keyword, author name or DOI before searching.');
        return;
      }
      const invalid = filtersError(f);
      if (invalid) {
        setError(invalid);
        return;
      }
      if (searchOptionsRef.current) searchOptionsRef.current.open = false;
      setSelectedId(null);
      const data = await search.run({
        query: q,
        limit: f.resultLimit ? Number(f.resultLimit) : undefined,
        year_min: f.yearMin ? Number(f.yearMin) : undefined,
        year_max: f.yearMax ? Number(f.yearMax) : undefined,
        open_access_only: openAccessOnly,
        sources: sourcesFor(f.scope, f.customSources),
        workspace_id: workspaceScope,
      });
      if (data) setSourceFilter(f.scope === 'vietnam' ? 'vietnam' : 'all');
    },
    [search.run, setError, workspaceScope]
  );

  // A query pushed in from another tab (omnibox, history) always re-runs.
  useEffect(() => {
    if (initialQuery && initialQuery.trim()) {
      setQuery(initialQuery);
      void runSearch(initialQuery, filters, oaOnly);
    } else {
      search.reset();
    }
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [initialQuery, searchNonce]);

  const handleScopeChange = (scope: Scope) => {
    // Picking a preset directly clears any custom source override.
    patchFilters({ scope, customSources: [] });
  };

  const handleApplyCustomSources = (sources: string[]) => {
    const next = { ...filters, customSources: sources };
    setFilters(next);
    if (query.trim()) void runSearch(query, next, oaOnly);
  };

  const submitSearch = () => {
    if (onSubmitQuery) onSubmitQuery(query);
    else void runSearch(query, filters, oaOnly);
  };

  const applyMeshSearch = (meshQuery: string) => {
    if (onSubmitQuery) onSubmitQuery(meshQuery);
    else {
      setQuery(meshQuery);
      void runSearch(meshQuery, filters, oaOnly);
    }
  };

  const handleOaToggle = (next: boolean) => {
    setOaOnly(next);
    if (query.trim()) void runSearch(query, filters, next);
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
  }, [setError]);

  const handleDownload = useCallback(async (paper: Paper) => {
    if (!canDownloadPdf(paper)) return;
    setDownloadingId(paper.id);
    setDownloadError(null);
    const result = await requestPdfDownload(paper, workspaceScope);
    setDownloadingId(null);
    if (!result.ok) {
      setDownloadError({ message: result.error, paper });
      setTimeout(() => setDownloadError(null), 12000);
      return;
    }
    setDownloadSuccessId(paper.id);
    setTimeout(() => setDownloadSuccessId(null), 3000);
  }, [workspaceScope]);

  const openFulltextReader = useCallback((paper: Paper) => {
    setReaderPaper(paper);
    setReaderOpen(true);
  }, []);

  const openAgentExport = useCallback((papers: Paper[]) => {
    setAgentExportPapers(papers);
    setAgentExportModalOpen(true);
  }, []);
  const openAgentExportOne = useCallback((paper: Paper) => openAgentExport([paper]), [openAgentExport]);

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
    const counts = Object.fromEntries(Object.keys(SOURCE_GROUPS).map((group) => [group, 0])) as Record<SourceGroup, number>;
    allPapers.forEach((paper) => {
      counts[getSourceGroup(paper)] += 1;
    });
    return counts;
  }, [allPapers]);

  const interestedCount = useMemo(
    () => allPapers.filter((paper) => savedPaperIds.has(paper.id)).length,
    [allPapers, savedPaperIds]
  );

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

  // Selection over the rows currently shown, so papers can be gathered straight
  // from the results and exported without saving them first.
  const visibleIds = useMemo(() => displayedPapers.map((paper) => paper.id), [displayedPapers]);
  const selection = useSelection(visibleIds);
  const selectedPapers = useMemo(
    () => allPapers.filter((paper) => selection.isSelected(paper.id)),
    [allPapers, selection]
  );
  const stamp = () => `scholargate_${new Date().toISOString().slice(0, 10)}`;

  return (
    <div className="page-container">
      <div className="u-stack">
        {!hideSearchBar && (
          <form id="paper-search" className="search-omnibox" onSubmit={(e) => { e.preventDefault(); submitSearch(); }}>
            <Search size={18} className="omnibox-icon" />
            <input
              id="paper-search-input"
              type="text"
              className="search-input"
              placeholder="Search keywords, authors, topics, DOI (10.xxx) or PMID… (⌘K)"
              value={query}
              onChange={(e) => setQuery(e.target.value)}
              aria-label="Search query"
            />
            <button id="paper-search-submit" type="submit" className="search-submit-btn" disabled={loading || !query.trim()}>
              {loading ? <><Loader2 size={14} className="animate-spin" /><span>Scanning…</span></> : <><Sparkles size={14} /><span>Search</span></>}
            </button>
          </form>
        )}

        <ScopeBar scope={filters.scope} customSources={filters.customSources} onScopeChange={handleScopeChange} onOpenLimiter={() => setSourceLimiterOpen(true)} />

        <SearchOptions
          ref={searchOptionsRef}
          filters={filters}
          onChange={patchFilters}
          onScopeChange={handleScopeChange}
          onReset={() => setFilters({ ...DEFAULT_FILTERS, scope: 'default' })}
          hasQuery={!!query.trim()}
          busy={loading}
          onSubmit={submitSearch}
          onApplyMesh={applyMeshSearch}
        />
      </div>

      {error && (
        <div className="alert alert-danger">
          <AlertTriangle size={16} />
          <div>{error}</div>
        </div>
      )}

      {downloadError && (
        <div className="alert alert-warning floating-alert" role="alert">
          <AlertTriangle size={16} />
          <div className="u-row u-gap-12">
            <span>{downloadError.message}</span>
            {downloadError.paper && originalPaperUrl(downloadError.paper) && (
              <a className="action-btn" href={originalPaperUrl(downloadError.paper)!} target="_blank" rel="noreferrer">
                <ExternalLink size={13} />
                <span>Open the article page</span>
              </a>
            )}
          </div>
        </div>
      )}

      {results && !loading && (
        <ResultToolbar
          results={results}
          availableTotal={search.availableTotal}
          shownCount={allPapers.length}
          interestedCount={interestedCount}
          sourceCounts={sourceCounts}
          sourceFilter={sourceFilter}
          onSourceFilter={setSourceFilter}
          oaOnly={oaOnly}
          hiddenByOa={hiddenByOa}
          onOaToggle={handleOaToggle}
          recommendedPdfOnly={recommendedPdfOnly}
          onRecommendedPdfOnly={setRecommendedPdfOnly}
          kindFilter={kindFilter}
          onKindFilter={setKindFilter}
          sortKey={sortKey}
          onSortKey={setSortKey}
          canLoadMore={search.canLoadMore}
          loadingMore={search.loadingMore}
          onLoadMore={() => void search.loadMore()}
        />
      )}

      {results && !loading && displayedPapers.length > 0 && (
        <SelectionBar
          shownCount={displayedPapers.length}
          selectedCount={selection.count}
          allVisibleSelected={selection.allVisibleSelected}
          visibleSelectedCount={selection.visibleSelectedCount}
          onToggleAll={selection.toggleAllVisible}
          onClear={selection.clear}
          onExportRis={() => downloadTextFile(risLibrary(selectedPapers), `${stamp()}.ris`, 'application/x-research-info-systems')}
          onExportBibtex={() => downloadTextFile(bibtexLibrary(selectedPapers), `${stamp()}.bib`, 'application/x-bibtex')}
          onSendToAgent={() => openAgentExport(selectedPapers)}
        />
      )}

      {results?.sources && results.sources.length > 0 && !loading && <SourceHealth sources={results.sources} />}

      <div className="paper-list">
        {loading && (
          <div className="u-stack u-gap-14">
            <SearchScanner query={query || 'All topics'} scope={filters.scope} />
            {[0, 1].map((i) => (
              <div key={i} className="cockpit-card skeleton-card">
                <div className="skeleton" style={{ width: 140, height: 12 }} />
                <div className="skeleton" style={{ width: '80%', height: 16 }} />
                <div className="skeleton" style={{ width: '50%', height: 12 }} />
                <div className="skeleton" style={{ width: '100%', height: 36 }} />
              </div>
            ))}
          </div>
        )}

        {!loading && !results && !error && (
          <div className="empty-state">
            <div className="empty-state-icon"><Compass size={24} /></div>
            <div className="empty-state-title">Explore academic research papers</div>
            <div className="empty-state-text">
              Type a keyword, paper title or DOI above. You can pick a discipline quickly, or open <b>Limit sources</b> to narrow the search.
            </div>
          </div>
        )}

        {!loading && results && displayedPapers.length === 0 && (
          <div className="empty-state">
            <div className="empty-state-icon"><Search size={24} /></div>
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
              isCitationOpen={!!citations.open[paper.id]}
              isChecked={selection.isSelected(paper.id)}
              onToggleChecked={selection.toggle}
              citationState={citations.citations[paper.id]}
              savedPaperIds={savedPaperIds}
              detailRef={detailRef}
              detailTrigger={detailTrigger}
              onSelect={setSelectedId}
              onCloseDetails={closeDetails}
              onToggleAbstract={toggleAbstract}
              onSavePaper={onSavePaper}
              onCopyCitation={copyCitation}
              onToggleCitations={citations.toggle}
              onLoadCitations={citations.load}
              onDownload={handleDownload}
              onOpenFulltext={openFulltextReader}
              onOpenAgentExport={openAgentExportOne}
            />
          ))}
      </div>

      {/* Secondary analysis, below the results it describes */}
      {results && !loading && results.papers.length > 0 && (
        <>
          <EvidenceSynthesis query={results.query} papers={results.papers} />
          <ResearchGapPanel query={query} papers={results.papers} />
        </>
      )}

      <SourceLimiterModal
        isOpen={sourceLimiterOpen}
        onClose={() => setSourceLimiterOpen(false)}
        activeScope={filters.scope}
        selectedSources={filters.customSources}
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
