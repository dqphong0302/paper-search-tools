import React from 'react';
import { Filter } from 'lucide-react';
import { AVAILABLE_PRESETS, AVAILABLE_SOURCE_IDS, QUICK_DISCIPLINES, Scope } from './searchConfig';

interface ScopeBarProps {
  scope: Scope;
  customSources: string[];
  onScopeChange: (scope: Scope) => void;
  onOpenLimiter: () => void;
}

/** Quick discipline pills and the source-limiter button, on one line above the results. */
export const ScopeBar: React.FC<ScopeBarProps> = ({ scope, customSources, onScopeChange, onOpenLimiter }) => {
  const custom = customSources.length > 0;
  const preset = AVAILABLE_PRESETS.find((p) => p.id === scope);
  return (
    <div className="scope-bar">
      <div className="quick-pill-row">
        <span className="eyebrow-label">Discipline:</span>
        <button
          type="button"
          className={`quick-pill ${scope === 'default' && !custom ? 'active' : ''}`}
          onClick={() => onScopeChange('default')}
          title="Use the default source selection from Settings"
        >
          Default
        </button>
        {QUICK_DISCIPLINES.slice(0, 6).map((p) => (
          <button
            key={p.id}
            type="button"
            className={`quick-pill ${scope === p.id && !custom ? 'active' : ''}`}
            onClick={() => onScopeChange(p.id)}
            title={p.description}
          >
            <span>{p.label}</span>
          </button>
        ))}
      </div>

      <button
        type="button"
        className={`action-btn action-btn-sm ${custom ? 'action-btn-primary' : ''}`}
        onClick={onOpenLimiter}
        title={`Open the limiter to customise the ${AVAILABLE_SOURCE_IDS.size} active sources`}
      >
        <Filter size={13} />
        <span>
          {custom
            ? `Custom (${customSources.length} sources)`
            : preset
            ? `Sources: ${preset.label}`
            : `Limit sources (${AVAILABLE_SOURCE_IDS.size})`}
        </span>
      </button>
    </div>
  );
};
