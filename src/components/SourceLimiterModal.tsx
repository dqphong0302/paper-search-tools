import React, { useState, useMemo } from 'react';
import {
  X,
  Search,
  Filter,
  CheckSquare,
  Square,
  RotateCcw,
  Check,
  Layers,
  KeyRound,
  ShieldCheck,
} from 'lucide-react';
import searchCatalog from '../lib/searchCatalog.json';

export interface SourceLimiterModalProps {
  isOpen: boolean;
  onClose: () => void;
  activeScope: string;
  selectedSources: string[];
  onApplySources: (sources: string[]) => void;
}

export const SourceLimiterModal: React.FC<SourceLimiterModalProps> = ({
  isOpen,
  onClose,
  activeScope,
  selectedSources,
  onApplySources,
}) => {
  const [searchTerm, setSearchTerm] = useState('');
  const [activeGroupFilter, setActiveGroupFilter] = useState<string>('all');
  const [tempSelected, setTempSelected] = useState<string[]>(selectedSources);

  // Sync tempSelected when modal opens or selectedSources prop changes
  React.useEffect(() => {
    if (isOpen) {
      setTempSelected(selectedSources);
      setSearchTerm('');
      setActiveGroupFilter('all');
    }
  }, [isOpen, selectedSources]);

  const allSources = useMemo(() => searchCatalog.sources.filter((source) => source.available !== false), []);
  const availableIds = useMemo(() => new Set(allSources.map((source) => source.id)), [allSources]);
  const presets = useMemo(() => searchCatalog.presets
    .map((preset) => ({ ...preset, sources: preset.sources.filter((id) => availableIds.has(id)) }))
    .filter((preset) => preset.sources.length > 0), [availableIds]);

  const currentPreset = useMemo(() => {
    return presets.find((p) => p.id === activeScope);
  }, [presets, activeScope]);

  // Unique groups for filtering
  const groups = useMemo(() => {
    const set = new Set<string>();
    allSources.forEach((s) => set.add(s.group));
    return Array.from(set);
  }, [allSources]);

  // Filtered source list based on search and group filter
  const filteredSources = useMemo(() => {
    return allSources.filter((s) => {
      if (activeGroupFilter !== 'all' && s.group !== activeGroupFilter) return false;
      if (searchTerm.trim()) {
        const term = searchTerm.toLowerCase();
        return (
          s.name.toLowerCase().includes(term) ||
          s.id.toLowerCase().includes(term) ||
          s.desc.toLowerCase().includes(term) ||
          s.group.toLowerCase().includes(term)
        );
      }
      return true;
    });
  }, [allSources, activeGroupFilter, searchTerm]);

  if (!isOpen) return null;

  const isSelected = (id: string) => tempSelected.includes(id);

  const toggleSource = (id: string) => {
    setTempSelected((prev) =>
      prev.includes(id) ? prev.filter((item) => item !== id) : [...prev, id]
    );
  };

  const handleSelectAllFiltered = () => {
    const availableFilteredIds = filteredSources.filter((s) => s.available).map((s) => s.id);
    setTempSelected((prev) => Array.from(new Set([...prev, ...availableFilteredIds])));
  };

  const handleDeselectAllFiltered = () => {
    const filteredIds = new Set(filteredSources.map((s) => s.id));
    setTempSelected((prev) => prev.filter((id) => !filteredIds.has(id)));
  };

  const handleSelectPresetSources = () => {
    if (currentPreset && currentPreset.sources.length > 0) {
      setTempSelected([...currentPreset.sources]);
    } else {
      setTempSelected([]);
    }
  };

  const handleResetToDefault = () => {
    setTempSelected([]);
  };

  const handleApply = () => {
    onApplySources(tempSelected);
    onClose();
  };

  return (
    <div className="modal-overlay" onClick={onClose} role="dialog" aria-modal="true">
      <div
        className="modal-dialog"
        onClick={(e) => e.stopPropagation()}
        style={{ maxWidth: 780, width: '92vw', maxHeight: '88vh', display: 'flex', flexDirection: 'column' }}
      >
        <div className="modal-header">
          <div className="u-flex u-center">
            <Filter size={18} className="text-accent" />
            <div>
              <div className="modal-title" style={{ fontSize: 16 }}>
                Academic source limiter
              </div>
              <div className="text-sm text-muted">
                Showing only the {allSources.length} sources this build can query
              </div>
            </div>
          </div>
          <button className="modal-close-btn" onClick={onClose} aria-label="Close">
            <X size={16} />
          </button>
        </div>

        {/* Preset Context & Quick Actions */}
        <div
          style={{
            padding: '12px 20px',
            background: 'var(--cockpit-card-hover)',
            borderBottom: '1px solid var(--cockpit-border)',
            display: 'flex',
            alignItems: 'center',
            justifyContent: 'space-between',
            flexWrap: 'wrap',
            gap: 10,
          }}
        >
          <div style={{ display: 'flex', alignItems: 'center', gap: 6, fontSize: 13 }}>
            <Layers size={14} className="text-accent" />
            <span>Current discipline:</span>
            <span className="badge badge-group" style={{ fontWeight: 600 }}>
              {currentPreset?.label || 'Default (Settings)'}
            </span>
            <span style={{ color: 'var(--text-dim)', fontSize: 12 }}>
              ({tempSelected.length === 0 ? 'All sources in this preset' : `${tempSelected.length} sources selected`})
            </span>
          </div>

          <div className="u-wrap u-gap-6">
            {currentPreset && currentPreset.sources.length > 0 && (
              <button
                type="button"
                className="action-btn"
                onClick={handleSelectPresetSources}
                title="Select exactly the sources in the current discipline"
                style={{ fontSize: 11.5, padding: '3px 8px' }}
              >
                <Check size={12} />
                <span>Sources in {currentPreset.label}</span>
              </button>
            )}
            <button
              type="button"
              className="action-btn"
              onClick={handleSelectAllFiltered}
              style={{ fontSize: 11.5, padding: '3px 8px' }}
            >
              <CheckSquare size={12} />
              <span>Select what is shown</span>
            </button>
            <button
              type="button"
              className="action-btn"
              onClick={handleDeselectAllFiltered}
              style={{ fontSize: 11.5, padding: '3px 8px' }}
            >
              <Square size={12} />
              <span>Clear the current selection</span>
            </button>
            <button
              type="button"
              className="action-btn"
              onClick={handleResetToDefault}
              title="Restore the default selection from Settings"
              style={{ fontSize: 11.5, padding: '3px 8px' }}
            >
              <RotateCcw size={12} />
              <span>Default</span>
            </button>
          </div>
        </div>

        {/* Filter & Search Bar inside modal */}
        <div
          style={{
            padding: '12px 20px',
            display: 'flex',
            gap: 10,
            alignItems: 'center',
            borderBottom: '1px solid var(--cockpit-border)',
            flexWrap: 'wrap',
          }}
        >
          <div style={{ position: 'relative', flex: 1, minWidth: 200 }}>
            <Search
              size={15}
              style={{ position: 'absolute', left: 10, top: '50%', transform: 'translateY(-50%)', color: 'var(--text-dim)' }}
            />
            <input
              type="text"
              className="field-input"
              style={{ width: '100%', paddingLeft: 32, fontSize: 12.5 }}
              placeholder="Find a source by name or id (PubMed, VJOL, arXiv, Crossref…)"
              value={searchTerm}
              onChange={(e) => setSearchTerm(e.target.value)}
              autoFocus
            />
            {searchTerm && (
              <button
                type="button"
                onClick={() => setSearchTerm('')}
                style={{
                  position: 'absolute',
                  right: 8,
                  top: '50%',
                  transform: 'translateY(-50%)',
                  background: 'none',
                  border: 'none',
                  cursor: 'pointer',
                  color: 'var(--text-dim)',
                }}
              >
                <X size={13} />
              </button>
            )}
          </div>

          <select
            className="field-input"
            style={{ width: 'auto', fontSize: 12, padding: '6px 10px' }}
            value={activeGroupFilter}
            onChange={(e) => setActiveGroupFilter(e.target.value)}
          >
            <option value="all">All source groups ({allSources.length})</option>
            {groups.map((g) => (
              <option key={g} value={g}>
                {g}
              </option>
            ))}
          </select>
        </div>

        {/* Source Items Grid */}
        <div
          style={{
            flex: 1,
            overflowY: 'auto',
            padding: '14px 20px',
            display: 'grid',
            gridTemplateColumns: 'repeat(auto-fill, minmax(320px, 1fr))',
            gap: 10,
          }}
        >
          {filteredSources.map((source) => {
            const checked = isSelected(source.id);
            const needsApiKey = source.credentials && source.credentials.length > 0;
            return (
              <div
                key={source.id}
                onClick={() => toggleSource(source.id)}
                style={{
                  display: 'flex',
                  alignItems: 'flex-start',
                  gap: 10,
                  padding: '10px 12px',
                  borderRadius: 'var(--radius-md)',
                  border: `1px solid ${checked ? 'var(--primary-cyan)' : 'var(--cockpit-border)'}`,
                  background: checked ? 'var(--primary-cyan-bg)' : 'var(--cockpit-card)',
                  cursor: 'pointer',
                  transition: 'all 0.15s ease',
                }}
              >
                <input
                  type="checkbox"
                  checked={checked}
                  onChange={() => toggleSource(source.id)}
                  style={{
                    marginTop: 3,
                    accentColor: 'var(--primary-cyan)',
                    cursor: 'pointer',
                  }}
                  onClick={(e) => e.stopPropagation()}
                />
                <div className="u-grow">
                  <div style={{ display: 'flex', alignItems: 'center', justifyContent: 'space-between', gap: 6 }}>
                    <span
                      style={{
                        fontWeight: 600,
                        fontSize: 13,
                        color: checked ? 'var(--primary-cyan-hover)' : 'var(--text-main)',
                      }}
                    >
                      {source.name}
                    </span>
                    <span style={{ fontSize: 10, color: 'var(--text-dim)', fontFamily: 'var(--font-mono)' }}>
                      {source.id}
                    </span>
                  </div>

                  <div style={{ fontSize: 11.5, color: 'var(--text-muted)', marginTop: 2, lineHeight: 1.3 }}>
                    {source.desc}
                  </div>

                  <div style={{ display: 'flex', gap: 6, alignItems: 'center', marginTop: 6, flexWrap: 'wrap' }}>
                    <span className="badge" style={{ fontSize: 10, padding: '1px 6px' }}>
                      {source.group}
                    </span>
                    {needsApiKey && (
                      <span
                        className="badge"
                        style={{
                          fontSize: 10,
                          padding: '1px 6px',
                          display: 'inline-flex',
                          alignItems: 'center',
                          gap: 3,
                          color: 'var(--status-amber)',
                          background: 'var(--status-amber-bg)',
                        }}
                      >
                        <KeyRound size={9} /> API Key / Session
                      </span>
                    )}
                  </div>
                </div>
              </div>
            );
          })}
        </div>

        {/* Modal Footer */}
        <div className="modal-footer" style={{ justifyContent: 'space-between', alignItems: 'center' }}>
          <div className="text-sm text-muted">
            {tempSelected.length === 0 ? (
              <span>Using the default discipline / Settings selection</span>
            ) : (
              <span>
                Selected <b>{tempSelected.length}</b> custom academic sources
              </span>
            )}
          </div>
          <div className="u-flex">
            <button type="button" className="action-btn" onClick={onClose}>
              Cancel
            </button>
            <button type="button" className="action-btn action-btn-primary" onClick={handleApply}>
              <ShieldCheck size={14} />
              <span>Apply limits</span>
            </button>
          </div>
        </div>
      </div>
    </div>
  );
};
