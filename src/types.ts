export interface Paper {
  id: string;
  title: string;
  authors: string[];
  year?: number;
  venue?: string;
  abstract?: string;
  doi?: string;
  source_url?: string;
  pdf_url?: string;
  citations?: number;
  quartile?: string;
  source: string;
  score?: number;
  open_access: boolean;
  biblio?: Biblio;
}

/** Bibliographic details reference managers need beyond title/authors/venue. */
export interface Biblio {
  volume?: string;
  issue?: string;
  /** "123-130" or a single article number. */
  pages?: string;
  issn?: string;
  publisher?: string;
  keywords?: string[];
  /** OpenAlex primary topic and field. */
  topic?: string;
  field?: string;
}

export interface Workspace {
  id: string;
  name: string;
  description?: string;
  created_at: number;
  updated_at: number;
  paper_count: number;
  query_count: number;
}

export type ReadingStatus = 'unread' | 'reading' | 'read';

export interface WorkspacePaper {
  paper: Paper;
  note?: string;
  added_at: number;
  status?: ReadingStatus;
  favorite?: boolean;
  tags?: string[];
}

export interface WorkspacePaperPatch {
  note?: string;
  status?: ReadingStatus;
  favorite?: boolean;
  tags?: string[];
}

export interface SourceStatus {
  id: string;
  name: string;
  /** false when the current scope excludes this source */
  queried: boolean;
  ok: boolean;
  count: number;
  error?: string | null;
  /** true when the source needs an API key, session or URL before it can answer */
  needs_setup?: boolean;
  /** true when the source was skipped because it failed repeatedly and is cooling down */
  cooling_down?: boolean;
}

export interface SearchResponse {
  query: string;
  total: number;
  /** Matches before the limit cut; used to offer "load more". */
  available_total?: number;
  elapsed_ms: number;
  cache_hit: boolean;
  papers: Paper[];
  sources?: SourceStatus[];
}

export interface AgentLog {
  id: string;
  timestamp: string;
  agent_name: string;
  method: string;
  query: string;
  result_count: number;
  latency_ms: number;
  status: string;
}

export interface TelemetryStats {
  gateway_status: string;
  port: number;
  total_queries: number;
  cache_hit_rate: number;
  avg_latency_ms: number;
  recent_logs: AgentLog[];
}
