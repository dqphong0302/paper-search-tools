# 🎓 ScholarGate Desktop

> **A self-contained native desktop app (Tauri 2 + Rust) with a localhost agent gateway (default `8795`) for AI agents and academic research workspaces.**

ScholarGate combines two jobs in one fully local app (macOS/Windows/Linux — no Docker, Proxmox or Python required):

1. **For researchers**: multi-source paper search, saving into **per-project workspaces**, notes, a **citation graph**, Zotero/BibTeX export, and in-depth analysis tools (PRISMA / meta-analysis).
2. **As a portal for AI agents**: a loopback gateway exposing **REST** + **MCP** so agents can call the search tools, read a workspace and write results back into the right project.

---

## 🌟 Key features

### Search engine (Rust, in-process)
- **A catalog of 66 first-party sources** (cleaned up: every bot-blocked site and every site without an open API has been removed). Main groups: multidisciplinary indexes (OpenAlex, Crossref, Semantic Scholar, OpenAIRE, CORE*, Dimensions*, Web of Science*), preprints (arXiv), biomedical (PubMed, Europe PMC, PMC, PLOS, bioRxiv/medRxiv, ClinicalTrials.gov, ClinVar, NCBI GEO), open access (DOAJ, Zenodo), humanities (HAL), data and reports (Dryad, Dataverse, Figshare, NTRS, World Bank, SEC EDGAR), regional sources (CiNii, AJOL, ThaiJO, VJOL, VAST, JST-HUST, VNU-JS, BanglaJOL, NepJOL…), domain data (UniProt, openFDA, CISA KEV) and keyed sources (marked `*`: Scopus, IEEE, Springer, Perplexity). An optional **external SearXNG** connector adds metasearch.
- **Record kind**: every result is classified as `article` (default), `dataset` (Dryad/Dataverse/Zenodo/HF Datasets/Figshare/NCBI GEO), `report` (NTRS/World Bank/SEC EDGAR), `advisory` (NVD CVE/CISA KEV), `discussion` (Stack Exchange) or `record` (UniProt/openFDA/ClinVar). The UI shows a kind badge and offers a **Kind** filter in Explorer, so non-article data is never presented as a paper.
- Results are merged with **Reciprocal Rank Fusion (RRF k=60)**, duplicate metadata is combined, and Open Access PDF links are filled in via **Unpaywall** (when an email is configured).
- **Bibliographic metadata** — volume, issue, pages, ISSN, publisher and keywords — is collected from OpenAlex, Crossref, PubMed and Europe PMC and merged across duplicates, and records keep up to 100 authors, so RIS/BibTeX exports import completely into Zotero, EndNote and Mendeley.
- **PDF download with fallback**: when the advertised PDF link is blocked or missing, the gateway looks up other open-access copies of the DOI (OpenAlex locations, and Unpaywall when an email is configured) before giving up.
- Every paper carries a **`source_url`** pointing at the original record (OpenAlex/PubMed/arXiv/Crossref/Europe PMC…), even when there is no DOI.
- Year filtering and `open_access_only` are applied **server-side**; the response carries `total` (returned) and `available_total` (matches before the cut) for pagination.
- SQLite cache with a configurable TTL, query history and telemetry.
- **Per-source health check**: each source has a Check button that sends one real query and reports what came back — result count, latency, or the source's own error. It deliberately bypasses the response cache and the failure breaker, so the answer describes the source right now.

### Per-project research workspaces
- Create/rename/delete **workspaces**; each saved paper carries a **note**, **reading status** (unread/reading/read), **favourite** flag and **tags**; one paper can belong to several workspaces.
- Search queries (which can be **saved**) and downloaded PDFs are recorded per workspace; review the whole project in the **Library** tab.
- Downloaded papers open in a built-in **PDF reader** with page navigation, zoom, local full-text extraction, search, copy and Markdown export. Files never leave the machine; image-only scans are reported as requiring OCR.
- **Portable backup and restore** preserves workspaces, papers, notes, tags, reading state and history without exporting API keys, tokens, agent credentials, cache or PDF binaries.
- **Hand-off to an agent**: copy the `workspace_id` plus instructions; the agent reads the whole project through the MCP `get_workspace` tool and keeps searching with the same `workspace_id`.
- The default workspace adopts any papers saved before workspaces existed.

