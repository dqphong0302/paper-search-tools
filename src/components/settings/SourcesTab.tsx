import React, { useMemo, useRef, useState } from 'react';
import {
  Activity, BarChart3, BookOpen, Check, ChevronDown, ChevronUp, Cpu, Database, Globe, Layers, Radio, RotateCcw, Search, Sparkles, Stethoscope,
} from 'lucide-react';
import searchCatalog from '../../lib/searchCatalog.json';
import { gatewayFetch } from '../../lib/gateway';
import {
  CREDENTIAL_LABELS, DEFAULT_PRESET, DEFAULT_SOURCES, DOMAIN_GROUPS, isConfiguredSecret, PRESETS, PRIMARY_PRESETS, SettingsFormApi, SourceHealth, SOURCES_LIST,
} from './model';
import { Card } from './ui';

const GROUP_ICONS: [string, React.ReactNode][] = [
  ['Biomedical', <Stethoscope size={15} color="#ef4444" />],
  ['CS', <Cpu size={15} color="#3b82f6" />],
  ['Engineering', <Cpu size={15} color="#3b82f6" />],
  ['Vietnamese', <BookOpen size={15} color="#eab308" />],
  ['Syntheses', <Sparkles size={15} color="#8b5cf6" />],
  ['Data Repositories', <Database size={15} color="#10b981" />],
  ['Economics', <BarChart3 size={15} color="#f97316" />],
  ['Physics', <Radio size={15} color="#06b6d4" />],
];
const groupIcon = (name: string) => GROUP_ICONS.find(([key]) => name.includes(key))?.[1] ?? <Globe size={15} color="#0ea5e9" />;
const SOURCE_GROUP_NAMES = ['all', ...Array.from(new Set(SOURCES_LIST.map((s) => s.group)))];
const OTHER_PRESETS = PRESETS.filter((p) => !PRIMARY_PRESETS.some((prim) => prim.id === p.id));

type HealthTone = 'muted' | 'amber' | 'emerald' | 'rose';

/** One line describing the last probe of a source, in the source's own words. */
function healthLabel(state: SourceHealth | undefined): { text: string; tone: HealthTone } | null {
  if (!state) return null;
  if (state.loading) return { text: 'Checking…', tone: 'muted' };
  if (state.needsSetup) return { text: state.error || 'Needs credentials', tone: 'amber' };
  if (state.ok) {
    const seconds = state.elapsedMs !== undefined ? ` · ${(state.elapsedMs / 1000).toFixed(1)}s` : '';
    return {
      text: state.count === 0 ? `Reachable, 0 results for “${state.query}”${seconds}` : `${state.count} results${seconds}`,
      tone: 'emerald',
    };
  }
  return { text: state.error || 'Failed', tone: 'rose' };
}

function useSourceHealth() {
  const [health, setHealth] = useState<Record<string, SourceHealth>>({});
  const [progress, setProgress] = useState<{ done: number; total: number } | null>(null);
  const runId = useRef(0);

  /** Probe one source through the gateway and record exactly what came back. */
  const check = async (id: string): Promise<SourceHealth> => {
    setHealth((prev) => ({ ...prev, [id]: { ...prev[id], loading: true } }));
    let result: SourceHealth;
    try {
      const res = await gatewayFetch('/api/source/check', {
        method: 'POST',
        headers: { 'Content-Type': 'application/json' },
        body: JSON.stringify({ id }),
      });
      const data = await res.json().catch(() => null);
      if (!res.ok) throw new Error(data?.error || `HTTP ${res.status}`);
      result = {
        loading: false,
        ok: data.ok === true,
        count: data.count ?? 0,
        elapsedMs: data.elapsed_ms,
        needsSetup: data.needs_setup === true,
        error: data.error ?? null,
        query: data.query,
        checkedAt: Date.now(),
      };
    } catch (e) {
      result = { loading: false, ok: false, error: (e as Error).message, checkedAt: Date.now() };
    }
    setHealth((prev) => ({ ...prev, [id]: result }));
    return result;
  };

  /** Check several sources, a few at a time so one slow site cannot stall the
   *  rest and the gateway is not hit with fifty parallel searches. */
  const checkAll = async (ids: string[]) => {
    if (ids.length === 0) return;
    const run = ++runId.current;
    setProgress({ done: 0, total: ids.length });
    const queue = [...ids];
    const worker = async () => {
      while (queue.length) {
        const id = queue.shift();
        if (!id || runId.current !== run) return;
        await check(id);
        setProgress((prev) => (prev ? { ...prev, done: prev.done + 1 } : prev));
      }
    };
    await Promise.all(Array.from({ length: Math.min(4, ids.length) }, worker));
    if (runId.current === run) setProgress(null);
  };

  const stop = () => {
    runId.current += 1;
    setProgress(null);
  };

  return { health, progress, check, checkAll, stop };
}

