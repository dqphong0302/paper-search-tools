import React from 'react';
import { AlertTriangle, Cpu, Database, ExternalLink, LogIn, RefreshCw, Search, ShieldCheck, Sparkles, Terminal, Zap } from 'lucide-react';
import { invoke, isTauri } from '@tauri-apps/api/core';
import { isConfiguredSecret, isValidHttpUrl, KEEP_SENTINEL, LoginService, SettingsFormApi, SOURCES_LIST } from './model';
import { Card, CheckboxField, Field, SecretField, TestStatus, TextField } from './ui';
import { useConnectionTests } from './useConnectionTests';

interface Provider {
  id: string;
  name: string;
  key: string;
  url?: string;
  login?: LoginService;
  icon: React.ReactNode;
}

const PROVIDERS: Provider[] = [
  { id: 'gemini', name: 'Google Gemini', key: 'gemini_api_key', url: 'https://aistudio.google.com/apikey', login: 'gemini', icon: <Sparkles size={15} className="icon-accent" /> },
  { id: 'perplexity', name: 'Perplexity Sonar', key: 'perplexity_api_key', url: 'https://www.perplexity.ai/settings/api', login: 'perplexity', icon: <Sparkles size={15} className="icon-accent" /> },
  { id: 'deepseek', name: 'DeepSeek', key: 'deepseek_api_key', url: 'https://platform.deepseek.com/api_keys', login: 'deepseek', icon: <Cpu size={15} className="icon-accent" /> },
  { id: 'openai', name: 'OpenAI / Compatible', key: 'openai_api_key', login: 'openai', icon: <Zap size={15} className="tone-emerald" /> },
  { id: 'anthropic', name: 'Anthropic Claude', key: 'anthropic_api_key', url: 'https://console.anthropic.com/settings/keys', login: 'anthropic', icon: <Terminal size={15} className="tone-amber" /> },
  { id: 'groq', name: 'Groq', key: 'groq_api_key', url: 'https://console.groq.com/keys', icon: <Zap size={15} className="icon-accent" /> },
];

const WEB_SESSIONS: { service: LoginService; name: string; icon: React.ReactNode; blurb: string; placeholder: string }[] = [
  {
    service: 'consensus',
    name: 'Consensus.app',
    icon: <Sparkles size={15} className="icon-accent" />,
    blurb: 'Scientific evidence search, study design classification (RCT, Meta-analysis, Cohort), and verbatim claim extraction.',
    placeholder: 'Paste __session=... or Bearer JWT',
  },
  {
    service: 'openevidence',
    name: 'OpenEvidence',
    icon: <ShieldCheck size={15} className="icon-accent" />,
    blurb: 'AI clinical decision assistant synthesizing peer-reviewed citations and clinical guidelines.',
    placeholder: 'Paste cookie or session token',
  },
];

