import searchCatalog from '../../lib/searchCatalog.json';
import { getGatewayPort } from '../../lib/gateway';

// The catalog also lists sources the engine cannot query yet; never offer those.
export const SOURCES_LIST = searchCatalog.sources.filter((source) => source.available !== false);
export type CatalogSource = (typeof SOURCES_LIST)[number];
export const PRESETS = searchCatalog.presets.filter((preset) => preset.id === 'custom' || preset.sources.length > 0);

// Stored credentials are never sent to the UI; the backend substitutes this
// sentinel and ignores it on write, so an untouched field keeps the secret.
export const KEEP_SENTINEL = '__SG_KEEP__';
export const isConfiguredSecret = (value: string | undefined) => value === KEEP_SENTINEL;

export interface CredentialMeta {
  label: string;
  secret: boolean;
  type?: string;
  placeholder?: string;
}

export const CREDENTIAL_LABELS: Record<string, CredentialMeta> = {
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
  proxy_url: { label: 'HTTP(S) Proxy URL', secret: true, placeholder: 'http://user:password@proxy.example:8080' },
};

export type LoginService = 'consensus' | 'openevidence' | 'openai' | 'anthropic' | 'gemini' | 'deepseek' | 'perplexity';

/** The sources a fresh install searches until the user changes anything. */
export const DEFAULT_PRESET = 'auto';
export const DEFAULT_SOURCES = searchCatalog.presets.find((preset) => preset.id === DEFAULT_PRESET)?.sources ?? [];

export const PRIMARY_PRESETS = [
  { id: 'auto', label: '⚡ Auto Discovery' },
  { id: 'vietnam', label: '🇻🇳 Vietnam Research' },
  { id: 'biomedical', label: '🧬 Biomedical & Clinical' },
  { id: 'ai_cs', label: '🤖 AI & Computer Science' },
  { id: 'stem_nature', label: '🔬 STEM & Physics' },
  { id: 'social_humanities', label: '📚 Social & Humanities' },
  { id: 'evidence_review', label: '📊 Evidence Review' },
  { id: 'open_access', label: '🔓 Open Access' },
  { id: 'exhaustive', label: '🌐 All Sources' },
  { id: 'custom', label: '🛠️ Custom Selection' },
];

const GROUP_ORDER = [
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

/** Catalog sources grouped by domain, in a fixed reading order. */
export const DOMAIN_GROUPS: { name: string; sources: CatalogSource[] }[] = (() => {
  const map = new Map<string, CatalogSource[]>();
  for (const src of SOURCES_LIST) map.set(src.group, [...(map.get(src.group) ?? []), src]);
  const keys = [...GROUP_ORDER.filter((k) => map.has(k)), ...[...map.keys()].filter((k) => !GROUP_ORDER.includes(k))];
  return keys.map((name) => ({ name, sources: map.get(name) ?? [] }));
})();

/** Outcome of a single source health probe, as the sources tab renders it. */
export interface SourceHealth {
  loading: boolean;
  ok?: boolean;
  count?: number;
  elapsedMs?: number;
  needsSetup?: boolean;
  error?: string | null;
  query?: string;
  checkedAt?: number;
}

export function defaultConfig() {
  return {
    domain_preset: 'auto',
    topic_setup_completed: 'false',
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
    core_api_key: '',
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
    gateway_port: String(getGatewayPort()),
    cache_ttl_hours: '24',
    max_results_default: '15',
    search_timeout_seconds: '12',
    search_delay_ms: '1500',
    rate_limit_per_minute: '0',
    proxy_enabled: 'false',
    proxy_url: '',
    download_directory: '',
  };
}

export type SettingsConfig = ReturnType<typeof defaultConfig>;
export type SettingsKey = keyof SettingsConfig;

/** Everything a settings tab needs to read and edit the shared form. */
export interface SettingsFormApi {
  config: SettingsConfig;
  set: (field: string, value: string) => void;
  update: (patch: Partial<SettingsConfig>) => void;
  onError: (message: string) => void;
}

export function isValidHttpUrl(value: string): boolean {
  try {
    const url = new URL(value);
    return ['http:', 'https:'].includes(url.protocol) && !url.username && !url.password && !url.hash;
  } catch {
    return false;
  }
}
