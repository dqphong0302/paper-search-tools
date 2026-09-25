import React, { lazy, Suspense, useState } from 'react';
import { Bookmark, DownloadCloud, FlaskConical, FolderOpen, History, Pencil, Trash2 } from 'lucide-react';
import { WorkspacePaper, WorkspacePaperPatch } from '../types';
import { Library } from './Library';
import { DownloadHistory } from './DownloadHistory';
import { SearchHistory } from './SearchHistory';
import { useWorkspace } from '../state/WorkspaceContext';
const ClinicalSuite = lazy(() => import('./ClinicalSuite').then((module) => ({ default: module.ClinicalSuite })));

interface ResearchLibraryProps {
  papers: WorkspacePaper[];
  onRemovePaper: (id: string) => void;
  onUpdatePaper: (id: string, patch: WorkspacePaperPatch) => void;
  onRerunSearch: (query: string) => void;
}

type Section = 'library' | 'downloads' | 'history' | 'analysis';

/** Rename/delete for project workspaces; the built-in library cannot be removed. */
const WorkspaceActions: React.FC = () => {
  const { active, rename, remove } = useWorkspace();
  const [editing, setEditing] = useState(false);
  const [name, setName] = useState('');
  const [confirmDelete, setConfirmDelete] = useState(false);
  const [error, setError] = useState('');

  if (active.isLibrary) return null;

  const run = async (action: () => Promise<void>) => {
    try {
      await action();
      setError('');
    } catch (err) {
      setError((err as Error).message);
    }
  };

  return (
    <div className="workspace-actions">
      {editing ? (
        <form className="workspace-actions" onSubmit={(e) => { e.preventDefault(); if (name.trim()) void run(async () => { await rename(active.id, name.trim()); setEditing(false); }); }}>
          <input id="workspace-rename-input" autoFocus value={name} onChange={(e) => setName(e.target.value)} aria-label="Workspace name" />
          <button type="submit" className="action-btn action-btn-primary" disabled={!name.trim()}>Save</button>
          <button type="button" className="action-btn" onClick={() => setEditing(false)}>Cancel</button>
        </form>
      ) : (
        <button id="workspace-rename" type="button" className="action-btn" onClick={() => { setName(active.name); setEditing(true); }}><Pencil size={13} /><span>Rename</span></button>
      )}
      {confirmDelete ? (
        <>
          <button id="workspace-delete-confirm" type="button" className="action-btn action-btn-danger" onClick={() => void run(() => remove(active.id))}><Trash2 size={13} /><span>Delete “{active.name}”</span></button>
          <button type="button" className="action-btn" onClick={() => setConfirmDelete(false)}>Keep</button>
        </>
      ) : (
        <button id="workspace-delete" type="button" className="action-btn" onClick={() => setConfirmDelete(true)}><Trash2 size={13} /><span>Delete</span></button>
      )}
      {error && <span className="workspace-switcher-error" role="alert">{error}</span>}
    </div>
  );
};

export const ResearchLibrary: React.FC<ResearchLibraryProps> = ({
  papers,
  onRemovePaper,
  onUpdatePaper,
  onRerunSearch,
}) => {
  const { active, scopeId } = useWorkspace();
  const [section, setSection] = useState<Section>('library');
  const tabs: { id: Section; label: string; icon: React.ReactNode; count?: number }[] = [
    { id: 'library', label: 'Papers of interest', icon: <Bookmark size={14} />, count: papers.length },
    { id: 'downloads', label: 'Downloaded PDFs', icon: <DownloadCloud size={14} /> },
    { id: 'history', label: 'Search history', icon: <History size={14} /> },
    { id: 'analysis', label: 'Analysis tools', icon: <FlaskConical size={14} /> },
  ];

  return (
    <div className="page-container" style={{ gap: 14 }}>
      <div className="page-header">
        <div>
          <h2 className="page-title">{active.isLibrary ? <Bookmark size={17} /> : <FolderOpen size={17} />} {active.name}</h2>
          <p className="page-subtitle">
            {active.isLibrary
              ? 'The papers you marked while searching.'
              : 'A project workspace: papers, searches and downloads made while it is selected are kept here. Agents reach it through MCP with the same workspace_id.'}
          </p>
        </div>
        <WorkspaceActions />
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

      {section === 'library' && <Library workspacePapers={papers} onRemovePaper={onRemovePaper} onUpdatePaper={onUpdatePaper} workspaceName={active.name} />}
      {section === 'downloads' && <DownloadHistory workspaceId={scopeId} />}
      {section === 'history' && <SearchHistory onRerunSearch={onRerunSearch} workspaceId={scopeId} />}
      {section === 'analysis' && <Suspense fallback={<div role="status">Loading tools…</div>}><ClinicalSuite /></Suspense>}
    </div>
  );
};