### Gateway for AI agents
- **REST** (`/api/search`, `/api/download`, …), **MCP Streamable HTTP** (`/mcp`, stateless JSON) and legacy-compatible **MCP SSE** (`/sse`, `/messages`).
- Serves Claude Desktop, Claude Code, Codex, Antigravity, Cursor, Windsurf, OpenClaw, a Python CLI, LangChain…
- **One-click client setup**: Settings → AI Clients detects the clients installed on this machine, shows whether ScholarGate is already wired into each one, and installs the MCP entry plus the bundled skills.
- **Security**: an optional token protects **every** route; API keys and tokens are stored **write-only**; SQLite files are `0600`.

---

## 🚀 Install and run

Requires **Rust ≥ 1.75**, **Node ≥ 18** and **pnpm**.

```bash
cd /Volumes/DATA/workspace/paper-search-tools
pnpm install
pnpm tauri dev      # dev: Vite :1420 + gateway :8795
pnpm tauri build    # package .dmg / .exe
```

Testing:
```bash
pnpm build                                                # tsc + vite build
pnpm test                                                 # Vitest (citations, evaluation, settings UI)
cargo test --manifest-path src-tauri/Cargo.toml --bin scholargate
scripts/mcp-smoke.sh http://127.0.0.1:8795 "$MCP_AUTH_TOKEN"   # curl smoke test
pnpm mcp:check "http://127.0.0.1:8795/mcp" "$MCP_AUTH_TOKEN"   # official MCP SDK
```

CI validates the frontend and Rust backend on macOS, Windows and Linux. Tagged releases build signed updater artifacts for macOS (Apple Silicon/Intel), Windows x64 and Linux x64, publish SHA-256 checksums, and expose an in-app **Update Center**. OS publisher signing/notarization is applied when its certificate secrets are configured. Secrets use the OS keychain; set `SCHOLARGATE_KEYCHAIN=0` to disable it.

---

## 🧭 The app shell

The interface is **English throughout**, including preset and source-group names. The search box is **pinned to the top bar** (`⌘K`) and is the single entry point; results open in the **Search** tab.

| Workspace | Contents |
|---|---|
| **Search** | **Academic Papers / Academic Web** toggle. An overview landing page before the first query; Explorer once there are results (filter by source, year, OA, sort, download PDFs, mark papers of interest, export ticked results as .RIS/.bib, "Load more"). Results sit directly under the filter bar; the two analysis panels (**Document Overview**, **Sample Audit & Landscape**) sit **below** the list and are collapsed by default. |
| **Library** | The **Interest Library**: papers marked with **Interest** (notes, tags, reading status, .RIS/.bib export, batch PDF download), downloaded PDFs, search history, and the research tools. |
| **Connections** | Per-workspace agent access; MCP & skills; gateway activity. |
| **Settings** | Four groups: **Search Sources** (presets, enable/disable, credential readiness, per-source health checks), **Connections & Keys** (LLM providers, source credentials, MetaSearch/SearXNG), **AI Clients** (install MCP + skills into Claude, Codex, Antigravity), **Gateway & Security** (token, port, rate limit, timeout, cache, download directory, Update Center, backup/restore). |

The desktop UI works on a single **Interest Library** (the default workspace). Additional workspaces are created and used through the REST API and MCP tools (`/api/workspaces`, `list_workspaces`, `get_workspace`, `save_paper_to_workspace`), which is how agents keep per-project results apart.

---

## 🔎 Search sources

- A fresh install starts on the **Auto** preset, which searches ten sources (OpenAlex, Semantic Scholar, Crossref, PubMed, arXiv, VJOL, VAST, VISTA NASATI, DOAJ, Zenodo) and routes each query to the ones that fit it. **Restore defaults** in Settings → Search Sources returns to exactly that selection at any time.
- Switching from a preset to **Custom Selection** carries the currently active sources over, so a custom list starts from what was already being searched rather than from nothing.
- **Check** on a source, or **Check active sources** for the whole selection, probes the sources for real, four at a time. Each result is one of: a result count with latency, "reachable, 0 results" (the source answered, the probe query just did not match), a credential warning, or the source's own error. Checking a source also clears its failure cooldown, so a source that has recovered is used again immediately.

---

## 🤖 Connecting an AI agent

The default port is `8795` (change it in **Settings → Gateway & Security**, save, quit fully from the tray and reopen; valid range 1024–65535, excluding 1420).

### Gateway security

**Connections → Access**: set an administrator token in Settings first, then create a connection with a name, Read only / Read & write permission, and specific workspaces. An agent token is shown once; the app stores its SHA-256 rather than the token. Copy the MCP config, then Test access to verify with the new token itself. Revoking disables the token for REST/MCP/SSE; requests already in flight may still finish.