export const ConnectionsTab: React.FC<SettingsFormApi> = ({ config, set, onError }) => {
  const values = config as Record<string, string>;
  const tests = useConnectionTests();

  const testLlm = (provider: string, apiKey: string, baseUrl?: string, model?: string) => {
    if (baseUrl && !isValidHttpUrl(baseUrl)) return tests.fail(provider, 'Enter a valid HTTP(S) base URL without credentials or a fragment.');
    const freshKey = apiKey && apiKey !== KEEP_SENTINEL ? apiKey : '';
    void tests.run(provider, '/api/test-llm', { provider, api_key: freshKey, base_url: baseUrl, model, use_saved: !freshKey });
  };

  const testSearxng = () => {
    if (!isValidHttpUrl(config.searxng_url)) return tests.fail('searxng', 'Enter a valid HTTP(S) SearXNG base URL.');
    void tests.run('searxng', '/api/test-searxng', { url: config.searxng_url, categories: config.searxng_categories, engines: config.searxng_engines });
  };

  const openLogin = async (service: LoginService) => {
    if (!isTauri()) {
      onError('Webview login is only available in the desktop application.');
      return;
    }
    try {
      await invoke('open_service_login', { service });
    } catch (e) {
      onError(`Unable to open login window: ${e}`);
    }
  };

  const clearSession = async (service: LoginService) => {
    if (isTauri()) {
      try {
        await invoke('clear_service_session', { service });
      } catch (e) {
        onError(`Unable to clear ${service} session: ${String(e)}`);
        return;
      }
    }
    set(`${service}_session`, '');
  };

  const testButton = (id: string, onClick: () => void, disabled: boolean, label = 'Test') => (
    <button className="action-btn action-btn-xs" onClick={onClick} disabled={disabled || tests.results[id]?.loading}>
      {tests.results[id]?.loading ? <RefreshCw size={12} className="animate-spin" /> : <Zap size={12} />}
      <span>{label}</span>
    </button>
  );

  return (
    <div className="u-stack u-gap-14">
      <Card advanced title="AI Model API Keys (LLM)" subtitle="Used for testing connectivity; evidence synthesis operates deterministically without external transmissions" icon={<Cpu size={16} className="icon-accent" />}>
        <div className="settings-grid settings-grid-280">
          {PROVIDERS.map((provider) => {
            const signedIn = provider.login ? isConfiguredSecret(values[`${provider.login}_session`]) : false;
            return (
              <div key={provider.id} className="settings-panel">
                <div className="u-row u-between">
                  <span className="settings-panel-title">{provider.icon}{provider.name}</span>
                  {provider.url && <a href={provider.url} target="_blank" rel="noreferrer" className="settings-link">Get Key <ExternalLink size={11} /></a>}
                </div>
                {provider.login && (
                  <div className="u-row u-gap-6">
                    <button
                      id={`signin-${provider.id}`}
                      type="button"
                      className="action-btn action-btn-xs"
                      onClick={() => void openLogin(provider.login!)}
                      title={`Open ${provider.name}'s developer console in an app window and sign in with your account`}
                    >
                      <LogIn size={12} />
                      <span>{signedIn ? 'Sign in again' : 'Sign in'}</span>
                    </button>
                    <span className={`session-state ${signedIn ? 'tone-emerald' : 'text-dim'}`}>{signedIn ? '● Console session saved' : '○ Not signed in'}</span>
                    {signedIn && <button type="button" className="action-btn action-btn-2xs tone-rose" onClick={() => void clearSession(provider.login!)}>Clear</button>}
                  </div>
                )}
                <SecretField name={provider.key} value={values[provider.key] ?? ''} onChange={set} placeholder="sk-..." />
                {provider.id === 'openai' && (
                  <TextField label="Base URL" value={config.openai_base_url} onValue={(v) => set('openai_base_url', v)} placeholder="https://api.openai.com/v1" />
                )}
                <div className="u-row">
                  {testButton(provider.id, () => testLlm(provider.id, values[provider.key], provider.id === 'openai' ? config.openai_base_url : undefined), values[provider.key] === '')}
                  <div className="u-grow"><TestStatus status={tests.results[provider.id]} /></div>
                </div>
              </div>
            );
          })}
        </div>

        <p className="settings-note u-row u-gap-6 u-align-start">
          <AlertTriangle size={12} className="tone-amber" />
          <span>
            Sign in opens the provider's own developer console in an app window, so the account login
            (Google, GitHub, email) happens on the provider's page and the session stays in this app.
            A console session is not an API credential: calls to these providers still use the API key
            saved above. Consensus and OpenEvidence are different — their sessions are what their
            search sources authenticate with.
          </span>
        </p>

        <div className="settings-panel ollama-grid">
          <TextField label="Ollama Base URL (Local / Offline)" value={config.ollama_base_url} onValue={(v) => set('ollama_base_url', v)} placeholder="http://localhost:11434" />
          <TextField label="Model" value={config.ollama_model} onValue={(v) => set('ollama_model', v)} placeholder="qwen2.5:7b" />
          {testButton('ollama', () => testLlm('ollama', 'local', config.ollama_base_url, config.ollama_model), false, 'Test Port')}
          <div className="u-span-all"><TestStatus status={tests.results.ollama} /></div>
        </div>
      </Card>

      <Card
        advanced
        title="Web Sessions (Consensus.app & OpenEvidence)"
        subtitle="Sign in directly via an in-app Webview window or paste session token for independent desktop access"
        icon={<ShieldCheck size={16} className="icon-accent" />}
      >
        <div className="settings-grid settings-grid-320">
          {WEB_SESSIONS.map(({ service, name, icon, blurb, placeholder }) => {
            const key = `${service}_session`;
            const connected = isConfiguredSecret(values[key]);
            return (
              <div key={service} className="settings-panel settings-panel-lg">
                <div className="u-row u-between">
                  <span className="settings-panel-title">{icon}{name}</span>
                  <span className={`status-chip ${connected ? 'ok' : ''}`}>{connected ? '● Session saved' : '○ Not connected'}</span>
                </div>
                <div className="settings-note">{blurb}</div>
                <div className="u-row">
                  <button type="button" className="action-btn action-btn-xs" onClick={() => void openLogin(service)}>
                    <ExternalLink size={12} />
                    <span>{connected ? 'Re-login via Webview' : `Sign in to ${name.replace('.app', '')} via Webview`}</span>
                  </button>
                  {connected && <button type="button" className="action-btn action-btn-xs tone-rose" onClick={() => void clearSession(service)}>Clear Session</button>}
                </div>
                <details className="manual-secret">
                  <summary>Paste Cookie / Token manually</summary>
                  <SecretField name={key} value={values[key] ?? ''} onChange={set} placeholder={placeholder} />
                </details>
              </div>
            );
          })}
        </div>
      </Card>

      <Card title="Academic Source Credentials" subtitle="Emails are used for API polite pools; API keys increase rate limits. Secrets use the OS keychain when available." icon={<Database size={16} className="icon-accent" />}>
        <div className="settings-grid settings-grid-260">
          {SOURCES_LIST.filter((source) => (source.credentials as string[]).length > 0).map((source) => (
            <div key={source.id} className="settings-panel">
              <span className="settings-panel-title">{source.name}</span>
              {(source.credentials as string[]).map((key) => <SecretField key={key} name={key} value={values[key] ?? ''} onChange={set} />)}
            </div>
          ))}
        </div>
      </Card>

      <Card
        advanced
        title="MetaSearch & Web Discovery (SearXNG)"
        subtitle="Native MetaSearch (Europe PMC) runs locally; SearXNG connector is optional for open web queries"
        icon={<Search size={16} className="icon-accent" />}
        right={<CheckboxField label="Use external SearXNG for metasearch" checked={config.searxng_enabled === 'true'} onChange={(on) => set('searxng_enabled', on ? 'true' : 'false')} />}
      >
        <div className="settings-grid settings-grid-240">
          <TextField label="SearXNG Base URL (Academic)" value={config.searxng_url} onValue={(v) => set('searxng_url', v)} placeholder="http://localhost:8080" />
          <Field label="Category">
            <select className="settings-input" value={config.searxng_categories} onChange={(e) => set('searxng_categories', e.target.value)}>
              <option value="science">science (Academic & Medical)</option>
              <option value="general">general (Entire Web)</option>
            </select>
          </Field>
          <TextField label="Engines" value={config.searxng_engines} onValue={(v) => set('searxng_engines', v)} placeholder="google scholar, pubmed, arxiv" />
        </div>
        <div className="u-row u-gap-10 settings-footer-row">
          <button className="action-btn action-btn-primary" onClick={testSearxng} disabled={!config.searxng_url || tests.results.searxng?.loading}>
            {tests.results.searxng?.loading ? <RefreshCw size={13} className="animate-spin" /> : <Zap size={13} />}
            <span>Test Connection</span>
          </button>
          <div className="u-grow"><TestStatus status={tests.results.searxng} /></div>
        </div>
      </Card>
    </div>
  );
};
