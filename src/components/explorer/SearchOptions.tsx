import { forwardRef } from 'react';
import { Calendar, Check, Layers, Search, SlidersHorizontal } from 'lucide-react';
import { MeshBuilder } from './MeshBuilder';
import { AVAILABLE_PRESETS, QUICK_DISCIPLINES, SCOPES, Scope, SearchFilters, sourcesFor } from './searchConfig';

interface SearchOptionsProps {
  filters: SearchFilters;
  onChange: (patch: Partial<SearchFilters>) => void;
  onScopeChange: (scope: Scope) => void;
  onReset: () => void;
  hasQuery: boolean;
  busy: boolean;
  onSubmit: () => void;
  onApplyMesh: (query: string) => void;
}

const thisYear = () => new Date().getFullYear();

/** The collapsed "Advanced filters" panel: disciplines, MeSH builder, years and limit. */
export const SearchOptions = forwardRef<HTMLDetailsElement, SearchOptionsProps>(({
  filters, onChange, onScopeChange, onReset, hasQuery, busy, onSubmit, onApplyMesh,
}, ref) => {
  const { scope, customSources, yearMin, yearMax, resultLimit } = filters;
  const custom = customSources.length > 0;
  const preset = AVAILABLE_PRESETS.find((p) => p.id === scope);
  const yearPill = (label: string, min: string) => (
    <button
      type="button"
      className={`quick-pill ${yearMin === min && !yearMax ? 'active' : ''}`}
      onClick={() => onChange({ yearMin: min, yearMax: '' })}
    >
      {label}
    </button>
  );

  return (
    <details ref={ref} id="search-options" className="compact-options">
      <summary className="u-row">
        <SlidersHorizontal size={14} className="icon-accent" />
        <span>
          Advanced filters · {preset?.label || 'Default'} ·{' '}
          {yearMin || yearMax ? `${yearMin || '…'}–${yearMax || '…'}` : 'All years'}
          {custom ? ` · ${customSources.length} custom sources` : ''}
        </span>
      </summary>
      <div className="compact-options-body">
        <section className="discipline-picker" aria-labelledby="discipline-heading">
          <div>
            <h2 id="discipline-heading">8 standardised discipline groups & 4 modes</h2>
            <p>Pick a discipline group to tune the sources. Your keywords are left untouched.</p>
          </div>
          <div className="discipline-grid" role="group" aria-label="Discipline selection grid">
            {QUICK_DISCIPLINES.map((p) => (
              <button
                id={`discipline-${p.id}`}
                key={p.id}
                type="button"
                className={`discipline-card ${scope === p.id && !custom ? 'active' : ''}`}
                aria-pressed={scope === p.id}
                title={p.description}
                onClick={() => onScopeChange(p.id)}
              >
                <span>{p.label}</span>
                <small>
                  {p.sources.length} sources{' '}
                  {scope === p.id && !custom && <Check size={13} aria-hidden="true" />}
                </small>
              </button>
            ))}
          </div>
        </section>

        <MeshBuilder sources={sourcesFor(scope, customSources)} busy={busy} onApply={onApplyMesh} />

        <div className="u-row u-between">
          <div className="u-row">
            <div className="inline-label">
              <Layers size={14} className="icon-accent" />
              <label htmlFor="search-scope">All source groups</label>
            </div>
            <select id="search-scope" aria-label="Search scope" className="field-input field-sm field-scope" value={scope} onChange={(e) => onScopeChange(e.target.value)}>
              {SCOPES.map((s) => (
                <option key={s.id} value={s.id}>{s.label}</option>
              ))}
            </select>

            <div className="divider-v" />

            <div className="inline-label">
              <Calendar size={14} className="icon-accent" />
              <span>Year</span>
            </div>
            <div className="quick-pill-group">
              <button type="button" className={`quick-pill ${!yearMin && !yearMax ? 'active' : ''}`} onClick={() => onChange({ yearMin: '', yearMax: '' })}>
                All
              </button>
              {yearPill('Last 5 years', String(thisYear() - 5))}
              {yearPill('Last 3 years', String(thisYear() - 3))}
              {yearPill('This year', String(thisYear()))}
            </div>

            <div className="u-row u-gap-4">
              <input id="search-year-min" aria-label="From year" className="field-input field-year" type="number" min="1000" max="9999" placeholder="From" value={yearMin} onChange={(e) => onChange({ yearMin: e.target.value })} />
              <span className="text-dim text-xs">-</span>
              <input id="search-year-max" aria-label="To year" className="field-input field-year" type="number" min="1000" max="9999" placeholder="To" value={yearMax} onChange={(e) => onChange({ yearMax: e.target.value })} />
            </div>
          </div>

          <div className="u-row">
            <span className="text-muted text-xs">Result limit:</span>
            <input id="search-limit" aria-label="Result limit" className="field-input field-year field-limit" type="number" min="1" max="50" placeholder="20" value={resultLimit} onChange={(e) => onChange({ resultLimit: e.target.value })} />
          </div>
        </div>

        <div className="search-options-footer">
          <p>{hasQuery ? 'Filters apply as soon as you press Search.' : 'Type a keyword in the search bar above to begin.'}</p>
          <button id="search-reset-options" type="button" className="action-btn" onClick={onReset}>
            Reset filters
          </button>
          <button id="search-apply-options" type="button" className="search-submit-btn" disabled={busy || !hasQuery} onClick={onSubmit}>
            <Search size={14} /> {busy ? 'Searching…' : 'Search with filters'}
          </button>
        </div>
      </div>
    </details>
  );
});
SearchOptions.displayName = 'SearchOptions';