An agent can only list and read the workspaces it was granted; write permission applies to papers and notes in those workspaces. Agents cannot change settings, manage other tokens, read global telemetry, open files or download files directly. Academic and web search still use the shared sources; a read-only grant does not write project history (operational logs and the cache are still updated). A paper that belongs only to a workspace outside the grant cannot be read through the details/citations API or copied in with save. Rate limiting uses the authenticated identity, never a self-declared client name. SSE streams are bound to their session owner. All agents must be revoked before the administrator token can be removed.

When a **gateway token** is set, every route (`/api/*`, `/mcp`, `/sse`, `/messages`) requires `Authorization: Bearer <token>`; only `/health`, `/api/health` and `GET /api/config` (with secrets filtered out) stay public so the UI can start. API keys and tokens are **write-only**: the backend returns the sentinel `__SG_KEEP__` instead of the real value, and sending the sentinel back is a no-op.

- **Keychain**: secrets live in the **OS keychain** (service `scholargate`), not in plaintext in SQLite; if the keychain is unavailable the app falls back to SQLite (set `SCHOLARGATE_KEYCHAIN=0` to disable it entirely). Values are cached in-process so the keychain is not read on every request.
- **Rate limit**: `rate_limit_per_minute` (0 = off) limits each agent (by `x-sg-agent`/token) and answers `429` with `Retry-After`.
- SQLite files and the data directory are `0600`/`0700` (Unix). The gateway binds loopback only and rejects non-loopback Origin/Host headers and DNS rebinding.

### MCP
Streamable HTTP endpoint: `http://127.0.0.1:8795/mcp` (add the header only if a token is set). Claude Code requires the explicit HTTP transport:

```json
{
  "mcpServers": {
    "scholargate": {
      "type": "http",
      "url": "http://127.0.0.1:8795/mcp",
      "headers": { "Authorization": "Bearer <MCP_AUTH_TOKEN>" }
    }
  }
}
```

Codex uses `[mcp_servers.scholargate]` with the same `/mcp` URL and `bearer_token_env_var = "SCHOLARGATE_TOKEN"`. Antigravity uses its documented SSE shape instead: `{ "serverUrl": "http://127.0.0.1:8795/sse" }`. Drop `headers` when no token is set.

**The 8 MCP tools:**

| Tool | Parameters | Purpose |
|---|---|---|
| `search_academic_papers` | `query`, `sources`, `limit`, `year_min`, `year_max`, `open_access_only`, `offset`, `workspace_id` | Multi-source search (paginate with `offset`); omit `sources`/`limit` to use the saved settings; the query is recorded against `workspace_id`. |
| `get_paper_details` | `paper_id` | Details from the library/cache, or an exact DOI lookup on Crossref. |
| `get_citations` | `paper_id`, `direction` (`references`/`cited_by`/`related`), `limit` | Citation graph and related papers via OpenAlex (DOI/PMID/OpenAlex ID). |
| `get_search_catalog` | — | Disciplines, sources and credential field names (never a secret value). |
| `list_workspaces` | — | Workspaces with paper and query counts. |
| `get_workspace` | `workspace_id`, `query_limit`, `limit`, `offset`, `fields` | Read a project page by page; 20 papers and 5 queries on the first page by default; follow `next_offset` until it is `null`. |
| `save_paper_to_workspace` | `workspace_id`, `paper`, `note` | Save a paper into a workspace; saving the same paper again updates the note. |
| `search_web` | `query`, `limit` | General web search through the configured SearXNG connector (not papers). |

- `search_academic_papers` returns `total`, `available_total`, `papers[]`, `sources[]` (per-source status), `elapsed_ms` and `cache_hit`. `offset` (0–10000) pages through one ranked, cached result set instead of re-querying the sources for each page. `available_total` is a **lower bound**: each source contributes at most 100 records (some connectors cap lower). Changing a source credential selects a different cache entry.
- `sources` accepts source or preset IDs from `get_search_catalog`; `[]` disables all of them; the old aliases `auto`/`all`/`international`/`vjol`/`searxng` are still accepted.
- `search_academic_papers`, `get_workspace`, `get_citations` and `get_paper_details` accept `fields` to select metadata. The default list omits abstracts; details default to everything. ID, title, source, source URL and DOI are always kept. Search also returns `next_offset`; source status and errors are never truncated. Saving a trimmed paper uses the full record from the library/cache when one exists.
- `search_web` only uses the dedicated web connector (category `general`); snippets are untrusted content, not peer-reviewed evidence.
- Verified with the **official MCP TypeScript SDK** (`pnpm mcp:check`): connect → `tools/list` (13 tools) → `get_search_catalog` → `list_workspaces` → `get_workspace`, with and without a token. The app must be running for an agent to connect.

