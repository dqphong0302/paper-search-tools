import React, { useState, useEffect, useMemo } from 'react';
import {
  Activity,
  AlertTriangle,
  BarChart3,
  BookOpen,
  Check,
  CheckCircle2,
  ChevronDown,
  ChevronUp,
  Cpu,
  Database,
  ExternalLink,
  Eye,
  EyeOff,
  Globe,
  Layers,
  LogIn,
  Plug,
  Radio,
  RotateCcw,
  RefreshCw,
  Save,
  Search,
  Server,
  Settings,
  ShieldCheck,
  Sparkles,
  Stethoscope,
  Terminal,
  Trash2,
  Zap,
} from 'lucide-react';

import searchCatalog from '../lib/searchCatalog.json';
import { AiClients } from './AiClients';
import { invoke, isTauri } from '@tauri-apps/api/core';
import { gatewayFetch, setGatewayToken } from '../lib/gateway';

interface SettingsPageProps {
  port: number;
}

// The catalog also lists sources the engine cannot query yet; never offer those.
const ALL_SOURCES = searchCatalog.sources;
const SOURCES_LIST = ALL_SOURCES.filter((source) => source.available !== false);
const PRESETS = searchCatalog.presets.filter((preset) => preset.id === 'custom' || preset.sources.length > 0);

// Stored credentials are never sent to the UI; the backend substitutes this
// sentinel and ignores it on write, so an untouched field keeps the secret.
const KEEP_SENTINEL = '__SG_KEEP__';
const isConfiguredSecret = (value: string | undefined) => value === KEEP_SENTINEL;

const CREDENTIAL_LABELS: Record<string, { label: string; secret: boolean; type?: string; placeholder?: string }> = {
  openai_api_key: { label: 'OpenAI API Key', secret: true, placeholder: 'sk-...' },
  anthropic_api_key: { label: 'Anthropic API Key', secret: true, placeholder: 'sk-ant-...' },
  gemini_api_key: { label: 'Google Gemini API Key', secret: true, placeholder: 'AIza...' },
  deepseek_api_key: { label: 'DeepSeek API Key', secret: true, placeholder: 'sk-...' },
  groq_api_key: { label: 'Groq API Key', secret: true, placeholder: 'gsk_...' },
  openalex_email: { label: 'Contact Email (Polite Pool)', secret: false, type: 'email', placeholder: 'researcher@university.edu' },
  openalex_api_key: { label: 'API Key', secret: true },
  semantic_scholar_api_key: { label: 'API Key', secret: true },
  crossref_email: { label: 'Contact Email', secret: false, type: 'email', placeholder: 'researcher@university.edu' },
  ncbi_api_key: { label: 'API Key', secret: true },
  ncbi_email: { label: 'Registered Email', secret: false, type: 'email', placeholder: 'researcher@university.edu' },
  unpaywall_email: { label: 'Contact Email', secret: false, type: 'email' },
  scopus_api_key: { label: 'Elsevier Scopus API Key', secret: true, placeholder: 'Scopus Key...' },
  ieee_api_key: { label: 'IEEE Xplore API Key', secret: true, placeholder: 'IEEE Key...' },
  springer_api_key: { label: 'Springer Nature API Key', secret: true, placeholder: 'Springer Key...' },
  perplexity_api_key: { label: 'Perplexity API Key', secret: true, placeholder: 'pplx-...' },
  core_api_key: { label: 'CORE API Key', secret: true, placeholder: 'CORE Key...' },
  dimensions_api_key: { label: 'Dimensions API Key', secret: true, placeholder: 'Dimensions Key...' },
  wos_api_key: { label: 'Web of Science API Key', secret: true, placeholder: 'Clarivate Key...' },
  consensus_session: { label: 'Consensus Session (Token / Cookie)', secret: true, placeholder: 'Paste __session=... or Bearer JWT...' },
  openevidence_session: { label: 'OpenEvidence Session (Cookie)', secret: true, placeholder: 'Paste cookie or session token...' },
  openai_session: { label: 'OpenAI Console Session', secret: true },
  anthropic_session: { label: 'Anthropic Console Session', secret: true },
  gemini_session: { label: 'Google AI Studio Session', secret: true },
  deepseek_session: { label: 'DeepSeek Console Session', secret: true },
  perplexity_session: { label: 'Perplexity Session', secret: true },
  mcp_auth_token: { label: 'Gateway Auth Token', secret: true },
};

/** Group names now ship in English from searchCatalog.json; kept as a seam for future i18n. */
const getGroupLabel = (name: string) => name;

type Tab = 'sources' | 'connections' | 'clients' | 'gateway';

/** Outcome of a single source health probe, as the sources tab renders it. */
interface SourceHealth {
  loading: boolean;
  ok?: boolean;
  count?: number;
  elapsedMs?: number;
  needsSetup?: boolean;
  error?: string | null;
  query?: string;
  checkedAt?: number;
}

/** The sources a fresh install searches until the user changes anything. */
const DEFAULT_PRESET = 'auto';
const DEFAULT_SOURCES =
  searchCatalog.presets.find((preset) => preset.id === DEFAULT_PRESET)?.sources ?? [];

const FIELD_STYLE: React.CSSProperties = {
  background: '#ffffff',
  border: '1px solid #cbd5e1',
  borderRadius: 'var(--radius-sm)',
  padding: '6px 10px',
  fontSize: 12,
  width: '100%',
};

const Card: React.FC<{ title?: string; subtitle?: string; icon?: React.ReactNode; right?: React.ReactNode; children: React.ReactNode; advanced?: boolean }> = ({ title, subtitle, icon, right, children, advanced }) => advanced ? (
  <details className="compact-options">
    <summary>{title}</summary>
    <div className="compact-options-body">
      {subtitle && <p className="page-subtitle">{subtitle}</p>}
      {right}
      {children}
    </div>
  </details>
) : (
  <section className="cockpit-card" style={{ display: 'flex', flexDirection: 'column', gap: 12, padding: 16 }}>
    {(title || right) && (
      <header className="settings-card-header">
        <div className="settings-card-heading">
          <div style={{ fontSize: 14, fontWeight: 700, color: 'var(--text-main)', display: 'flex', alignItems: 'center', gap: 6 }}>
            {icon}
            <span>{title}</span>
          </div>
          {subtitle && <div style={{ fontSize: 12, color: 'var(--text-muted)', marginTop: 2 }}>{subtitle}</div>}
        </div>
        {right}
      </header>
    )}
    {children}
  </section>
);

