import React from 'react';
import { Loader2 } from 'lucide-react';
import { SearchResponse } from '../../types';
import { SOURCE_GROUPS, SourceGroup } from '../../lib/paperEvaluation';
import { KIND_META, PaperKind } from '../../lib/paperKind';
import { SortKey, SourceFilter } from './searchConfig';

interface ResultToolbarProps {
  results: SearchResponse;
  availableTotal: number;
  shownCount: number;
  interestedCount: number;
  sourceCounts: Record<SourceGroup, number>;
  sourceFilter: SourceFilter;
  onSourceFilter: (filter: SourceFilter) => void;
  oaOnly: boolean;
  hiddenByOa: number;
  onOaToggle: (next: boolean) => void;
  recommendedPdfOnly: boolean;
  onRecommendedPdfOnly: (next: boolean) => void;
  kindFilter: 'all' | PaperKind;
  onKindFilter: (kind: 'all' | PaperKind) => void;
  sortKey: SortKey;
  onSortKey: (key: SortKey) => void;
  canLoadMore: boolean;
  loadingMore: boolean;
  onLoadMore: () => void;
}

const Tab: React.FC<{ id: string; active: boolean; onClick: () => void; label: string; count: number }> = ({ id, active, onClick, label, count }) => (
  <button id={id} type="button" role="tab" aria-selected={active} className={`segmented-item ${active ? 'active' : ''}`} onClick={onClick}>
    <span>{label}</span>
    <span className="segmented-count">{count}</span>
  </button>
);

/** Source-group tabs, result filters, sort and pagination for one result set. */
export const ResultToolbar: React.FC<ResultToolbarProps> = (props) => {
  const { results, sourceFilter, onSourceFilter, sourceCounts } = props;
  const elapsed = results.elapsed_ms >= 1000 ? `${(results.elapsed_ms / 1000).toFixed(1)}s` : `${results.elapsed_ms}ms`;
  return (
    <div className="result-toolbar">
      <div className="segmented source-group-filter" role="tablist" aria-label="Filter by source group or interest list">
        <Tab id="source-group-all" active={sourceFilter === 'all'} onClick={() => onSourceFilter('all')} label="All" count={props.shownCount} />
        {props.interestedCount > 0 && (
          <Tab id="source-group-interested" active={sourceFilter === 'interested'} onClick={() => onSourceFilter('interested')} label="Interest" count={props.interestedCount} />
        )}
        {Object.entries(SOURCE_GROUPS)
          .filter(([id]) => sourceCounts[id as SourceGroup] > 0 || sourceFilter === id)
          .map(([id, meta]) => (
            <Tab key={id} id={`source-group-${id}`} active={sourceFilter === id} onClick={() => onSourceFilter(id as SourceGroup)} label={meta.shortLabel} count={sourceCounts[id as SourceGroup]} />
          ))}
      </div>

      <div className="result-toolbar-controls">
        <label className="check-label">
          <input id="filter-open-access" type="checkbox" checked={props.oaOnly} onChange={(e) => props.onOaToggle(e.target.checked)} />
          <span>Open access / has PDF</span>
          {props.oaOnly && props.hiddenByOa > 0 && <span className="text-dim">(hidden {props.hiddenByOa})</span>}
        </label>

        <label className="check-label">
          <input id="filter-recommended-pdf" type="checkbox" checked={props.recommendedPdfOnly} onChange={(e) => props.onRecommendedPdfOnly(e.target.checked)} />
          <span>Prefer PDF</span>
        </label>

        <label className="u-row u-gap-6">
          <span>Kind</span>
          <select id="filter-kind" className="field-input field-sm" value={props.kindFilter} onChange={(e) => props.onKindFilter(e.target.value as 'all' | PaperKind)}>
            <option value="all">All kinds</option>
            {(Object.keys(KIND_META) as PaperKind[]).map((kind) => (
              <option key={kind} value={kind}>{KIND_META[kind].label}</option>
            ))}
          </select>
        </label>

        <label className="u-row u-gap-6">
          <span>Sort</span>
          <select id="sort-results" className="field-input field-sm" value={props.sortKey} onChange={(e) => props.onSortKey(e.target.value as SortKey)}>
            <option value="relevance">Relevance (RRF)</option>
            <option value="evaluation">Screening score</option>
            <option value="pdf">PDF availability</option>
            <option value="year">Most recent publication year</option>
            <option value="citations">Citation count</option>
          </select>
        </label>

        <span className="mono-dim">
          {results.total}
          {props.availableTotal > results.total ? ` / ${props.availableTotal}` : ''} results
          {' · '}
          {elapsed}
        </span>
        {props.canLoadMore && (
          <button id="search-load-more" type="button" className="action-btn action-btn-xs" onClick={props.onLoadMore} disabled={props.loadingMore} title="Load more results">
            {props.loadingMore ? <Loader2 size={12} className="animate-spin" /> : null}
            {props.loadingMore ? 'Loading…' : 'Load more'}
          </button>
        )}
      </div>
    </div>
  );
};
