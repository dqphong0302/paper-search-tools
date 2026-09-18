import React, { lazy, Suspense, useState } from 'react';
import { Bookmark, DownloadCloud, History } from 'lucide-react';
import { WorkspacePaper, WorkspacePaperPatch } from '../types';
import { Library } from './Library';
import { DownloadHistory } from './DownloadHistory';
import { SearchHistory } from './SearchHistory';
const ClinicalSuite = lazy(() => import('./ClinicalSuite').then((module) => ({ default: module.ClinicalSuite })));

interface ResearchLibraryProps {
  papers: WorkspacePaper[];
  onRemovePaper: (id: string) => void;
  onUpdatePaper: (id: string, patch: WorkspacePaperPatch) => void;
  onRerunSearch: (query: string) => void;
  port: number;
}

type Section = 'library' | 'downloads' | 'history' | 'analysis';

export const ResearchLibrary: React.FC<ResearchLibraryProps> = ({
  papers,
  onRemovePaper,
  onUpdatePaper,
  onRerunSearch,
  port,
}) => {
  const [section, setSection] = useState<Section>('library');
  const tabs: { id: Section; label: string; icon: React.ReactNode; count?: number }[] = [
    { id: 'library', label: 'Papers of interest', icon: <Bookmark size={14} />, count: papers.length },
    { id: 'downloads', label: 'Downloaded PDFs', icon: <DownloadCloud size={14} /> },
    { id: 'history', label: 'Search history', icon: <History size={14} /> },
    { id: 'analysis', label: 'Analysis tools', icon: <Bookmark size={14} /> },
  ];

  return (
    <div className="page-container" style={{ gap: 14 }}>
      <div>
        <h2 className="page-title"><Bookmark size={17} /> Interest Library</h2>
        <p className="page-subtitle">The papers you marked while searching.</p>
      </div>

      <div role="tablist" aria-label="Library sections" className="segmented" style={{ alignSelf: 'flex-start' }}>
        {tabs.map((tab) => (
          <button
            key={tab.id}
            id={`research-tab-${tab.id}`}
            type="button"
            role="tab"
            aria-selected={section === tab.id}
            className={`segmented-item ${section === tab.id ? 'active' : ''}`}
            onClick={() => setSection(tab.id)}
          >
            {tab.icon}
            <span>{tab.label}</span>
            {tab.count !== undefined && tab.count > 0 && <span className="segmented-count">{tab.count}</span>}
          </button>
        ))}
      </div>

      {section === 'library' && <Library workspacePapers={papers} onRemovePaper={onRemovePaper} onUpdatePaper={onUpdatePaper} workspaceName="Interest Library" />}
      {section === 'downloads' && <DownloadHistory port={port} />}
      {section === 'history' && <SearchHistory onRerunSearch={onRerunSearch} port={port} />}
      {section === 'analysis' && <Suspense fallback={<div role="status">Loading tools…</div>}><ClinicalSuite /></Suspense>}
    </div>
  );
};