export const SettingsPage: React.FC<SettingsPageProps> = ({ port }) => {
  const [tab, setTab] = useState<Tab>('sources');
  const [showKeys, setShowKeys] = useState<Record<string, boolean>>({});
  const [testStatus, setTestStatus] = useState<Record<string, { loading: boolean; success?: boolean; message?: string; latency?: number }>>({});
  const [saveSuccess, setSaveSuccess] = useState(false);
  const [saveError, setSaveError] = useState<string | null>(null);
  const [configLoaded, setConfigLoaded] = useState(false);
  const [loadingConfig, setLoadingConfig] = useState(true);
  const [saving, setSaving] = useState(false);
  const [loadAttempt, setLoadAttempt] = useState(0);
  const [clearCacheSuccess, setClearCacheSuccess] = useState(false);
  const [sourceSearch, setSourceSearch] = useState('');
  const [selectedSourceGroup, setSelectedSourceGroup] = useState<string>('all');
  const [expandedGroups, setExpandedGroups] = useState<Record<string, boolean>>({});
  const [health, setHealth] = useState<Record<string, SourceHealth>>({});
  const [checkProgress, setCheckProgress] = useState<{ done: number; total: number } | null>(null);
  const checkRun = React.useRef(0);

  const [config, setConfig] = useState({
    domain_preset: 'auto',
    enabled_sources: 'openalex,crossref,semantic_scholar,arxiv,zenodo,hal,pubmed,doaj,vietnam,metasearch',

    openai_api_key: '',
    openai_base_url: 'https://api.openai.com/v1',
    anthropic_api_key: '',
    gemini_api_key: '',
    deepseek_api_key: '',
    ollama_base_url: 'http://localhost:11434',
    ollama_model: 'qwen2.5:7b',
    groq_api_key: '',

    searxng_enabled: 'false',
    searxng_url: 'http://localhost:8080',
    searxng_categories: 'science',
    searxng_engines: 'google scholar, pubmed, arxiv, crossref',

    ncbi_api_key: '',
    ncbi_email: '',
    openalex_email: '',
    openalex_api_key: '',
    semantic_scholar_api_key: '',
    crossref_email: '',
    unpaywall_email: '',
    scopus_api_key: '',
    ieee_api_key: '',
    springer_api_key: '',
    perplexity_api_key: '',
    dimensions_api_key: '',
    wos_api_key: '',
    consensus_session: '',
    openevidence_session: '',
    openai_session: '',
    anthropic_session: '',
    gemini_session: '',
    deepseek_session: '',
    perplexity_session: '',

    web_search_enabled: 'false',
    web_search_url: '',
    mcp_auth_token: '',
    gateway_port: String(port),
    cache_ttl_hours: '24',
    max_results_default: '15',
    search_timeout_seconds: '12',
    rate_limit_per_minute: '0',
    download_directory: '',
  });

  useEffect(() => {
    const controller = new AbortController();
    setConfigLoaded(false);
    setLoadingConfig(true);
    const loadConfig = async () => {
      try {
        const data = isTauri() ? await invoke('read_settings') : await (async () => {
          const res = await gatewayFetch('/api/config', { signal: controller.signal });
          if (!res.ok) throw new Error(`HTTP ${res.status}`);
          return res.json();
        })();
        if (!data || typeof data !== 'object' || Array.isArray(data)) throw new Error('Invalid configuration format');
        if (controller.signal.aborted) return;
        setConfig((prev) => {
          const normalized = Object.fromEntries(
            Object.entries(data).map(([key, value]) => [key, value == null ? '' : String(value)])
          ) as Partial<typeof prev>;
          return { ...prev, ...normalized };
        });
        setConfigLoaded(true);
        setSaveError(null);
      } catch (e) {
        if (!controller.signal.aborted) setSaveError(`Unable to load gateway settings; saving is locked to prevent overriding with defaults: ${(e as Error).message}`);
      } finally {
        if (!controller.signal.aborted) setLoadingConfig(false);
      }
    };
    loadConfig();
    return () => controller.abort();
  }, [port, loadAttempt]);

  const toggleShowKey = (id: string) => setShowKeys((prev) => ({ ...prev, [id]: !prev[id] }));
  const handleInputChange = (field: string, value: string) => setConfig((prev) => ({ ...prev, [field]: value }));

  const handleSave = async () => {
    if (!configLoaded || saving) return;
    setSaveError(null);
    setSaveSuccess(false);
    setSaving(true);
    try {
      if (isTauri()) {
        await invoke('save_settings', { payload: config });
      } else {
        const res = await gatewayFetch('/api/config', {
          method: 'POST',
          headers: { 'Content-Type': 'application/json' },
          body: JSON.stringify(config),
        });
        const data = await res.json().catch(() => null);
        if (!res.ok || data?.success === false) throw new Error(data?.error || `Server returned status code ${res.status}`);
      }
      if (!isConfiguredSecret(config.mcp_auth_token)) {
        setGatewayToken(config.mcp_auth_token || null);
      }
      setSaveSuccess(true);
      setTimeout(() => setSaveSuccess(false), 2000);
    } catch (e) {
      setSaveSuccess(false);
      setSaveError(`Failed to save settings. Changes remain in form: ${(e as Error).message}`);
    } finally {
      setSaving(false);
    }
  };

  const testLlm = async (provider: string, apiKey: string, baseUrl?: string, model?: string) => {
    setTestStatus((prev) => ({ ...prev, [provider]: { loading: true } }));
    const freshKey = apiKey && apiKey !== KEEP_SENTINEL ? apiKey : '';
    try {
      const res = await gatewayFetch('/api/test-llm', {
        method: 'POST',
        headers: { 'Content-Type': 'application/json' },
        body: JSON.stringify({ provider, api_key: freshKey, base_url: baseUrl, model, use_saved: !freshKey }),
      });
      const data = await res.json().catch(() => null);
      setTestStatus((prev) => ({
        ...prev,
        [provider]: { loading: false, success: res.ok && data?.success === true, message: data?.message || `Server returned status code ${res.status}`, latency: data?.latency_ms },
      }));
    } catch (e: any) {
      setTestStatus((prev) => ({ ...prev, [provider]: { loading: false, success: false, message: `Error: ${e.message}` } }));
    }
  };

  const testSearxng = async () => {
    setTestStatus((prev) => ({ ...prev, searxng: { loading: true } }));
    try {
      const res = await gatewayFetch('/api/test-searxng', {
        method: 'POST',
        headers: { 'Content-Type': 'application/json' },
        body: JSON.stringify({ url: config.searxng_url, categories: config.searxng_categories, engines: config.searxng_engines }),
      });
      const data = await res.json().catch(() => null);
      setTestStatus((prev) => ({
        ...prev,
        searxng: { loading: false, success: res.ok && data?.success === true, message: data?.message || `Server returned status code ${res.status}`, latency: data?.latency_ms },
      }));
    } catch (e: any) {
      setTestStatus((prev) => ({ ...prev, searxng: { loading: false, success: false, message: `Error: ${e.message}` } }));
    }
  };

  /** Probe one source through the gateway and record exactly what came back. */
  const checkSource = async (id: string): Promise<SourceHealth> => {
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

  /** Check every active source, a few at a time so one slow site cannot stall
   *  the rest and the gateway is not hit with fifty parallel searches. */
  const checkActiveSources = async () => {
    const ids = SOURCES_LIST.filter((source) => isSourceActive(source.id)).map((source) => source.id);
    if (ids.length === 0) return;
    const run = ++checkRun.current;
    setCheckProgress({ done: 0, total: ids.length });
    const queue = [...ids];
    const worker = async () => {
      while (queue.length) {
        const id = queue.shift();
        if (!id || checkRun.current !== run) return;
        await checkSource(id);
        setCheckProgress((prev) => (prev ? { ...prev, done: prev.done + 1 } : prev));
      }
    };
    await Promise.all(Array.from({ length: Math.min(4, ids.length) }, worker));
    if (checkRun.current === run) setCheckProgress(null);
  };

  const stopChecking = () => {
    checkRun.current += 1;
    setCheckProgress(null);
  };

  /** Back to the source selection a fresh install starts with. */
  const restoreDefaultSources = () => {
    setConfig((prev) => ({
      ...prev,
      domain_preset: DEFAULT_PRESET,
      enabled_sources: DEFAULT_SOURCES.join(','),
    }));
  };

  const handleClearCache = async () => {
    if (!window.confirm('Clear all SQLite search cache?')) return;
    try {
      const res = await gatewayFetch('/api/cache/clear', { method: 'POST' });
      const data = await res.json().catch(() => null);
      if (!res.ok || data?.success !== true) throw new Error(data?.error || `HTTP ${res.status}`);
      setClearCacheSuccess(true);
      setTimeout(() => setClearCacheSuccess(false), 2000);
    } catch (e) {
      setSaveError(`Unable to clear cache: ${(e as Error).message}`);
    }
  };

  type LoginService = 'consensus' | 'openevidence' | 'openai' | 'anthropic' | 'gemini' | 'deepseek' | 'perplexity';

  const handleOpenLogin = async (service: LoginService) => {
    if (!isTauri()) {
      alert('Webview login is only available in the desktop application.');
      return;
    }
    try {
      await invoke('open_service_login', { service });
    } catch (e) {
      alert(`Unable to open login window: ${e}`);
    }
  };

  const handleClearSession = async (service: LoginService) => {
    if (isTauri()) {
      try {
        await invoke('clear_service_session', { service });
      } catch {}
    }
    handleInputChange(`${service}_session`, '');
  };

  const currentSources = (config.enabled_sources || '')
    .split(',')
    .map((s) => s.trim().toLowerCase())
    .map((s) => (s === 'vjol' ? 'vietnam' : s))
    .map((s) => (s === 'searxng' ? 'metasearch' : s))
    .filter(Boolean);

  const isSourceActive = (sourceId: string): boolean => {
    if (config.domain_preset === 'custom') return currentSources.includes(sourceId);
    return searchCatalog.presets.find((preset) => preset.id === config.domain_preset)?.sources.includes(sourceId) ?? false;
  };

  const toggleSource = (sourceId: string) => {
    let activeList = [...currentSources];
    if (config.domain_preset !== 'custom') {
      activeList = SOURCES_LIST.map((s) => s.id).filter((id) => isSourceActive(id));
    }
    const nextList = activeList.includes(sourceId) ? activeList.filter((s) => s !== sourceId) : [...activeList, sourceId];
    setConfig((prev) => ({ ...prev, domain_preset: 'custom', enabled_sources: nextList.join(',') }));
  };

  const handlePresetChange = (presetId: string) => {
    // Leaving a preset for Custom keeps the sources that were active a moment
    // ago; starting from an empty list would silently disable every source.
    const seeded =
      config.domain_preset === 'custom'
        ? currentSources
        : SOURCES_LIST.map((source) => source.id).filter((id) => isSourceActive(id));
    setConfig((prev) => ({
      ...prev,
      domain_preset: presetId,
      enabled_sources:
        presetId === 'custom'
          ? seeded.join(',')
          : searchCatalog.presets.find((preset) => preset.id === presetId)?.sources.join(',') ?? '',
    }));
  };

  const sourceGroups = ['all', ...Array.from(new Set(SOURCES_LIST.map((s) => s.group)))];

  const domainGroups = useMemo(() => {
    const groupsOrder = [
      'Multidisciplinary Indexes',
      'Biomedical & Clinical',
      'CS, AI & Engineering',
      'Vietnamese Journals',
      'AI & Syntheses',
      'Data Repositories & Open Access',
      'Economics & Social Sciences',
      'Physics & Natural Sciences',
      'Regional & Global South',
    ];

    const map = new Map<string, typeof SOURCES_LIST>();
    for (const src of SOURCES_LIST) {
      const list = map.get(src.group) || [];
      list.push(src);
      map.set(src.group, list);
    }

    const sortedKeys = [
      ...groupsOrder.filter((k) => map.has(k)),
      ...Array.from(map.keys()).filter((k) => !groupsOrder.includes(k)),
    ];

    return sortedKeys.map((name) => ({
      name,
      sources: map.get(name) || [],
    }));
  }, []);

  const getGroupIcon = (groupName: string) => {
    if (groupName.includes('Biomedical')) return <Stethoscope size={15} style={{ color: '#ef4444' }} />;
    if (groupName.includes('CS') || groupName.includes('Engineering')) return <Cpu size={15} style={{ color: '#3b82f6' }} />;
    if (groupName.includes('Vietnamese')) return <BookOpen size={15} style={{ color: '#eab308' }} />;
    if (groupName.includes('Syntheses')) return <Sparkles size={15} style={{ color: '#8b5cf6' }} />;
    if (groupName.includes('Data Repositories')) return <Database size={15} style={{ color: '#10b981' }} />;
    if (groupName.includes('Economics')) return <BarChart3 size={15} style={{ color: '#f97316' }} />;
    if (groupName.includes('Physics')) return <Radio size={15} style={{ color: '#06b6d4' }} />;
    return <Globe size={15} style={{ color: '#0ea5e9' }} />;
  };

  const toggleGroup = (groupName: string) => {
    const group = domainGroups.find((g) => g.name === groupName);
    if (!group) return;
    const groupSourceIds = group.sources.map((s) => s.id);
    let activeList = [...currentSources];
    if (config.domain_preset !== 'custom') {
      activeList = SOURCES_LIST.map((s) => s.id).filter((id) => isSourceActive(id));
    }
    const allActive = groupSourceIds.every((id) => activeList.includes(id));
    let nextList: string[];
    if (allActive) {
      const removeSet = new Set(groupSourceIds);
      nextList = activeList.filter((id) => !removeSet.has(id));
    } else {
      nextList = Array.from(new Set([...activeList, ...groupSourceIds]));
    }
    setConfig((prev) => ({ ...prev, domain_preset: 'custom', enabled_sources: nextList.join(',') }));
  };

  const toggleGroupExpand = (groupName: string) => {
    setExpandedGroups((prev) => ({ ...prev, [groupName]: !prev[groupName] }));
  };

  const expandAllGroups = () => {
    const all: Record<string, boolean> = {};
    domainGroups.forEach((g) => { all[g.name] = true; });
    setExpandedGroups(all);
  };

  const collapseAllGroups = () => {
    setExpandedGroups({});
  };

  const filteredGroups = useMemo(() => {
    const q = sourceSearch.trim().toLowerCase();
    return domainGroups
      .map((g) => {
        const matchesGroupFilter = selectedSourceGroup === 'all' || g.name === selectedSourceGroup;
        if (!matchesGroupFilter) return null;
        if (!q) return g;
        const matchingSources = g.sources.filter(
          (s) =>
            s.name.toLowerCase().includes(q) ||
            s.id.toLowerCase().includes(q) ||
            (s.desc && s.desc.toLowerCase().includes(q))
        );
        if (matchingSources.length === 0) return null;
        return {
          ...g,
          sources: matchingSources,
        };
      })
      .filter((g): g is NonNullable<typeof g> => g !== null);
  }, [domainGroups, sourceSearch, selectedSourceGroup]);

  const PRIMARY_PRESETS = [
    { id: 'auto', label: '⚡ Auto (Recommended)' },
    { id: 'all', label: '🌐 Comprehensive (All Sources)' },
    { id: 'vietnam_academic', label: '🇻🇳 Vietnamese Academic' },
    { id: 'biomedical', label: '🧬 International Biomedical' },
    { id: 'cs_ai', label: '🤖 Computer Science & AI' },
    { id: 'custom', label: '🛠️ Custom Selection' },
  ];

  const credentialReady = (key: string) => {
    const meta = CREDENTIAL_LABELS[key];
    const value = (config as Record<string, string>)[key] ?? '';
    return meta?.secret ? isConfiguredSecret(value) : value.trim().length > 0;
  };

  const renderSecretField = (key: string, placeholder?: string) => {
    const meta: { label: string; secret: boolean; type?: string; placeholder?: string } =
      CREDENTIAL_LABELS[key] ?? { label: key, secret: true };
    const value = (config as Record<string, string>)[key] ?? '';
    const isSecret = meta.secret !== false;
    return (
      <label key={key} style={{ display: 'flex', flexDirection: 'column', gap: 4 }}>
        <span className="field-label" style={{ margin: 0 }}>{meta.label}</span>
        <div style={{ position: 'relative', display: 'flex', alignItems: 'center' }}>
          <input
            type={isSecret && !showKeys[key] ? 'password' : 'text'}
            autoComplete="off"
            style={{ ...FIELD_STYLE, paddingRight: isSecret ? 32 : 10 }}
            placeholder={isConfiguredSecret(value) ? 'Saved — enter to overwrite' : (placeholder ?? meta.placeholder ?? '')}
            value={value}
            onChange={(e) => handleInputChange(key, e.target.value)}
          />
          {isSecret && (
            <button type="button" onClick={() => toggleShowKey(key)} style={{ position: 'absolute', right: 8, background: 'none', border: 'none', color: 'var(--text-dim)', cursor: 'pointer' }} aria-label="Toggle visibility">
              {showKeys[key] ? <EyeOff size={14} /> : <Eye size={14} />}
            </button>
          )}
        </div>
        {isConfiguredSecret(value) && <span style={{ fontSize: 10, color: 'var(--status-emerald)' }}>Saved in keychain</span>}
      </label>
    );
  };

  /** One line describing the last probe of a source, in the source's own words. */
  const healthLabel = (state: SourceHealth | undefined): { text: string; color: string } | null => {
    if (!state) return null;
    if (state.loading) return { text: 'Checking…', color: 'var(--text-muted)' };
    if (state.needsSetup) return { text: state.error || 'Needs credentials', color: 'var(--status-amber)' };
    if (state.ok) {
      const seconds = state.elapsedMs !== undefined ? ` · ${(state.elapsedMs / 1000).toFixed(1)}s` : '';
      return {
        text: state.count === 0 ? `Reachable, 0 results for “${state.query}”${seconds}` : `${state.count} results${seconds}`,
        color: 'var(--status-emerald)',
      };
    }
    return { text: state.error || 'Failed', color: 'var(--status-rose)' };
  };

  const renderHealth = (id: string) => {
    const label = healthLabel(health[id]);
    if (!label) return null;
    return (
      <span style={{ fontSize: 10, color: label.color, overflowWrap: 'anywhere' }} data-testid={`health-${id}`}>
        {label.text}
      </span>
    );
  };

  /** Small coloured dot the collapsed pills carry once a source has been probed. */
  const healthDot = (id: string) => {
    const state = health[id];
    if (!state) return null;
    const color = state.loading
      ? 'var(--text-dim)'
      : state.needsSetup
        ? 'var(--status-amber)'
        : state.ok
          ? 'var(--status-emerald)'
          : 'var(--status-rose)';
    return (
      <span
        aria-hidden="true"
        style={{ width: 6, height: 6, borderRadius: '50%', background: color, flexShrink: 0 }}
      />
    );
  };

  const renderTestStatus = (id: string) => {
    const status = testStatus[id];
    if (!status) return null;
    return (
      <div style={{ fontSize: 11, padding: '5px 8px', borderRadius: 4, background: status.loading ? '#f8fafc' : status.success ? 'var(--status-emerald-bg)' : 'var(--status-rose-bg)', color: status.success ? 'var(--status-emerald)' : 'var(--status-rose)' }}>
        {status.loading ? 'Testing…' : `${status.message ?? ''}${status.latency ? ` (${status.latency}ms)` : ''}`}
      </div>
    );
  };

  return (
    <div className="page-container" style={{ gap: 16 }}>
      <div style={{ display: 'flex', alignItems: 'center', justifyContent: 'space-between', borderBottom: '1px solid var(--cockpit-border)', paddingBottom: 12, gap: 10, flexWrap: 'wrap' }}>
        <div>
          <h2 style={{ fontSize: 18, fontWeight: 700, color: 'var(--text-main)', display: 'flex', alignItems: 'center', gap: 8 }}>
            <Settings size={18} style={{ color: 'var(--primary-cyan)' }} />
            <span>Settings</span>
          </h2>
          <p style={{ fontSize: 12, color: 'var(--text-muted)', marginTop: 2 }}>Search sources, API credentials, and local agent gateway</p>
        </div>
        <button id="save-settings" className="action-btn action-btn-primary" onClick={handleSave} disabled={!configLoaded || saving} style={{ padding: '6px 14px', fontSize: 12, fontWeight: 600 }}>
          {saveSuccess ? <><CheckCircle2 size={15} /><span>Saved!</span></> : <><Save size={15} /><span>{saving ? 'Saving…' : 'Save Settings'}</span></>}
        </button>
      </div>

      {saveError && <div className="alert alert-warning" role="alert"><div>{saveError}</div></div>}
      {!configLoaded && (
        <button id="settings-retry-load" className="action-btn" disabled={loadingConfig} onClick={() => setLoadAttempt((attempt) => attempt + 1)}>
          <RefreshCw size={14} /> {loadingConfig ? 'Loading settings…' : 'Retry loading settings'}
        </button>
      )}

      <div style={{ display: 'flex', gap: 6, flexWrap: 'wrap' }}>
        {([
          ['sources', 'Search Sources', <Layers size={14} />],
          ['connections', 'Connections & Keys', <Terminal size={14} />],
          ['clients', 'AI Clients', <Plug size={14} />],
          ['gateway', 'Gateway & Security', <Server size={14} />],
        ] as [Tab, string, React.ReactNode][]).map(([id, label, icon]) => (
          <button key={id} className={`action-btn ${tab === id ? 'action-btn-primary' : ''}`} onClick={() => setTab(id)} style={{ padding: '6px 14px' }}>
            {icon}
            <span>{label}</span>
          </button>
        ))}
      </div>

      {/* ---------------- Search Sources ---------------- */}
      {tab === 'sources' && (
        <div style={{ display: 'flex', flexDirection: 'column', gap: 14 }}>
          {/* Preset Selector Card */}
          <Card
            title="Domain Presets"
            subtitle="Quickly configure optimal search sources tailored for your research goal"
            icon={<Layers size={16} style={{ color: 'var(--primary-cyan)' }} />}
            right={
              <select
                aria-label="Select specialized preset"
                style={{ ...FIELD_STYLE, width: 260, maxWidth: '100%', padding: '4px 8px', fontSize: 11 }}
                value={config.domain_preset}
                onChange={(e) => handlePresetChange(e.target.value)}
              >
                <option value="custom">🛠️ Custom Selection</option>
                <optgroup label="Recommended Presets">
                  {PRIMARY_PRESETS.filter((p) => p.id !== 'custom').map((p) => (
                    <option key={p.id} value={p.id}>{p.label}</option>
                  ))}
                </optgroup>
                <optgroup label={`All ${PRESETS.filter((p) => !PRIMARY_PRESETS.some((prim) => prim.id === p.id)).length} specialised presets`}>
                  {PRESETS.filter((p) => !PRIMARY_PRESETS.some((prim) => prim.id === p.id)).map((p) => (
                    <option key={p.id} value={p.id}>{p.label}</option>
                  ))}
                </optgroup>
              </select>
            }
          >
            {/* Quick Chips for primary presets */}
            <div style={{ display: 'flex', gap: 6, flexWrap: 'wrap', alignItems: 'center' }}>
              {PRIMARY_PRESETS.map((p) => {
                const selected = config.domain_preset === p.id;
                return (
                  <button
                    key={p.id}
                    id={`preset-${p.id}`}
                    type="button"
                    onClick={() => handlePresetChange(p.id)}
                    className="quick-pill"
                    style={{
                      padding: '5px 12px',
                      fontSize: 11.5,
                      fontWeight: selected ? 700 : 500,
                      background: selected ? 'var(--primary-cyan-bg)' : '#ffffff',
                      color: selected ? 'var(--primary-cyan)' : 'var(--text-main)',
                      borderColor: selected ? 'var(--primary-cyan)' : 'var(--cockpit-border)',
                      borderRadius: 16,
                    }}
                  >
                    {p.label}
                  </button>
                );
              })}
            </div>

            {/* Active Preset Summary info */}
            <div style={{ fontSize: 11.5, color: 'var(--text-dim)', background: '#f8fafc', padding: '6px 10px', borderRadius: 'var(--radius-sm)', border: '1px solid #e2e8f0', display: 'flex', alignItems: 'center', justifyContent: 'space-between', flexWrap: 'wrap', gap: 6 }}>
              <div>
                <strong style={{ color: 'var(--text-main)' }}>
                  {config.domain_preset === 'custom'
                    ? 'Custom Selection Mode'
                    : (PRESETS.find((p) => p.id === config.domain_preset)?.label || config.domain_preset)}
                </strong>
                <span>: </span>
                <span>
                  {config.domain_preset === 'custom'
                    ? `${currentSources.length}/${SOURCES_LIST.length} sources manually selected below.`
                    : (PRESETS.find((p) => p.id === config.domain_preset)?.description || 'Auto-selects compatible sources.')}
                </span>
              </div>
              <span style={{ display: 'flex', alignItems: 'center', gap: 8 }}>
                <span style={{ fontWeight: 600, color: 'var(--primary-cyan)' }}>
                  {SOURCES_LIST.filter((s) => isSourceActive(s.id)).length} sources active
                </span>
                <button
                  id="restore-default-sources"
                  type="button"
                  className="action-btn"
                  onClick={restoreDefaultSources}
                  title={`Back to the default selection: ${DEFAULT_SOURCES.join(', ')}`}
                  style={{ padding: '2px 8px', fontSize: 10 }}
                >
                  <RotateCcw size={11} />
                  <span>Restore defaults</span>
                </button>
              </span>
            </div>
          </Card>

          {/* Grouped Sources Card */}
          <Card
            title="Search Sources by Domain"
            subtitle={`${SOURCES_LIST.filter((s) => isSourceActive(s.id)).length}/${SOURCES_LIST.length} active sources • ${domainGroups.length} domain categories`}
            icon={<Globe size={16} style={{ color: 'var(--primary-cyan)' }} />}
            right={
              <div style={{ display: 'flex', gap: 6, flexWrap: 'wrap', alignItems: 'center' }}>
                <div style={{ position: 'relative', display: 'flex', alignItems: 'center' }}>
                  <Search size={12} style={{ position: 'absolute', left: 7, color: 'var(--text-dim)' }} />
                  <input
                    type="text"
                    placeholder={`Filter ${SOURCES_LIST.length} sources...`}
                    value={sourceSearch}
                    onChange={(e) => setSourceSearch(e.target.value)}
                    style={{ ...FIELD_STYLE, width: 140, padding: '4px 8px 4px 24px', fontSize: 11 }}
                  />
                </div>
                <button type="button" className="action-btn" onClick={() => setConfig((prev) => ({ ...prev, domain_preset: 'custom', enabled_sources: SOURCES_LIST.map((s) => s.id).join(',') }))} style={{ padding: '3px 8px', fontSize: 10 }}>Enable All</button>
                <button type="button" className="action-btn" onClick={() => setConfig((prev) => ({ ...prev, domain_preset: 'custom', enabled_sources: '' }))} style={{ padding: '3px 8px', fontSize: 10 }}>Disable All</button>
                <button
                  id="check-active-sources"
                  type="button"
                  className="action-btn"
                  onClick={() => (checkProgress ? stopChecking() : void checkActiveSources())}
                  style={{ padding: '3px 8px', fontSize: 10 }}
                  title="Send one real query to every active source and report what each one answers"
                >
                  <Activity size={11} className={checkProgress ? 'animate-spin' : ''} />
                  <span>
                    {checkProgress ? `Stop (${checkProgress.done}/${checkProgress.total})` : 'Check active sources'}
                  </span>
                </button>
                <button type="button" className="action-btn" onClick={expandAllGroups} style={{ padding: '3px 8px', fontSize: 10 }}>Expand All</button>
                <button type="button" className="action-btn" onClick={collapseAllGroups} style={{ padding: '3px 8px', fontSize: 10 }}>Collapse All</button>
              </div>
            }
          >
            {/* Category pills for instant group filtering */}
            <div style={{ display: 'flex', gap: 6, flexWrap: 'wrap', marginBottom: 2 }}>
              {sourceGroups.map((grp) => {
                const isSelected = selectedSourceGroup === grp;
                const count = grp === 'all' ? SOURCES_LIST.length : SOURCES_LIST.filter((s) => s.group === grp).length;
                return (
                  <button
                    key={grp}
                    type="button"
                    onClick={() => setSelectedSourceGroup(grp)}
                    className="action-btn"
                    style={{
                      padding: '2px 9px',
                      fontSize: 10.5,
                      fontWeight: isSelected ? 700 : 500,
                      background: isSelected ? 'var(--primary-cyan-bg)' : '#ffffff',
                      color: isSelected ? 'var(--primary-cyan)' : 'var(--text-main)',
                      borderColor: isSelected ? 'var(--primary-cyan)' : 'var(--cockpit-border)',
                      borderRadius: 12,
                    }}
                  >
                    {grp === 'all' ? 'All' : getGroupLabel(grp)} ({count})
                  </button>
                );
              })}
            </div>

            {/* List of Domain Groups */}
            <div style={{ display: 'flex', flexDirection: 'column', gap: 10 }}>
              {filteredGroups.map((group) => {
                const groupActiveCount = group.sources.filter((s) => isSourceActive(s.id)).length;
                const totalCount = group.sources.length;
                const allActive = groupActiveCount === totalCount && totalCount > 0;
                const hasActive = groupActiveCount > 0;
                const isExpanded = Boolean(sourceSearch.trim()) || Boolean(expandedGroups[group.name]);

                return (
                  <div
                    key={group.name}
                    className={`settings-domain-card ${allActive ? 'all-active' : ''}`}
                  >
                    {/* Domain Group Header */}
                    <div style={{ display: 'flex', alignItems: 'center', justifyContent: 'space-between', gap: 10, flexWrap: 'wrap' }}>
                      <div style={{ display: 'flex', alignItems: 'center', gap: 8 }}>
                        {getGroupIcon(group.name)}
                        <span style={{ fontSize: 13, fontWeight: 700, color: 'var(--text-main)' }}>{getGroupLabel(group.name)}</span>
                        <span
                          className={`cockpit-badge ${allActive ? 'badge-emerald' : hasActive ? 'badge-cyan' : 'badge-vjol'}`}
                          style={{ fontSize: 9.5 }}
                        >
                          {groupActiveCount}/{totalCount} sources
                        </span>
                      </div>

                      <div style={{ display: 'flex', alignItems: 'center', gap: 6 }}>
                        <button
                          type="button"
                          className="action-btn"
                          onClick={() => toggleGroup(group.name)}
                          style={{ padding: '3px 8px', fontSize: 10.5, fontWeight: 600 }}
                        >
                          {allActive ? 'Disable Group' : 'Enable Group'}
                        </button>
                        <button
                          type="button"
                          className="action-btn"
                          onClick={() => toggleGroupExpand(group.name)}
                          style={{ padding: '3px 8px', fontSize: 10.5, display: 'flex', alignItems: 'center', gap: 4 }}
                        >
                          <span>{isExpanded ? 'Collapse' : `Details (${totalCount})`}</span>
                          {isExpanded ? <ChevronUp size={12} /> : <ChevronDown size={12} />}
                        </button>
                      </div>
                    </div>

                    {/* Collapsed view: Fast interactive pills */}
                    {!isExpanded && (
                      <div style={{ display: 'flex', flexWrap: 'wrap', gap: 5, alignItems: 'center' }}>
                        {group.sources.map((s) => {
                          const active = isSourceActive(s.id);
                          return (
                            <span
                              key={s.id}
                              className={`settings-source-pill ${active ? 'active' : 'inactive'}`}
                              onClick={() => toggleSource(s.id)}
                              title={
                                healthLabel(health[s.id])
                                  ? `${s.name} — ${healthLabel(health[s.id])!.text}`
                                  : active
                                    ? `${s.name} (Active — click to disable)`
                                    : `${s.name} (Inactive — click to enable)`
                              }
                            >
                              {active && <Check size={10} style={{ strokeWidth: 3 }} />}
                              <span>{s.name}</span>
                              {healthDot(s.id)}
                            </span>
                          );
                        })}
                      </div>
                    )}

                    {/* Expanded view: Detailed cards with credentials */}
                    {isExpanded && (
                      <div style={{ display: 'grid', gridTemplateColumns: 'repeat(auto-fit, minmax(240px, 1fr))', gap: 8, paddingTop: 6, borderTop: '1px solid #f1f5f9' }}>
                        {group.sources.map((src) => {
                          const enabled = isSourceActive(src.id);
                          const creds = src.credentials as string[];
                          const ready = creds.every((key) => credentialReady(key));
                          return (
                            <div
                              key={src.id}
                              role="button"
                              tabIndex={0}
                              onClick={() => toggleSource(src.id)}
                              onKeyDown={(e) => {
                                if (e.key === 'Enter' || e.key === ' ') {
                                  e.preventDefault();
                                  toggleSource(src.id);
                                }
                              }}
                              style={{
                                display: 'flex',
                                flexDirection: 'column',
                                gap: 4,
                                padding: '8px 10px',
                                background: enabled ? '#f0fdf4' : '#fafafa',
                                border: `1px solid ${enabled ? '#86efac' : 'var(--cockpit-border)'}`,
                                borderRadius: 'var(--radius-sm)',
                                cursor: 'pointer',
                                transition: 'all 0.15s ease',
                              }}
                            >
                              <div style={{ display: 'flex', alignItems: 'center', justifyContent: 'space-between', gap: 6 }}>
                                <span style={{ fontSize: 12, fontWeight: 700, color: 'var(--text-main)' }}>{src.name}</span>
                                <span className={`cockpit-badge ${enabled ? 'badge-emerald' : 'badge-vjol'}`} style={{ fontSize: 9 }}>
                                  {enabled ? 'ENABLED' : 'DISABLED'}
                                </span>
                              </div>
                              {src.desc && (
                                <span style={{ fontSize: 10.5, color: 'var(--text-muted)', lineHeight: 1.3 }}>
                                  {src.desc}
                                </span>
                              )}
                              {creds.length > 0 && (
                                <span style={{ fontSize: 9.5, color: ready ? 'var(--status-emerald)' : 'var(--status-amber)', marginTop: 2 }}>
                                  {ready ? '✓ credentials ready' : `requires: ${creds.map((k) => CREDENTIAL_LABELS[k]?.label ?? k).join(', ')}`}
                                </span>
                              )}
                              <div
                                style={{ display: 'flex', alignItems: 'center', gap: 6, marginTop: 4, flexWrap: 'wrap' }}
                                onClick={(e) => e.stopPropagation()}
                              >
                                <button
                                  id={`check-source-${src.id}`}
                                  type="button"
                                  className="action-btn"
                                  style={{ padding: '2px 8px', fontSize: 10 }}
                                  disabled={health[src.id]?.loading}
                                  onClick={() => void checkSource(src.id)}
                                  title={`Query ${src.name} once and report the result`}
                                >
                                  <Activity size={10} className={health[src.id]?.loading ? 'animate-spin' : ''} />
                                  <span>{health[src.id]?.loading ? 'Checking…' : 'Check'}</span>
                                </button>
                                {renderHealth(src.id)}
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
      )}

      {/* ---------------- Connections & Keys ---------------- */}
      {tab === 'connections' && (
        <div style={{ display: 'flex', flexDirection: 'column', gap: 14 }}>
          <Card advanced title="AI Model API Keys (LLM)" subtitle="Used for testing connectivity; evidence synthesis operates deterministically without external transmissions" icon={<Cpu size={16} style={{ color: 'var(--primary-cyan)' }} />}>
            <div style={{ display: 'grid', gridTemplateColumns: 'repeat(auto-fit, minmax(280px, 1fr))', gap: 12 }}>
              {([
                { id: 'gemini', name: 'Google Gemini', key: 'gemini_api_key', url: 'https://aistudio.google.com/apikey', icon: <Sparkles size={15} style={{ color: 'var(--primary-cyan)' }} /> },
                { id: 'perplexity', name: 'Perplexity Sonar', key: 'perplexity_api_key', url: 'https://www.perplexity.ai/settings/api', icon: <Sparkles size={15} style={{ color: 'var(--primary-cyan)' }} /> },
                { id: 'deepseek', name: 'DeepSeek', key: 'deepseek_api_key', url: 'https://platform.deepseek.com/api_keys', icon: <Cpu size={15} style={{ color: 'var(--primary-cyan)' }} /> },
                { id: 'openai', name: 'OpenAI / Compatible', key: 'openai_api_key', icon: <Zap size={15} style={{ color: 'var(--status-emerald)' }} /> },
                { id: 'anthropic', name: 'Anthropic Claude', key: 'anthropic_api_key', url: 'https://console.anthropic.com/settings/keys', icon: <Terminal size={15} style={{ color: 'var(--status-amber)' }} /> },
              ] as { id: string; name: string; key: string; url?: string; icon: React.ReactNode }[]).map((provider) => {
                const signedIn = isConfiguredSecret((config as Record<string, string>)[`${provider.id}_session`]);
                return (
                <div key={provider.id} style={{ display: 'flex', flexDirection: 'column', gap: 8, padding: 12, border: '1px solid var(--cockpit-border)', borderRadius: 'var(--radius-sm)' }}>
                  <div style={{ display: 'flex', alignItems: 'center', justifyContent: 'space-between' }}>
                    <span style={{ fontSize: 13, fontWeight: 600, display: 'flex', alignItems: 'center', gap: 6 }}>{provider.icon}{provider.name}</span>
                    {provider.url && (
                      <a href={provider.url} target="_blank" rel="noreferrer" style={{ fontSize: 11, color: 'var(--primary-cyan)', textDecoration: 'none', display: 'flex', alignItems: 'center', gap: 4 }}>Get Key <ExternalLink size={11} /></a>
                    )}
                  </div>
                  <div style={{ display: 'flex', alignItems: 'center', gap: 6, flexWrap: 'wrap' }}>
                    <button
                      id={`signin-${provider.id}`}
                      type="button"
                      className="action-btn"
                      style={{ padding: '4px 10px', fontSize: 11 }}
                      onClick={() => handleOpenLogin(provider.id as LoginService)}
                      title={`Open ${provider.name}'s developer console in an app window and sign in with your account`}
                    >
                      <LogIn size={12} />
                      <span>{signedIn ? 'Sign in again' : 'Sign in'}</span>
                    </button>
                    <span style={{ fontSize: 10.5, color: signedIn ? 'var(--status-emerald)' : 'var(--text-dim)' }}>
                      {signedIn ? '● Console session saved' : '○ Not signed in'}
                    </span>
                    {signedIn && (
                      <button
                        type="button"
                        className="action-btn"
                        style={{ padding: '2px 8px', fontSize: 10, color: 'var(--status-rose)' }}
                        onClick={() => handleClearSession(provider.id as LoginService)}
                      >
                        Clear
                      </button>
                    )}
                  </div>
                  {renderSecretField(provider.key, 'sk-...')}
                  {provider.id === 'openai' && (
                    <label style={{ display: 'flex', flexDirection: 'column', gap: 4 }}>
                      <span className="field-label" style={{ margin: 0 }}>Base URL</span>
                      <input type="text" style={FIELD_STYLE} value={config.openai_base_url} onChange={(e) => handleInputChange('openai_base_url', e.target.value)} placeholder="https://api.openai.com/v1" />
                    </label>
                  )}
                  <div style={{ display: 'flex', alignItems: 'center', gap: 8 }}>
                    <button className="action-btn" onClick={() => testLlm(provider.id, (config as Record<string, string>)[provider.key], provider.id === 'openai' ? config.openai_base_url : undefined)} disabled={(config as Record<string, string>)[provider.key] === '' || testStatus[provider.id]?.loading} style={{ padding: '4px 10px', fontSize: 11 }}>
                      {testStatus[provider.id]?.loading ? <RefreshCw size={12} className="animate-spin" /> : <Zap size={12} />}
                      <span>Test</span>
                    </button>
                    <div style={{ flex: 1 }}>{renderTestStatus(provider.id)}</div>
                  </div>
                </div>
                );
              })}
            </div>

            <p style={{ fontSize: 11, color: 'var(--text-muted)', margin: 0, display: 'flex', gap: 6, alignItems: 'flex-start' }}>
              <AlertTriangle size={12} style={{ flexShrink: 0, marginTop: 1, color: 'var(--status-amber)' }} />
              <span>
                Sign in opens the provider's own developer console in an app window, so the account login
                (Google, GitHub, email) happens on the provider's page and the session stays in this app.
                A console session is not an API credential: calls to these providers still use the API key
                saved above. Consensus and OpenEvidence are different — their sessions are what their
                search sources authenticate with.
              </span>
            </p>

            <div style={{ display: 'grid', gridTemplateColumns: '1.5fr 1fr auto', gap: 8, alignItems: 'flex-end', padding: 12, border: '1px solid var(--cockpit-border)', borderRadius: 'var(--radius-sm)' }}>
              <label style={{ display: 'flex', flexDirection: 'column', gap: 4 }}>
                <span className="field-label" style={{ margin: 0 }}>Ollama Base URL (Local / Offline)</span>
                <input type="text" style={FIELD_STYLE} value={config.ollama_base_url} onChange={(e) => handleInputChange('ollama_base_url', e.target.value)} placeholder="http://localhost:11434" />
              </label>
              <label style={{ display: 'flex', flexDirection: 'column', gap: 4 }}>
                <span className="field-label" style={{ margin: 0 }}>Model</span>
                <input type="text" style={FIELD_STYLE} value={config.ollama_model} onChange={(e) => handleInputChange('ollama_model', e.target.value)} placeholder="qwen2.5:7b" />
              </label>
              <button className="action-btn" onClick={() => testLlm('ollama', 'local', config.ollama_base_url)} disabled={testStatus['ollama']?.loading} style={{ padding: '6px 12px', fontSize: 11, height: 32 }}>
                {testStatus['ollama']?.loading ? <RefreshCw size={12} className="animate-spin" /> : <Zap size={12} />}
                <span>Test Port</span>
              </button>
              <div style={{ gridColumn: '1 / -1' }}>{renderTestStatus('ollama')}</div>
            </div>
          </Card>

          <Card
            advanced title="Web Sessions (Consensus.app & OpenEvidence)"
            subtitle="Sign in directly via an in-app Webview window or paste session token for independent desktop access"
            icon={<ShieldCheck size={16} style={{ color: 'var(--primary-cyan)' }} />}
          >
            <div style={{ display: 'grid', gridTemplateColumns: 'repeat(auto-fit, minmax(320px, 1fr))', gap: 14 }}>
              {/* Consensus Card */}
              <div style={{ display: 'flex', flexDirection: 'column', gap: 10, padding: 14, border: '1px solid var(--cockpit-border)', borderRadius: 'var(--radius-sm)', background: 'var(--card-bg, #fff)' }}>
                <div style={{ display: 'flex', alignItems: 'center', justifyContent: 'space-between' }}>
                  <span style={{ fontSize: 13, fontWeight: 700, display: 'flex', alignItems: 'center', gap: 6 }}>
                    <Sparkles size={15} style={{ color: 'var(--primary-cyan)' }} />
                    Consensus.app
                  </span>
                  {isConfiguredSecret(config.consensus_session) ? (
                    <span style={{ fontSize: 11, padding: '2px 8px', borderRadius: 10, background: 'rgba(16, 185, 129, 0.15)', color: 'var(--status-emerald)', fontWeight: 600 }}>
                      ● Session saved
                    </span>
                  ) : (
                    <span style={{ fontSize: 11, padding: '2px 8px', borderRadius: 10, background: 'rgba(148, 163, 184, 0.15)', color: 'var(--text-dim)', fontWeight: 500 }}>
                      ○ Not connected
                    </span>
                  )}
                </div>
                <div style={{ fontSize: 11, color: 'var(--text-dim)', lineHeight: 1.4 }}>
                  Scientific evidence search, study design classification (RCT, Meta-analysis, Cohort), and verbatim claim extraction.
                </div>
                <div style={{ display: 'flex', gap: 8, flexWrap: 'wrap' }}>
                  <button
                    type="button"
                    className="action-btn"
                    onClick={() => handleOpenLogin('consensus')}
                    style={{ padding: '6px 12px', fontSize: 11, fontWeight: 600, display: 'flex', alignItems: 'center', gap: 6 }}
                  >
                    <ExternalLink size={12} />
                    <span>{isConfiguredSecret(config.consensus_session) ? 'Re-login via Webview' : 'Sign in to Consensus via Webview'}</span>
                  </button>
                  {isConfiguredSecret(config.consensus_session) && (
                    <button
                      type="button"
                      className="action-btn"
                      onClick={() => handleClearSession('consensus')}
                      style={{ padding: '6px 10px', fontSize: 11, color: 'var(--status-rose, #f43f5e)' }}
                    >
                      Clear Session
                    </button>
                  )}
                </div>
                <details style={{ marginTop: 4 }}>
                  <summary style={{ fontSize: 11, color: 'var(--text-dim)', cursor: 'pointer' }}>Paste Cookie / Token manually</summary>
                  <div style={{ marginTop: 6 }}>
                    {renderSecretField('consensus_session', 'Paste __session=... or Bearer JWT')}
                  </div>
                </details>
              </div>

              {/* OpenEvidence Card */}
              <div style={{ display: 'flex', flexDirection: 'column', gap: 10, padding: 14, border: '1px solid var(--cockpit-border)', borderRadius: 'var(--radius-sm)', background: 'var(--card-bg, #fff)' }}>
                <div style={{ display: 'flex', alignItems: 'center', justifyContent: 'space-between' }}>
                  <span style={{ fontSize: 13, fontWeight: 700, display: 'flex', alignItems: 'center', gap: 6 }}>
                    <ShieldCheck size={15} style={{ color: 'var(--primary-cyan)' }} />
                    OpenEvidence
                  </span>
                  {isConfiguredSecret(config.openevidence_session) ? (
                    <span style={{ fontSize: 11, padding: '2px 8px', borderRadius: 10, background: 'rgba(16, 185, 129, 0.15)', color: 'var(--status-emerald)', fontWeight: 600 }}>
                      ● Session saved
                    </span>
                  ) : (
                    <span style={{ fontSize: 11, padding: '2px 8px', borderRadius: 10, background: 'rgba(148, 163, 184, 0.15)', color: 'var(--text-dim)', fontWeight: 500 }}>
                      ○ Not connected
                    </span>
                  )}
                </div>
                <div style={{ fontSize: 11, color: 'var(--text-dim)', lineHeight: 1.4 }}>
                  AI clinical decision assistant synthesizing peer-reviewed citations and clinical guidelines.
                </div>
                <div style={{ display: 'flex', gap: 8, flexWrap: 'wrap' }}>
                  <button
                    type="button"
                    className="action-btn"
                    onClick={() => handleOpenLogin('openevidence')}
                    style={{ padding: '6px 12px', fontSize: 11, fontWeight: 600, display: 'flex', alignItems: 'center', gap: 6 }}
                  >
                    <ExternalLink size={12} />
                    <span>{isConfiguredSecret(config.openevidence_session) ? 'Re-login via Webview' : 'Sign in to OpenEvidence via Webview'}</span>
                  </button>
                  {isConfiguredSecret(config.openevidence_session) && (
                    <button
                      type="button"
                      className="action-btn"
                      onClick={() => handleClearSession('openevidence')}
                      style={{ padding: '6px 10px', fontSize: 11, color: 'var(--status-rose, #f43f5e)' }}
                    >
                      Clear Session
                    </button>
                  )}
                </div>
                <details style={{ marginTop: 4 }}>
                  <summary style={{ fontSize: 11, color: 'var(--text-dim)', cursor: 'pointer' }}>Paste Cookie / Token manually</summary>
                  <div style={{ marginTop: 6 }}>
                    {renderSecretField('openevidence_session', 'Paste cookie or session token')}
                  </div>
                </details>
              </div>
            </div>
          </Card>

          <Card title="Academic Source Credentials" subtitle="Emails are used for API polite pools; API keys increase rate limits. Stored securely in OS keychain." icon={<Database size={16} style={{ color: 'var(--primary-cyan)' }} />}>
            <div style={{ display: 'grid', gridTemplateColumns: 'repeat(auto-fit, minmax(260px, 1fr))', gap: 12 }}>
              {SOURCES_LIST.filter((source) => (source.credentials as string[]).length > 0).map((source) => (
                <div key={source.id} style={{ display: 'flex', flexDirection: 'column', gap: 8, padding: 12, border: '1px solid var(--cockpit-border)', borderRadius: 'var(--radius-sm)' }}>
                  <span style={{ fontSize: 13, fontWeight: 600 }}>{source.name}</span>
                  {(source.credentials as string[]).map((key) => renderSecretField(key))}
                </div>
              ))}
            </div>
          </Card>

          <Card
            advanced title="MetaSearch & Web Discovery (SearXNG)"
            subtitle="Native MetaSearch (Europe PMC) runs locally; SearXNG connector is optional for open web queries"
            icon={<Search size={16} style={{ color: 'var(--primary-cyan)' }} />}
            right={
              <label style={{ display: 'flex', alignItems: 'center', gap: 6, cursor: 'pointer' }}>
                <input type="checkbox" checked={config.searxng_enabled === 'true'} onChange={(e) => handleInputChange('searxng_enabled', e.target.checked ? 'true' : 'false')} style={{ width: 16, height: 16, accentColor: 'var(--primary-cyan)' }} />
                <span style={{ fontSize: 12, fontWeight: 600 }}>Use external SearXNG for metasearch</span>
              </label>
            }
          >
            <div style={{ display: 'grid', gridTemplateColumns: 'repeat(auto-fit, minmax(240px, 1fr))', gap: 10 }}>
              <label style={{ display: 'flex', flexDirection: 'column', gap: 4 }}>
                <span className="field-label" style={{ margin: 0 }}>SearXNG Base URL (Academic)</span>
                <input type="text" style={FIELD_STYLE} value={config.searxng_url} onChange={(e) => handleInputChange('searxng_url', e.target.value)} placeholder="http://localhost:8080" />
              </label>
              <label style={{ display: 'flex', flexDirection: 'column', gap: 4 }}>
                <span className="field-label" style={{ margin: 0 }}>Category</span>
                <select style={FIELD_STYLE} value={config.searxng_categories} onChange={(e) => handleInputChange('searxng_categories', e.target.value)}>
                  <option value="science">science (Academic & Medical)</option>
                  <option value="general">general (Entire Web)</option>
                </select>
              </label>
              <label style={{ display: 'flex', flexDirection: 'column', gap: 4 }}>
                <span className="field-label" style={{ margin: 0 }}>Engines</span>
                <input type="text" style={FIELD_STYLE} value={config.searxng_engines} onChange={(e) => handleInputChange('searxng_engines', e.target.value)} placeholder="google scholar, pubmed, arxiv" />
              </label>
            </div>
            <div style={{ display: 'flex', alignItems: 'center', gap: 10, borderTop: '1px solid var(--cockpit-border)', paddingTop: 10 }}>
              <button className="action-btn action-btn-primary" onClick={testSearxng} disabled={!config.searxng_url || testStatus['searxng']?.loading} style={{ padding: '6px 14px', fontSize: 12 }}>
                {testStatus['searxng']?.loading ? <RefreshCw size={13} className="animate-spin" /> : <Zap size={13} />}
                <span>Test Connection</span>
              </button>
              <div style={{ flex: 1 }}>{renderTestStatus('searxng')}</div>
            </div>
          </Card>
        </div>
      )}

      {/* ---------------- AI Clients ---------------- */}
      {tab === 'clients' && (
        <Card
          title="AI Clients on this machine"
          subtitle="Install the ScholarGateway MCP server and bundled skills into Claude, Codex and Antigravity"
          icon={<Plug size={16} style={{ color: 'var(--primary-cyan)' }} />}
        >
          <AiClients />
        </Card>
      )}

      {/* ---------------- Gateway & Security ---------------- */}
      {tab === 'gateway' && (
        <div style={{ display: 'flex', flexDirection: 'column', gap: 14 }}>
          <Card advanced title="Local Gateway Server" subtitle={`Serving at 127.0.0.1:${port} (REST + MCP)`} icon={<Server size={16} style={{ color: 'var(--primary-cyan)' }} />}>
            <div style={{ display: 'grid', gridTemplateColumns: 'repeat(auto-fit, minmax(200px, 1fr))', gap: 12 }}>
              <label style={{ display: 'flex', flexDirection: 'column', gap: 4 }}>
                <span className="field-label" style={{ margin: 0 }}>Port (effective upon app restart)</span>
                <input type="number" min="1024" max="65535" style={FIELD_STYLE} value={config.gateway_port} onChange={(e) => handleInputChange('gateway_port', e.target.value)} />
              </label>
              <label style={{ display: 'flex', flexDirection: 'column', gap: 4 }}>
                <span className="field-label" style={{ margin: 0 }}>Source Request Timeout (seconds)</span>
                <input type="number" min="1" max="120" style={FIELD_STYLE} value={config.search_timeout_seconds} onChange={(e) => handleInputChange('search_timeout_seconds', e.target.value)} />
              </label>
              <label style={{ display: 'flex', flexDirection: 'column', gap: 4 }}>
                <span className="field-label" style={{ margin: 0 }}>Default Max Results (1–50)</span>
                <input type="number" min="1" max="50" style={FIELD_STYLE} value={config.max_results_default} onChange={(e) => handleInputChange('max_results_default', e.target.value)} />
              </label>
              <label style={{ display: 'flex', flexDirection: 'column', gap: 4 }}>
                <span className="field-label" style={{ margin: 0 }}>Cache TTL (hours, 1–720)</span>
                <input type="number" min="1" max="720" style={FIELD_STYLE} value={config.cache_ttl_hours} onChange={(e) => handleInputChange('cache_ttl_hours', e.target.value)} />
              </label>
              <label style={{ display: 'flex', flexDirection: 'column', gap: 4 }}>
                <span className="field-label" style={{ margin: 0 }}>Rate Limit / min per Agent (0 = disabled)</span>
                <input type="number" min="0" max="100000" style={FIELD_STYLE} value={config.rate_limit_per_minute} onChange={(e) => handleInputChange('rate_limit_per_minute', e.target.value)} />
              </label>
              <label style={{ display: 'flex', flexDirection: 'column', gap: 4 }}>
                <span className="field-label" style={{ margin: 0 }}>PDF Download Directory</span>
                <input type="text" style={FIELD_STYLE} value={config.download_directory} onChange={(e) => handleInputChange('download_directory', e.target.value)} placeholder="Default: Documents/ScholarGateway/Papers" />
              </label>
            </div>
            <p style={{ fontSize: 11, color: 'var(--text-muted)', margin: 0 }}>Timeout applies per source request. Changing the port requires saving, quitting completely, and reopening the app; avoid port 1420.</p>
          </Card>

          <Card title="Security & Authentication" subtitle="Token protects all endpoints; write-only secret stored in OS keychain" icon={<Server size={16} style={{ color: 'var(--status-rose)' }} />}>
            {renderSecretField('mcp_auth_token', 'Leave blank for backward compatibility')}
            <p style={{ fontSize: 11, color: 'var(--text-muted)', margin: 0 }}>
              When a token is set, all routes (<code>/api/*</code>, <code>/mcp</code>, <code>/sse</code>, <code>/messages</code>) require <code>Authorization: Bearer …</code>; only <code>/health</code> and filtered <code>GET /api/config</code> remain public.
            </p>
          </Card>

          <Card advanced title="Web Search" subtitle="Dedicated SearXNG connector for Web Search tab; independent of academic literature sources" icon={<Globe size={16} style={{ color: 'var(--primary-cyan)' }} />}>
            <label style={{ display: 'flex', alignItems: 'center', gap: 6, cursor: 'pointer' }}>
              <input type="checkbox" checked={config.web_search_enabled === 'true'} onChange={(e) => handleInputChange('web_search_enabled', String(e.target.checked))} style={{ width: 16, height: 16, accentColor: 'var(--primary-cyan)' }} />
              <span style={{ fontSize: 12, fontWeight: 600 }}>Enable web search via SearXNG</span>
            </label>
            <label style={{ display: 'flex', flexDirection: 'column', gap: 4 }}>
              <span className="field-label" style={{ margin: 0 }}>Base URL for web search</span>
              <input type="url" style={FIELD_STYLE} value={config.web_search_url} onChange={(e) => handleInputChange('web_search_url', e.target.value)} placeholder="http://localhost:8080" />
            </label>
          </Card>

          <Card advanced title="System Cache" subtitle="Clear local SQLite search cache to fetch fresh records directly from source APIs" icon={<Database size={16} style={{ color: 'var(--text-muted)' }} />}>
            <button className="action-btn" onClick={handleClearCache} style={{ color: 'var(--status-rose)', borderColor: 'var(--status-rose-border)', padding: '6px 12px', alignSelf: 'flex-start' }}>
              <Trash2 size={14} />
              <span>{clearCacheSuccess ? 'Cache Cleared!' : 'Clear Search Cache'}</span>
            </button>
          </Card>
        </div>
      )}
    </div>
  );
};
