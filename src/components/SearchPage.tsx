import React, { useState } from 'react';
import { BookOpen, Globe } from 'lucide-react';
import { Paper, TelemetryStats } from '../types';
import { Explorer } from './Explorer';
import { WebSearch } from './WebSearch';

interface SearchPageProps {
  onSavePaper: (paper: Paper) => void;
  savedPaperIds: Set<string>;
  initialQuery?: string;
  draftQuery?: string;
  searchNonce?: number;
  port: number;
  telemetry: TelemetryStats | null;
  isOnline: boolean;
  onNavigateToExplorer: (query: string) => void;
  onShowOverview: () => void;
  workspaceId: string;
}

type Mode = 'papers' | 'web';

export const SearchPage: React.FC<SearchPageProps> = ({
  onSavePaper,
  savedPaperIds,
  initialQuery,
  draftQuery,
  searchNonce = 0,
  port,
  onShowOverview,
  onNavigateToExplorer,
  workspaceId,
}) => {
  const [mode, setMode] = useState<Mode>('papers');
  // Before the first query, show a compact overview instead of an empty result page.
  const hasSearch = searchNonce > 0 || Boolean(initialQuery?.trim());

  return (
    <div className="page-container" style={{ gap: 14 }}>
      <div style={{ display: 'flex', alignItems: 'center', gap: 10, flexWrap: 'wrap' }}>
      {hasSearch && mode === 'papers' && (
        <button
          id="search-back-overview"
          type="button"
          className="action-btn"
          onClick={onShowOverview}
          title="Clear search"
          style={{ padding: '6px 12px' }}
        >
          Clear
        </button>
      )}
      <div
        role="tablist"
        aria-label="Search Mode"
        className="segmented"
        style={{ alignSelf: 'flex-start' }}
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
          <span>Papers</span>
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
          <span>Web</span>
        </button>
      </div>
      </div>

      {mode === 'web' ? (
        <WebSearch port={port} />
      ) : (
        <Explorer
          key={workspaceId}
          onSavePaper={onSavePaper}
          savedPaperIds={savedPaperIds}
          initialQuery={initialQuery}
          draftQuery={draftQuery}
          onSubmitQuery={onNavigateToExplorer}
          searchNonce={searchNonce}
          port={port}
          hideSearchBar
          workspaceId={workspaceId}
        />
      )}
    </div>
  );
};
