import React, { useEffect, useMemo, useState } from 'react';
import { Search, X } from 'lucide-react';
import { gatewayFetch } from '../../lib/gateway';
import { buildMeshQuery, MeshField, MeshGroup, QueryPreviewItem } from './meshQuery';

const PREVIEW_MODES = [
  ['pubmed_mesh', 'Raw MeSH'],
  ['europe_pmc', 'Europe PMC translation'],
  ['arxiv', 'arXiv translation'],
  ['native_boolean', 'Raw Boolean'],
  ['plain_keywords', 'Safe keywords'],
] as const;

interface MeshBuilderProps {
  /** The sources the query would go to, for the per-source syntax preview. */
  sources?: string[];
  busy: boolean;
  onApply: (query: string) => void;
}

/** Builds a MeSH/Boolean query and previews how each selected source will receive it. */
export const MeshBuilder: React.FC<MeshBuilderProps> = ({ sources, busy, onApply }) => {
  const [groups, setGroups] = useState<MeshGroup[]>([
    { terms: '', field: 'mh' },
    { terms: '', field: 'tiab' },
  ]);
  const [operator, setOperator] = useState<'AND' | 'OR'>('AND');
  const [exclusions, setExclusions] = useState('');
  const [explode, setExplode] = useState(true);
  const [preview, setPreview] = useState<QueryPreviewItem[]>([]);
  const query = useMemo(() => buildMeshQuery(groups, operator, exclusions, explode), [groups, operator, exclusions, explode]);
  const sourcesKey = sources?.join(',') ?? '';

  useEffect(() => {
    if (!query) {
      setPreview([]);
      return;
    }
    const timer = window.setTimeout(() => {
      void gatewayFetch('/api/query/preview', {
        method: 'POST',
        headers: { 'Content-Type': 'application/json' },
        body: JSON.stringify({ query, sources: sourcesKey ? sourcesKey.split(',') : undefined }),
      })
        .then((response) => (response.ok ? response.json() : Promise.reject()))
        .then((data) => setPreview(Array.isArray(data?.sources) ? data.sources : []))
        .catch(() => setPreview([]));
    }, 250);
    return () => window.clearTimeout(timer);
  }, [query, sourcesKey]);

  const updateGroup = (index: number, patch: Partial<MeshGroup>) =>
    setGroups((current) => current.map((item, i) => (i === index ? { ...item, ...patch } : item)));

  return (
    <section aria-labelledby="mesh-builder-heading" className="mesh-builder">
      <div className="u-row u-between">
        <div>
          <h2 id="mesh-builder-heading">MeSH & Boolean</h2>
          <p>
            Each line is a synonym (OR); groups combine with AND/OR. PubMed uses raw MeSH,
            and other sources are translated into their own syntax automatically.
          </p>
        </div>
        <label className="check-label">
          <input id="mesh-explode" type="checkbox" checked={explode} onChange={(event) => setExplode(event.target.checked)} />
          Explode the MeSH tree
        </label>
      </div>

      <div className="mesh-groups">
        {groups.map((group, index) => (
          <div key={index} className="mesh-row">
            <textarea
              id={`mesh-group-${index}`}
              aria-label={`Concept group ${index + 1}`}
              className="field-input"
              rows={2}
              placeholder={index === 0 ? 'heart failure\ncardiac failure' : 'drug therapy\ntreatment'}
              value={group.terms}
              onChange={(event) => updateGroup(index, { terms: event.target.value })}
            />
            <select
              aria-label={`Search field for group ${index + 1}`}
              className="field-input"
              value={group.field}
              onChange={(event) => updateGroup(index, { field: event.target.value as MeshField })}
            >
              <option value="mh">MeSH Terms</option>
              <option value="majr">MeSH Major Topic</option>
              <option value="tiab">Title / Abstract</option>
              <option value="ti">Title</option>
              <option value="all">All Fields</option>
            </select>
            <button
              type="button"
              className="action-btn"
              aria-label={`Remove group ${index + 1}`}
              disabled={groups.length === 1}
              onClick={() => setGroups((current) => current.filter((_, i) => i !== index))}
            >
              <X size={14} />
            </button>
          </div>
        ))}
      </div>

      <div className="u-row mesh-controls">
        <button id="mesh-add-group" type="button" className="action-btn" onClick={() => setGroups((current) => [...current, { terms: '', field: 'tiab' }])}>
          Add group
        </button>
        <label htmlFor="mesh-operator" className="text-muted text-sm">Combine groups</label>
        <select id="mesh-operator" className="field-input field-operator" value={operator} onChange={(event) => setOperator(event.target.value as 'AND' | 'OR')}>
          <option value="AND">AND</option>
          <option value="OR">OR</option>
        </select>
        <input
          id="mesh-exclusions"
          className="field-input field-grow"
          placeholder="Exclusions (one per line, or separated by |)"
          value={exclusions}
          onChange={(event) => setExclusions(event.target.value)}
        />
      </div>

      {query && (
        <div className="mesh-output">
          <code id="mesh-query-preview" className="mesh-preview">{query}</code>
          <div aria-label="Syntax compatibility per source" className="u-row mesh-modes">
            {PREVIEW_MODES.map(([mode, label]) => {
              const matching = preview.filter((item) => item.mode === mode);
              return matching.length ? (
                <span key={mode} className="quick-pill" title={matching.map((item) => item.id).join(', ')}>
                  {label}: {matching.length}
                </span>
              ) : null;
            })}
          </div>
          <button id="mesh-apply-search" type="button" className="search-submit-btn u-self-start" disabled={busy} onClick={() => onApply(query)}>
            <Search size={14} /> Apply & search
          </button>
        </div>
      )}
    </section>
  );
};
