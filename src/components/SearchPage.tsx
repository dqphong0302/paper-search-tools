import React, { useState } from 'react';
import { BookOpen, Globe } from 'lucide-react';
import { Paper } from '../types';
import { Explorer } from './Explorer';
import { WebSearch } from './WebSearch';

interface SearchPageProps {
  onToggleInterest: (paper: Paper) => void;
  interestedPaperIds: Set<string>;
  initialQuery?: string;
  draftQuery?: string;
  searchNonce?: number;
  onNavigateToExplorer: (query: string) => void;
  onShowOverview: () => void;
  defaultTopic?: string;
}

type Mode = 'papers' | 'web';

export const SearchPage: React.FC<SearchPageProps> = ({
  onToggleInterest,
  interestedPaperIds,
  initialQuery,
  draftQuery,
  searchNonce = 0,
  onShowOverview,
  onNavigateToExplorer,
  defaultTopic,
}) => {
  const [mode, setMode] = useState<Mode>('papers');
  // Before the first query, show a compact overview instead of an empty result page.
  const hasSearch = searchNonce > 0 || Boolean(initialQuery?.trim());

  return (
    <div className="page-container u-gap-14">
      <div style={{ display: 'flex', alignItems: 'center', gap: 10, flexWrap: 'wrap' }}>
        {hasSearch && mode === 'papers' && (
          <button
            id="search-back-overview"
            type="button"
            className="action-btn"
            onClick={onShowOverview}
            title="Clear the current search"
            style={{ padding: '6px 12px' }}
          >
            Clear
          </button>
        )}
        <div
          role="tablist"
          aria-label="Search Mode"
          className="segmented u-self-start"
        >
          <button
            id="search-mode-papers"
            type="button"
            role="tab"
            aria-selected={mode === 'papers'}
            className={`segmented-item ${mode === 'papers' ? 'active' : ''}`}
            onClick={() => setMode('papers')}
          >
            <BookOpen size={14} />
            <span>Academic papers</span>
          </button>
          <button
            id="search-mode-web"
            type="button"
            role="tab"
            aria-selected={mode === 'web'}
            className={`segmented-item ${mode === 'web' ? 'active' : ''}`}
            onClick={() => setMode('web')}
          >
            <Globe size={14} />
            <span>Academic web</span>
          </button>
        </div>
      </div>

      {mode === 'web' ? (
        <WebSearch />
      ) : (
        <Explorer
          onSavePaper={onToggleInterest}
          savedPaperIds={interestedPaperIds}
          initialQuery={initialQuery}
          draftQuery={draftQuery}
          onSubmitQuery={onNavigateToExplorer}
          searchNonce={searchNonce}
          hideSearchBar
          initialScope={defaultTopic}
        />
      )}
    </div>
  );
};
