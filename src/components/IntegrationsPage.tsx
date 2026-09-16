import React, { useState } from 'react';
import { invoke, isTauri } from '@tauri-apps/api/core';
import { Package, RefreshCw, Download, Eye, Power, Archive } from 'lucide-react';
import { McpManager } from './McpManager';

interface SkillInfo {
  name: string;
  source: string;
  path: string;
  enabled: boolean;
  files: number;
  bytes: number;
  content: string;
}

export const IntegrationsPage: React.FC<{ port: number }> = ({ port }) => {
  const native = isTauri();
  const [source, setSource] = useState('builtin:paper-search');
  const [targetRoot, setTargetRoot] = useState(() => localStorage.getItem('sg_skills_root') || '');
  const [loadedRoot, setLoadedRoot] = useState('');
  const [skills, setSkills] = useState<SkillInfo[]>([]);
  const [preview, setPreview] = useState<SkillInfo | null>(null);
  const [busy, setBusy] = useState(false);
  const [error, setError] = useState('');
  const [message, setMessage] = useState('');

  const perform = async (action: () => Promise<void>) => {
    setBusy(true); setError(''); setMessage('');
    try { await action(); } catch (error) { setError(String(error)); }
    finally { setBusy(false); }
  };
  const refresh = async (root: string) => {
    const items = await invoke<SkillInfo[]>('list_skills', { targetRoot: root });
    setSkills(items); setLoadedRoot(root);
    localStorage.setItem('sg_skills_root', root);
  };
  const install = () => {
    if (!preview || !window.confirm(`Install skill ${preview.name} from ${source} into ${targetRoot}?\nSkills can instruct AI agents to run shell scripts. Only install from sources you trust; the app does not execute contents during installation.`)) return;
    void perform(async () => {
      const result = await invoke<SkillInfo>('install_skill', { source, targetRoot });
      setMessage(`Installed at ${result.path}. Refresh skills in your AI client to detect the new skill.`);
      await refresh(targetRoot);
    });
  };

  return <section className="page-container" style={{ display: 'flex', flexDirection: 'column', gap: 16 }}>
    <div className="page-header"><div>
      <h2><Package size={20} /> Skills & Integrations</h2>
      <p className="page-subtitle">Connect your client. Add skills when needed.</p>
      <p className="page-subtitle">
        For Claude, Codex and Antigravity, Settings → AI Clients installs the MCP server and these skills
        in one click. The manual controls below are for any other client.
      </p>
    </div></div>
    {!native && <div className="alert alert-warning" role="status">File installation is only supported in the desktop Tauri application. Web browser sandbox cannot access local filesystem paths.</div>}
    {error && <div className="alert alert-danger" role="alert">{error}</div>}
    {message && <div className="alert alert-success" role="status" style={{ overflowWrap: 'anywhere' }}>{message}</div>}

    <fieldset disabled={busy || !native} className="cockpit-card" style={{ display: 'grid', gap: 12, minWidth: 0 }}>
      <legend>1. Target Skills Directory</legend>
      <label htmlFor="skills-target">Absolute path to an existing directory</label>
      <input id="skills-target" className="field-input" placeholder="/path/to/AI-client/skills" value={targetRoot}
        onChange={(event) => { setTargetRoot(event.target.value); setSkills([]); setLoadedRoot(''); }} />
      <p className="page-subtitle">Symlinks and relative ".." paths are not permitted. Select the folder where your AI client (e.g. Claude Desktop, Codex) discovers custom skills.</p>
      <button id="skills-refresh" className="action-btn" disabled={!targetRoot.trim()} onClick={() => void perform(() => refresh(targetRoot))}><RefreshCw size={14} /> Read Directory</button>
    </fieldset>

    <fieldset disabled={busy || !native} className="cockpit-card" style={{ display: 'grid', gap: 12, minWidth: 0 }}>
      <legend>2. Preview & Install Skill</legend>
      <label htmlFor="skills-bundle">Skill</label>
      <select id="skills-bundle" className="field-input" value={source.startsWith('builtin:') ? source : ''}
        onChange={(event) => { setSource(event.target.value); setPreview(null); }}>
        <option value="builtin:paper-search">Paper search</option>
        <option value="builtin:paper-collect">Paper collection</option>
        <option value="builtin:research-resume">Resume research</option>
        <option value="">Local folder…</option>
      </select>
      {!source.startsWith('builtin:') && <><label htmlFor="skills-source">Source folder containing SKILL.md</label>
      <input id="skills-source" className="field-input" placeholder="/path/to/source-skill" value={source}
        onChange={(event) => { setSource(event.target.value); setPreview(null); }} /></>}
      <div style={{ display: 'flex', gap: 8, flexWrap: 'wrap' }}>
        <button id="skills-preview" className="action-btn" disabled={!source.trim()} onClick={() => void perform(async () => setPreview(await invoke<SkillInfo>('preview_skill', { source })))}><Eye size={14} /> Preview</button>
        <button id="skills-install" className="action-btn action-btn-primary" disabled={!preview || !targetRoot.trim()} onClick={install}><Download size={14} /> Install Skill</button>
      </div>
      {preview && <article>
        <h3>{preview.name}</h3><p>{preview.files} files · {(preview.bytes / 1024).toFixed(1)} KiB</p>
        <pre style={{ whiteSpace: 'pre-wrap', overflowWrap: 'anywhere', maxHeight: 320, overflow: 'auto' }}>{preview.content}</pre>
      </article>}
    </fieldset>

    <section className="cockpit-card" aria-labelledby="skills-installed-title">
      <h2 id="skills-installed-title">Installed ScholarGateway Skills</h2>
      <p className="page-subtitle">Disabling renames SKILL.md to SKILL.md.disabled. Removing moves the folder to a recoverable hidden archive rather than permanently deleting.</p>
      {loadedRoot && skills.length === 0 && <p>No app-managed skills found in this directory.</p>}
      {!loadedRoot && <p>Enter target skills directory and click Read Directory.</p>}
      {skills.map((skill) => <article key={skill.path} style={{ borderTop: '1px solid var(--cockpit-border)', padding: '12px 0' }}>
        <h3>{skill.name} <span className="cockpit-badge">{skill.enabled ? 'ENABLED' : 'DISABLED'}</span></h3>
        <p style={{ overflowWrap: 'anywhere' }}>Source: {skill.source}</p>
        <p style={{ overflowWrap: 'anywhere' }}>Target: {skill.path}</p>
        <div style={{ display: 'flex', gap: 8, flexWrap: 'wrap' }}>
          <button id={`skill-view-${skill.name}`} className="action-btn" onClick={() => { setPreview(skill); setSource(skill.source); }}><Eye size={14} /> View Contents</button>
          <button id={`skill-toggle-${skill.name}`} className="action-btn" disabled={busy || !native} onClick={() => void perform(async () => {
            await invoke('set_skill_enabled', { targetRoot: loadedRoot, name: skill.name, enabled: !skill.enabled });
            await refresh(loadedRoot);
          })}><Power size={14} /> {skill.enabled ? 'Disable' : 'Enable'}</button>
          <button id={`skill-remove-${skill.name}`} className="action-btn" disabled={busy || !native} onClick={() => {
            if (!window.confirm(`Remove ${skill.name}? The directory will be moved to a recoverable archive.`)) return;
            void perform(async () => {
              const archive = await invoke<string>('remove_skill', { targetRoot: loadedRoot, name: skill.name });
              setMessage(`Removed. Restore by moving directory ${archive} back to ${skill.path}.`);
              await refresh(loadedRoot);
            });
          }}><Archive size={14} /> Archive & Remove</button>
        </div>
      </article>)}
    </section>
    <McpManager port={port} />
  </section>;
};