### REST from Python
```python
import requests
res = requests.post("http://localhost:8795/api/search", json={
    "query": "gestational diabetes", "limit": 5, "workspace_id": "<optional id>"
})
data = res.json()
for p in data["papers"]:
    print(f"[{p['year']}] {p['title']} ({p['source']}) -> {p.get('source_url')}")
```

---

## 📡 REST API (port `8795`)

The gateway binds loopback only; Origin is restricted to the Vite UI (1420), Tauri, and the gateway's own origin; a non-loopback Host is rejected. Responses carry `Cache-Control: no-store`. When a token is set, every route except health and the filtered `GET /api/config` requires Bearer auth.

| Method | Endpoint | Purpose |
|---|---|---|
| `GET` | `/health`, `/api/health` | Server status |
| `GET` / `POST` | `/api/agents` | List/create agent connections; administrator token only; a new token is returned once |
| `DELETE` | `/api/agents/{id}` | Revoke a connection; the record is kept and no project data is deleted |
| `GET` | `/api/catalog` | Disciplines and sources |
| `POST` | `/api/search` | Multi-source paper search (RRF k=60) |
| `POST` | `/api/web/search` | Web search through the SearXNG connector |
| `POST` | `/api/source/check` | Probe one source (`{"id": "openalex"}`, optional `query`) and report count, latency and error; never served from cache |
| `GET` | `/api/paper/{id}` | Paper details (internal/cache ID or DOI) |
| `GET` | `/api/paper/{id}/citations?direction=references\|cited_by\|related&limit=` | Citation graph and related papers via OpenAlex (ID in the path) |
| `GET` | `/api/citations?id=…&direction=…&limit=` | The same, as a query (for OpenAlex IDs containing `/`) |
| `POST` | `/api/download` | Download a PDF into the download directory |
| `GET` | `/api/downloads/{id}/content` | Read a recorded local PDF in the built-in reader |
| `GET` | `/api/telemetry` | Statistics and the query log |
| `GET` | `/api/trends?geo=VN` | Google Trends RSS (VN/US/GB/SG/AU) |
| `GET` / `DELETE` | `/api/history/searches` | Read/clear history by `?workspace_id=`; omit it to apply to every workspace |
| `PATCH` / `DELETE` | `/api/history/searches/{id}` | Save/unsave (`{"saved":true}`) or delete one entry |
| `GET` | `/api/history/downloads` | Downloaded PDFs (filter with `?workspace_id=`) |
| `DELETE` | `/api/history/downloads/{id}` | Delete a download record |
| `POST` | `/api/open-file` | Open a file with the OS default application |
| `GET` / `POST` | `/api/config` | Read (secrets filtered) / write settings |
| `POST` | `/api/test-llm` | Test an LLM key (uses the saved key when the field is empty) |
| `POST` | `/api/test-searxng` | Test a SearXNG instance |
| `POST` | `/api/cache/clear` | Clear the search cache |
| `GET` / `POST` | `/api/workspaces` | List / create workspaces |
| `PATCH` / `DELETE` | `/api/workspaces/{id}` | Rename / delete a workspace |
| `GET` / `POST` | `/api/workspaces/{id}/papers` | Papers in a workspace / add a paper |
| `PATCH` / `DELETE` | `/api/workspaces/{id}/papers?paper_id=…` | Edit `note`/`status`/`favorite`/`tags` / remove a paper |
| `POST` | `/mcp` | MCP Streamable HTTP (notifications answer 202) |
| `GET` | `/sse` | SSE stream for legacy MCP clients |
| `POST` | `/messages?session_id=…` | JSON-RPC for an SSE session; answered on the matching stream |

Client configuration files are **not** editable over REST. Those actions run over Tauri IPC only, so no other local process can rewrite an AI client's config through the gateway.

---

## 🔌 AI clients: MCP and skills

### One click (Settings → AI Clients)

The app detects the clients installed on this machine and reports, per client, whether an MCP entry pointing at this gateway already exists and which bundled skills are installed:

| Client | MCP config | Skills folder |
|---|---|---|
| Claude Desktop | `~/Library/Application Support/Claude/claude_desktop_config.json` (JSON; `%APPDATA%\Claude` on Windows, `~/.config/Claude` on Linux) | — (Claude Desktop does not load skill folders) |
| Codex | `~/.codex/config.toml` (TOML) | `~/.codex/skills` |
| Antigravity 2.0 / IDE / CLI | `~/.gemini/config/mcp_config.json` (JSON; existing legacy config is preserved) | `~/.gemini/config/skills` |
| OpenCode | `~/.config/opencode/opencode.jsonc` (JSONC) | `~/.config/opencode/skills` |

- Install writes one entry named `scholargate` pointing at `http://127.0.0.1:<port>/mcp`, after backing up the previous file. Nothing else in the file is touched: the TOML editor preserves comments and formatting, and the JSON editor keeps every other key.
- A symlinked config (Antigravity ships one) is resolved to the real file, and the resolved path is what the panel displays.
- If a gateway token is set, the JSON clients receive an `Authorization` header — a plaintext file then contains the token, which the panel says. Codex only accepts an environment variable name, so it receives `bearer_token_env_var = "SCHOLARGATE_TOKEN"` and the token stays out of the file; export that variable before starting Codex.
- An entry the app did not create is reported but never overwritten or removed, whatever it is called.
- Installing skills copies the three bundled skills; each one it did not install is left alone, and removal moves folders to a recoverable archive rather than deleting them.

### Manual setup (Connections → MCP & Skills)

For any other client:

- **MCP config**: JSON files with an `mcpServers` key (not TOML/JSONC). Enter an absolute path, read it, compose the server entry and confirm. The app only edits, enables, disables or removes entries it manages, leaving pre-existing ones untouched; every edit writes a `<config>.<uuid>.bak` backup plus a receipt. stdio entries need an absolute executable path. The test button only performs an HTTP `initialize` (it never calls a tool).
- **Local skills**: the source must be an absolute path without symlinks; frontmatter needs a slug `name` and a `description`; an existing skill is never overwritten. Limits: 256 files / 20 MiB / 16 levels. Disabling renames `SKILL.md` to `SKILL.md.disabled`; removing moves the folder into a recoverable archive. Available over Tauri IPC only (a browser cannot write files).
- **Bundled skills**: pick Paper search / Paper collection / Resume research → Preview → Install. The content is embedded in the app from `skills/`; nothing is downloaded.

---

## 🔐 AI provider sign-in

Each provider card in Settings → Connections & Keys has a **Sign in** button that opens that provider's own developer console in an app window (OpenAI, Anthropic, Google AI Studio, DeepSeek, Perplexity). The account login — Google, GitHub, email — happens on the provider's own page, and the session stays inside this app.

A console session is **not** an API credential: calls to these providers still use the API key saved on the same card. Consensus and OpenEvidence work differently — their sessions are exactly what their search sources authenticate with, so signing in there is what makes those sources work. Every captured session is stored write-only like any other secret and can be cleared from the same card.

---

## 📁 Project layout

