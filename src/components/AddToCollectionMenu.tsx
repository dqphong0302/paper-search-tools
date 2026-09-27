import React, { useEffect, useRef, useState } from 'react';
import { Check, FolderPlus, Loader2, Plus } from 'lucide-react';
import { Paper, Workspace } from '../types';
import { addToCollection, createCollection, INTEREST_LIBRARY_ID, listCollections } from '../lib/collections';

/** "Add to collection" button with a small menu: pick a collection or create one. */
export const AddToCollectionMenu: React.FC<{ papers: Paper[]; onAdded?: (collection: Workspace, count: number) => void }> = ({
  papers,
  onAdded,
}) => {
  const [open, setOpen] = useState(false);
  const [collections, setCollections] = useState<Workspace[]>([]);
  const [newName, setNewName] = useState('');
  const [busy, setBusy] = useState(false);
  const [message, setMessage] = useState('');
  const [error, setError] = useState('');
  const rootRef = useRef<HTMLDivElement>(null);

  useEffect(() => {
    if (!open) return;
    void listCollections()
      .then((list) => setCollections(list.filter((item) => item.id !== INTEREST_LIBRARY_ID)))
      .catch((cause) => setError((cause as Error).message));
    const onClick = (event: MouseEvent) => {
      if (!rootRef.current?.contains(event.target as Node)) setOpen(false);
    };
    const onKey = (event: KeyboardEvent) => { if (event.key === 'Escape') setOpen(false); };
    window.addEventListener('mousedown', onClick);
    window.addEventListener('keydown', onKey);
    return () => { window.removeEventListener('mousedown', onClick); window.removeEventListener('keydown', onKey); };
  }, [open]);

  const addTo = async (collection: Workspace) => {
    setBusy(true); setError('');
    try {
      const count = await addToCollection(collection.id, papers);
      setMessage(`Added ${count} to “${collection.name}”`);
      onAdded?.(collection, count);
      setOpen(false);
      window.setTimeout(() => setMessage(''), 3000);
    } catch (cause) {
      setError((cause as Error).message);
    } finally {
      setBusy(false);
    }
  };

  const createAndAdd = async () => {
    const name = newName.trim();
    if (!name) return;
    setBusy(true); setError('');
    try {
      const collection = await createCollection(name);
      setNewName('');
      await addTo(collection);
    } catch (cause) {
      setError((cause as Error).message);
      setBusy(false);
    }
  };

  return (
    <div ref={rootRef} style={{ position: 'relative', display: 'inline-flex', alignItems: 'center', gap: 8 }}>
      <button
        type="button"
        className="action-btn action-btn-primary"
        onClick={() => setOpen((value) => !value)}
        disabled={!papers.length}
        aria-haspopup="menu"
        aria-expanded={open}
        style={{ padding: '4px 10px' }}
      >
        <FolderPlus size={13} /> Add to collection
      </button>
      {message && <span style={{ color: 'var(--status-emerald)', fontSize: 12 }}><Check size={12} /> {message}</span>}
      {open && (
        <div role="menu" className="cockpit-card collection-menu">
          <div style={{ fontSize: 11, fontWeight: 700, color: 'var(--text-dim)', textTransform: 'uppercase', marginBottom: 6 }}>
            Add {papers.length} paper{papers.length === 1 ? '' : 's'} to…
          </div>
          <div style={{ display: 'flex', flexDirection: 'column', gap: 2, maxHeight: 220, overflowY: 'auto' }}>
            {collections.map((collection) => (
              <button key={collection.id} type="button" role="menuitem" className="collection-menu-item" disabled={busy} onClick={() => void addTo(collection)}>
                <span>{collection.name}</span>
                <span style={{ color: 'var(--text-dim)' }}>{collection.paper_count}</span>
              </button>
            ))}
            {!collections.length && <div style={{ fontSize: 12, color: 'var(--text-muted)', padding: '4px 0' }}>No collections yet.</div>}
          </div>
          <form
            onSubmit={(event) => { event.preventDefault(); void createAndAdd(); }}
            style={{ display: 'flex', gap: 6, marginTop: 8, borderTop: '1px solid var(--cockpit-border)', paddingTop: 8 }}
          >
            <input
              className="field-input"
              placeholder="New collection name"
              aria-label="New collection name"
              value={newName}
              maxLength={120}
              onChange={(event) => setNewName(event.target.value)}
              style={{ flex: 1, minWidth: 0 }}
            />
            <button type="submit" className="action-btn" disabled={busy || !newName.trim()} aria-label="Create collection and add">
              {busy ? <Loader2 size={13} className="animate-spin" /> : <Plus size={13} />}
            </button>
          </form>
          {error && <div className="alert alert-warning" role="alert" style={{ marginTop: 8, fontSize: 12 }}>{error}</div>}
        </div>
      )}
    </div>
  );
};