export const SourcesTab: React.FC<SettingsFormApi> = ({ config, update }) => {
  const [sourceSearch, setSourceSearch] = useState('');
  const [selectedGroup, setSelectedGroup] = useState('all');
  const [expanded, setExpanded] = useState<Record<string, boolean>>({});
  const { health, progress, check, checkAll, stop } = useSourceHealth();

  const currentSources = (config.enabled_sources || '').split(',').map((s) => s.trim().toLowerCase()).filter(Boolean);
  const isCustom = config.domain_preset === 'custom';
  const isActive = (id: string): boolean =>
    isCustom
      ? currentSources.includes(id)
      : searchCatalog.presets.find((preset) => preset.id === config.domain_preset)?.sources.includes(id) ?? false;
  const activeIds = () => SOURCES_LIST.map((s) => s.id).filter(isActive);
  const activeCount = activeIds().length;
  const setCustom = (ids: string[]) => update({ domain_preset: 'custom', enabled_sources: ids.join(',') });
  const credentialReady = (key: string) => {
    const value = (config as Record<string, string>)[key] ?? '';
    return CREDENTIAL_LABELS[key]?.secret ? isConfiguredSecret(value) || value.trim().length > 0 : value.trim().length > 0;
  };

  const toggleSource = (id: string) => {
    const list = isCustom ? currentSources : activeIds();
    setCustom(list.includes(id) ? list.filter((s) => s !== id) : [...list, id]);
  };

  const toggleGroup = (name: string) => {
    const ids = DOMAIN_GROUPS.find((g) => g.name === name)?.sources.map((s) => s.id);
    if (!ids) return;
    const list = isCustom ? currentSources : activeIds();
    const allOn = ids.every((id) => list.includes(id));
    setCustom(allOn ? list.filter((id) => !ids.includes(id)) : Array.from(new Set([...list, ...ids])));
  };

  const handlePresetChange = (presetId: string) => {
    // Leaving a preset for Custom keeps the sources that were active a moment
    // ago; starting from an empty list would silently disable every source.
    const seeded = isCustom ? currentSources : activeIds();
    update({
      domain_preset: presetId,
      enabled_sources: presetId === 'custom'
        ? seeded.join(',')
        : searchCatalog.presets.find((preset) => preset.id === presetId)?.sources.join(',') ?? '',
    });
  };

  const filteredGroups = useMemo(() => {
    const q = sourceSearch.trim().toLowerCase();
    return DOMAIN_GROUPS
      .filter((g) => selectedGroup === 'all' || g.name === selectedGroup)
      .map((g) => q ? {
        ...g,
        sources: g.sources.filter((s) => s.name.toLowerCase().includes(q) || s.id.toLowerCase().includes(q) || (s.desc && s.desc.toLowerCase().includes(q))),
      } : g)
      .filter((g) => g.sources.length > 0);
  }, [sourceSearch, selectedGroup]);

  const activePreset = PRESETS.find((p) => p.id === config.domain_preset);

  return (
    <div className="u-stack u-gap-14">
      <Card
        title="Domain Presets"
        subtitle="Quickly configure optimal search sources tailored for your research goal"
        icon={<Layers size={16} className="icon-accent" />}
        right={
          <select aria-label="Select specialized preset" className="settings-input settings-input-sm preset-select" value={config.domain_preset} onChange={(e) => handlePresetChange(e.target.value)}>
            <option value="custom">🛠️ Custom Selection</option>
            <optgroup label="Recommended Presets">
              {PRIMARY_PRESETS.filter((p) => p.id !== 'custom').map((p) => <option key={p.id} value={p.id}>{p.label}</option>)}
            </optgroup>
            <optgroup label={`All ${OTHER_PRESETS.length} specialised presets`}>
              {OTHER_PRESETS.map((p) => <option key={p.id} value={p.id}>{p.label}</option>)}
            </optgroup>
          </select>
        }
      >
        <div className="u-row u-gap-6">
          {PRIMARY_PRESETS.map((p) => (
            <button key={p.id} id={`preset-${p.id}`} type="button" onClick={() => handlePresetChange(p.id)} className={`quick-pill choice-pill ${config.domain_preset === p.id ? 'selected' : ''}`}>
              {p.label}
            </button>
          ))}
        </div>

        <div className="preset-summary">
          <div>
            <strong>{isCustom ? 'Custom Selection Mode' : (activePreset?.label || config.domain_preset)}</strong>
            <span>: </span>
            <span>
              {isCustom
                ? `${currentSources.length}/${SOURCES_LIST.length} sources manually selected below.`
                : (activePreset?.description || 'Auto-selects compatible sources.')}
            </span>
          </div>
          <span className="u-row">
            <span className="selection-count">{activeCount} sources active</span>
            <button
              id="restore-default-sources"
              type="button"
              className="action-btn action-btn-2xs"
              onClick={() => update({ domain_preset: DEFAULT_PRESET, enabled_sources: DEFAULT_SOURCES.join(',') })}
              title={`Back to the default selection: ${DEFAULT_SOURCES.join(', ')}`}
            >
              <RotateCcw size={11} />
              <span>Restore defaults</span>
            </button>
          </span>
        </div>
      </Card>

      <Card
        title="Search Sources by Domain"
        subtitle={`${activeCount}/${SOURCES_LIST.length} active sources • ${DOMAIN_GROUPS.length} domain categories`}
        icon={<Globe size={16} className="icon-accent" />}
        right={
          <div className="u-row u-gap-6">
            <div className="search-field-sm">
              <Search size={12} />
              <input type="text" className="settings-input settings-input-sm" placeholder={`Filter ${SOURCES_LIST.length} sources...`} value={sourceSearch} onChange={(e) => setSourceSearch(e.target.value)} />
            </div>
            <button type="button" className="action-btn action-btn-2xs" onClick={() => setCustom(SOURCES_LIST.map((s) => s.id))}>Enable All</button>
            <button type="button" className="action-btn action-btn-2xs" onClick={() => setCustom([])}>Disable All</button>
            <button
              id="check-active-sources"
              type="button"
              className="action-btn action-btn-2xs"
              onClick={() => (progress ? stop() : void checkAll(activeIds()))}
              title="Send one real query to every active source and report what each one answers"
            >
              <Activity size={11} className={progress ? 'animate-spin' : ''} />
              <span>{progress ? `Stop (${progress.done}/${progress.total})` : 'Check active sources'}</span>
            </button>
            <button type="button" className="action-btn action-btn-2xs" onClick={() => setExpanded(Object.fromEntries(DOMAIN_GROUPS.map((g) => [g.name, true])))}>Expand All</button>
            <button type="button" className="action-btn action-btn-2xs" onClick={() => setExpanded({})}>Collapse All</button>
          </div>
        }
      >
        <div className="u-row u-gap-6">
          {SOURCE_GROUP_NAMES.map((grp) => {
            const count = grp === 'all' ? SOURCES_LIST.length : SOURCES_LIST.filter((s) => s.group === grp).length;
            return (
              <button key={grp} type="button" onClick={() => setSelectedGroup(grp)} className={`action-btn choice-pill choice-pill-sm ${selectedGroup === grp ? 'selected' : ''}`}>
                {grp === 'all' ? 'All' : grp} ({count})
              </button>
            );
          })}
        </div>

        <div className="u-stack u-gap-10">
          {filteredGroups.map((group) => {
            const groupActive = group.sources.filter((s) => isActive(s.id)).length;
            const total = group.sources.length;
            const allActive = groupActive === total && total > 0;
            const isExpanded = Boolean(sourceSearch.trim()) || Boolean(expanded[group.name]);

            return (
              <div key={group.name} className={`settings-domain-card ${allActive ? 'all-active' : ''}`}>
                <div className="u-row u-between u-gap-10">
                  <div className="u-row">
                    {groupIcon(group.name)}
                    <span className="domain-name">{group.name}</span>
                    <span className={`cockpit-badge badge-xs ${allActive ? 'badge-emerald' : groupActive > 0 ? 'badge-cyan' : 'badge-vjol'}`}>
                      {groupActive}/{total} sources
                    </span>
                  </div>
                  <div className="u-row u-gap-6">
                    <button type="button" className="action-btn action-btn-2xs" onClick={() => toggleGroup(group.name)}>
                      {allActive ? 'Disable Group' : 'Enable Group'}
                    </button>
                    <button type="button" className="action-btn action-btn-2xs" onClick={() => setExpanded((prev) => ({ ...prev, [group.name]: !prev[group.name] }))}>
                      <span>{isExpanded ? 'Collapse' : `Details (${total})`}</span>
                      {isExpanded ? <ChevronUp size={12} /> : <ChevronDown size={12} />}
                    </button>
                  </div>
                </div>

                {!isExpanded && (
                  <div className="u-row u-gap-4">
                    {group.sources.map((s) => {
                      const active = isActive(s.id);
                      const label = healthLabel(health[s.id]);
                      return (
                        <span
                          key={s.id}
                          className={`settings-source-pill ${active ? 'active' : 'inactive'}`}
                          onClick={() => toggleSource(s.id)}
                          title={label ? `${s.name} — ${label.text}` : active ? `${s.name} (Active — click to disable)` : `${s.name} (Inactive — click to enable)`}
                        >
                          {active && <Check size={10} strokeWidth={3} />}
                          <span>{s.name}</span>
                          {label && <span aria-hidden="true" className={`health-dot tone-${label.tone === 'muted' ? 'dim' : label.tone}`} />}
                        </span>
                      );
                    })}
                  </div>
                )}

                {isExpanded && (
                  <div className="source-detail-grid">
                    {group.sources.map((src) => {
                      const enabled = isActive(src.id);
                      const creds = src.credentials as string[];
                      const ready = creds.every(credentialReady);
                      const label = healthLabel(health[src.id]);
                      return (
                        <div
                          key={src.id}
                          role="button"
                          tabIndex={0}
                          className={`source-detail-card ${enabled ? 'enabled' : ''}`}
                          onClick={() => toggleSource(src.id)}
                          onKeyDown={(e) => {
                            if (e.key === 'Enter' || e.key === ' ') {
                              e.preventDefault();
                              toggleSource(src.id);
                            }
                          }}
                        >
                          <div className="u-row u-between u-gap-6">
                            <span className="source-detail-name">{src.name}</span>
                            <span className={`cockpit-badge badge-xs ${enabled ? 'badge-emerald' : 'badge-vjol'}`}>{enabled ? 'ENABLED' : 'DISABLED'}</span>
                          </div>
                          {src.desc && <span className="source-detail-desc">{src.desc}</span>}
                          {creds.length > 0 && (
                            <span className={`source-detail-creds tone-${ready ? 'emerald' : 'amber'}`}>
                              {ready ? '✓ credentials ready' : `requires: ${creds.map((k) => CREDENTIAL_LABELS[k]?.label ?? k).join(', ')}`}
                            </span>
                          )}
                          <div className="u-row u-gap-6 source-detail-actions" onClick={(e) => e.stopPropagation()}>
                            <button
                              id={`check-source-${src.id}`}
                              type="button"
                              className="action-btn action-btn-2xs"
                              disabled={health[src.id]?.loading}
                              onClick={() => void check(src.id)}
                              title={`Query ${src.name} once and report the result`}
                            >
                              <Activity size={10} className={health[src.id]?.loading ? 'animate-spin' : ''} />
                              <span>{health[src.id]?.loading ? 'Checking…' : 'Check'}</span>
                            </button>
                            {label && <span className={`source-health tone-${label.tone}`} data-testid={`health-${src.id}`}>{label.text}</span>}
                          </div>
                        </div>
                      );
                    })}
                  </div>
                )}
              </div>
            );
          })}
        </div>
      </Card>
    </div>
  );
};