```
paper-search-tools/
├── README.md                     # This document
├── PLAN.md                       # Roadmap and verification log
├── .gitignore
├── .github/workflows/ci.yml      # CI: frontend build/test + cargo test
├── scripts/mcp-smoke.sh          # MCP smoke test over HTTP
├── package.json                  # Vite, React 19, Lucide, Tauri CLI
├── vite.config.ts
├── src/                          # Frontend (React + TypeScript)
│   ├── App.tsx                   # App shell: Search / Library / Connections + switcher
│   ├── types.ts                  # Paper, Workspace, WorkspacePaper, Telemetry…
│   ├── lib/gateway.ts            # fetch to the gateway (port + Bearer + x-sg-client)
│   ├── lib/citation.ts           # APA / Vancouver / BibTeX / RIS (+ tests)
│   ├── lib/paperEvaluation.ts    # reading-support heuristics (not a quality judgement)
│   ├── styles/design-system.css
│   └── components/
│       ├── layout/SideNav.tsx    # Main navigation + gateway status
│       ├── SearchPage.tsx        # Papers / web toggle
│       ├── CockpitDashboard.tsx  # Overview landing page
│       ├── Explorer.tsx          # Results, filters, PDF downloads, saving, source links, citation graph
│       ├── ResearchGapPanel.tsx  # Landscape of the result set (years/sources/terms) + Google Trends
│       ├── EvidenceSynthesis.tsx # Heuristic abstract summary (with a warning)
│       ├── ResearchWorkspace.tsx # Workspace sub-tabs
│       ├── Library.tsx           # Workspace papers + notes + .RIS
│       ├── DownloadHistory.tsx   # Downloaded PDFs per workspace
│       ├── SearchHistory.tsx     # Queries per workspace
│       ├── WebSearch.tsx         # Web search
│       ├── AgentGateway.tsx      # Status / connection sub-tabs
│       ├── AgentMonitor.tsx      # Telemetry + simulator
│       ├── IntegrationsPage.tsx  # Manual MCP client + skills setup
│       ├── McpManager.tsx        # mcpServers management
│       ├── AiClients.tsx         # One-click MCP + skills setup per AI client
│       ├── ClinicalSuite.tsx     # PRISMA + meta-analysis
│       └── SettingsPage.tsx
└── src-tauri/                    # Rust backend
    ├── Cargo.toml
    ├── tauri.conf.json           # Tauri 2 + CSP
    ├── build.rs
    └── src/
        ├── main.rs               # Entry point, tray, gateway spawn, Tauri IPC, provider sign-in
        ├── server.rs             # REST router + handlers (incl. /api/source/check)
        ├── mcp.rs                # MCP HTTP/SSE, 8 tools
        ├── engine.rs             # Multi-source engine + RRF + Unpaywall + failure breaker
        ├── details.rs            # Paper details / Crossref DOI
        ├── citations.rs          # OpenAlex citation graph (references/cited_by)
        ├── web_search.rs         # SearXNG web connector
        ├── db.rs                 # SQLite: cache, library, workspaces, logs
        ├── secrets.rs            # OS keychain + cache, SQLite fallback
        ├── catalog.rs            # Reads searchCatalog.json
        ├── config.rs             # Settings validation + secret sanitisation
        ├── security.rs           # Loopback/origin/token
        ├── models.rs             # Data structs
        ├── skills.rs             # Local skill install/removal
        ├── integrations.rs       # MCP client config management
        └── clients.rs            # AI client detection + one-click MCP/skill install
```

---

## ✅ Verification status

- `cargo test` — **76 passed / 5 ignored** (the ignored ones need real network access: Crossref/arXiv/multi-source). Covers: RRF/merge and tie ordering, year filtering, source routing, the arXiv parser, cache/telemetry, REST and MCP pagination over one result set, credential changes, workspace lifecycle (status/favourite/tags), workspace-scoped history deletion, REST workspaces over a real TCP socket, token protection for REST+MCP+SSE, rate limiting, secret sanitisation, citation ID resolution, the web connector fixture, skills/integrations, the source-check endpoint (including that it never answers from cache), AI client detection plus install/remove round trips for both JSON and TOML clients against a throwaway home directory, and the default-workspace rename migration.
- `pnpm test` — **35 passed** (citation formats, heuristic evaluation, result landscape; React/jsdom regressions for workspace switching, stale responses, load-more and history deletion; settings regressions for source health checks, default sources and the AI client panel).
- `pnpm build` — TypeScript + Vite pass; `cargo build` is clean with no warnings.
- **Native bundle E2E** — `ScholarGate.app` (debug) built and the real binary run: SQLite init, gateway bind, `/health`, `/api/workspaces` and `/mcp tools/list` all pass; with a token: `401` when missing, `200` when correct; `/api/citations` returns real data; `offset` pagination returns different pages.
- **Official MCP SDK** — `pnpm mcp:check` connects, lists 8 tools and calls catalog/workspace successfully (with and without a token).
- **Live source checks** — `/api/source/check` exercised against the real services: OpenAlex/Crossref/DOAJ/PubMed/Zenodo/VJOL/VAST/VISTA NASATI answered; Scopus reported a rejected credential (HTTP 401) as "needs setup"; SLJOL reported HTTP 403; arXiv and Semantic Scholar reported their own rate limits.
- **Not verified**: full GUI click-through E2E; some narrow viewports; gap analysis is still a description of the result set, not a scientific conclusion.

## ⚠️ Known limitations

- Without a token, any local process — not just a browser — can call the API. Set a token if untrusted processes run on this machine.
- `total` is the number of papers returned; use `available_total` for the number of matches.
- Web search has no cache or history, and is not peer-reviewed evidence.
- The "SCREENING" score and "suggested PDF" flags are heuristics that help pick papers to read. They are **not** a judgement of methodological quality or strength of evidence.
- A source health check reports one probe at one moment. "Reachable, 0 results" means the source answered but had nothing for the probe query, and a rate-limited source (arXiv, Semantic Scholar) may fail a check immediately after several other queries and pass a minute later.
