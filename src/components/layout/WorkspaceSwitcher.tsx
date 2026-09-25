import React, { useState } from 'react';
import { Check, Plus, X } from 'lucide-react';
import { useWorkspace } from '../../state/WorkspaceContext';

/** Picks the workspace the Library, Interest toggles, searches and downloads apply to. */
export const WorkspaceSwitcher: React.FC = () => {
  const { workspaces, active, select, create } = useWorkspace();
  const [creating, setCreating] = useState(false);
  const [name, setName] = useState('');
  const [error, setError] = useState('');

  const submit = async (event: React.FormEvent) => {
    event.preventDefault();
    const next = name.trim();
    if (!next) return;
    try {
      await create(next);
      setCreating(false);
      setName('');
      setError('');
    } catch (err) {
      setError((err as Error).message);
    }
  };

  return (
    <div className="workspace-switcher">
      <label className="workspace-switcher-label" htmlFor="workspace-select">Workspace</label>
      {creating ? (
        <form className="workspace-switcher-row" onSubmit={submit}>
          <input id="workspace-new-name" autoFocus value={name} onChange={(e) => setName(e.target.value)} placeholder="Project name" aria-label="New workspace name" />
          <button type="submit" className="action-btn" aria-label="Create workspace" disabled={!name.trim()}><Check size={13} /></button>
          <button type="button" className="action-btn" aria-label="Cancel" onClick={() => { setCreating(false); setError(''); }}><X size={13} /></button>
        </form>
      ) : (
        <div className="workspace-switcher-row">
          <select id="workspace-select" value={active.id} onChange={(e) => select(e.target.value)}>
            {workspaces.map((w) => (
              <option key={w.id} value={w.id}>{w.name}{w.paperCount ? ` (${w.paperCount})` : ''}</option>
            ))}
          </select>
          <button id="workspace-new" type="button" className="action-btn" title="New workspace" aria-label="New workspace" onClick={() => setCreating(true)}><Plus size={13} /></button>
        </div>
      )}
      {error && <div className="workspace-switcher-error" role="alert">{error}</div>}
    </div>
  );
};
