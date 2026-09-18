import React, { useState, useMemo } from 'react';
import {
  X,
  Bot,
  Copy,
  Download,
  Check,
  FileCode,
  Sparkles,
  FileText,
  Bookmark,
} from 'lucide-react';
import { Paper } from '../types';
import { bibtexCitation, bibtexLibrary, risLibrary } from '../lib/citation';

export type TargetAgent = 'antigravity' | 'codex' | 'claude' | 'opencode' | 'obsidian';
export type ExportFormat = 'prompt_pack' | 'obsidian_note' | 'bibtex' | 'ris' | 'json';

export interface AiAgentExportModalProps {
  isOpen: boolean;
  onClose: () => void;
  papers: Paper[];
  workspaceName?: string;
}

const AGENT_OPTIONS: { id: TargetAgent; name: string; desc: string; icon: string }[] = [
  {
    id: 'antigravity',
    name: 'Google Antigravity',
    desc: 'Prompt tuned for the Antigravity AI agent and research loop',
    icon: '⚡',
  },
  {
    id: 'claude',
    name: 'Claude Desktop / Claude Code',
    desc: 'Normalised <research_context> XML tag structure',
    icon: '🟣',
  },
  {
    id: 'codex',
    name: 'OpenAI Codex',
    desc: 'Analysis and synthesis format for academic papers',
    icon: '🟢',
  },
  {
    id: 'opencode',
    name: 'OpenCode',
    desc: 'Markdown task and context format for OpenCode',
    icon: '🔵',
  },
  {
    id: 'obsidian',
    name: 'Obsidian Local Vault',
    desc: 'Markdown notes with YAML front matter for a second brain',
    icon: '💎',
  },
];

export const AiAgentExportModal: React.FC<AiAgentExportModalProps> = ({
  isOpen,
  onClose,
  papers,
  workspaceName = 'Interest Library',
}) => {
  const [selectedAgent, setSelectedAgent] = useState<TargetAgent>('antigravity');
  const [format, setFormat] = useState<ExportFormat>('prompt_pack');
  const [copied, setCopied] = useState(false);
  const [downloaded, setDownloaded] = useState(false);

  const isBatch = papers.length > 1;

  const generatedContent = useMemo(() => {
    if (papers.length === 0) return '';

    if (format === 'bibtex') {
      return papers.length === 1 ? bibtexCitation(papers[0]) : bibtexLibrary(papers);
    }

    if (format === 'ris') {
      return risLibrary(papers);
    }

    if (format === 'json') {
      return JSON.stringify(papers, null, 2);
    }

    if (format === 'obsidian_note' || selectedAgent === 'obsidian') {
      if (papers.length === 1) {
        const p = papers[0];
        return [
          '---',
          `title: "${p.title.replace(/"/g, '\\"')}"`,
          `authors: [${p.authors.map((a) => `"${a}"`).join(', ')}]`,
          `year: ${p.year || ''}`,
          `venue: "${p.venue || ''}"`,
          `doi: "${p.doi || ''}"`,
          `source: "${p.source}"`,
          `open_access: ${p.open_access}`,
          `pdf_url: "${p.pdf_url || ''}"`,
          'tags:',
          '  - academic-paper',
          '  - literature-review',
          '  - scholar-gateway',
          '---',
          '',
          `# ${p.title}`,
          '',
          `**Authors**: ${p.authors.join(', ') || 'Unknown'}  `,
          `**Year**: ${p.year || 'N/A'} | **Venue**: *${p.venue || 'N/A'}* | **Source**: \`${p.source}\`  `,
          p.doi ? `**DOI**: [${p.doi}](https://doi.org/${p.doi})  ` : '',
          p.pdf_url ? `**Full-text PDF**: [Open PDF](${p.pdf_url})  ` : '',
          '',
          '## 1. Abstract',
          p.abstract || '*(No abstract available)*',
          '',
          '## 2. Key points & method',
          '- **Problem addressed**:',
          '- **Proposed method**:',
          '- **Results**:',
          '',
          '## 3. Assessment & personal notes',
          '- **Strengths**:',
          '- **Limitations**:',
          '- **Relevance to the current project**:',
          '',
          '## 4. BibTeX citation',
          '```bibtex',
          bibtexCitation(p),
          '```',
        ].filter(Boolean).join('\n');
      } else {
        return [
          '---',
          `title: "Literature review: ${workspaceName}"`,
          `date: ${new Date().toISOString().slice(0, 10)}`,
          `paper_count: ${papers.length}`,
          'tags:',
          '  - literature-review',
          '  - research-synthesis',
          '---',
          '',
          `# Literature Review: ${workspaceName}`,
          '',
          `*Contains ${papers.length} papers exported from ScholarGate.*`,
          '',
          '## Paper list',
          ...papers.map((p, idx) =>
            [
              `### ${idx + 1}. ${p.title}`,
              `- **Authors**: ${p.authors.join(', ')} (${p.year || 'N/A'})`,
              `- **Source**: ${p.source} | **DOI**: ${p.doi || 'N/A'}`,
              `- **Abstract**: ${p.abstract ? p.abstract.slice(0, 300) + '...' : 'N/A'}`,
              '',
            ].join('\n')
          ),
        ].join('\n');
      }
    }

    // Default: AI Research Context Pack (Markdown with system/user prompt)
    const promptHeader = (() => {
      switch (selectedAgent) {
        case 'claude':
          return [
            '<instruction>',
            'You are an expert in academic research and literature review. Below is paper data from ScholarGate. Analyse it in depth, compare the main claims, identify the research gaps and propose the next directions.',
            '</instruction>',
            '',
            '<research_context>',
          ].join('\n');
        case 'codex':
          return [
            '# AI SCIENTIFIC RESEARCH BRIEFING',
            `Project: ${workspaceName}`,
            'Task: Synthesize the methodologies, empirical findings, and synthesize an evidence table from the following academic papers.',
            '',
          ].join('\n');
        case 'opencode':
          return [
            '## Academic Research Context for OpenCode Agent',
            `Objective: Review the following literature pack and extract key algorithms, datasets, and benchmark results.`,
            '',
          ].join('\n');
        case 'antigravity':
        default:
          return [
            '## Academic Research Context & Literature Pack (Google Antigravity)',
            `Goal: a comprehensive literature analysis for the project "${workspaceName}".`,
            'Instructions for the AI agent:',
            '1. Summarise each paper’s method and main scientific contribution.',
            '2. Build an evidence matrix comparing the studies.',
            '3. Identify the limitations and the research gaps.',
            '4. Give concrete recommendations for the research project.',
            '',
          ].join('\n');
      }
    })();

    const promptBody = papers
      .map(
        (p, i) =>
          `### Paper ${i + 1}: ${p.title}\n` +
          `- **Authors**: ${p.authors.join(', ') || 'N/A'}\n` +
          `- **Year / Venue**: ${p.year || 'N/A'} · ${p.venue || 'N/A'} (${p.source})\n` +
          `- **DOI**: ${p.doi || 'N/A'}\n` +
          `- **Open Access**: ${p.open_access ? 'Yes' : 'No'}\n` +
          `- **Abstract**: ${p.abstract || 'No abstract available.'}\n`
      )
      .join('\n');

    const promptFooter = selectedAgent === 'claude' ? '\n</research_context>' : '';

    return `${promptHeader}\n${promptBody}${promptFooter}`;
  }, [papers, format, selectedAgent, workspaceName]);

  if (!isOpen || papers.length === 0) return null;

  const handleCopy = () => {
    navigator.clipboard.writeText(generatedContent).then(() => {
      setCopied(true);
      setTimeout(() => setCopied(false), 2000);
    });
  };

  const handleDownloadFile = () => {
    const ext =
      format === 'bibtex' ? 'bib' : format === 'ris' ? 'ris' : format === 'json' ? 'json' : 'md';
    const mime =
      format === 'bibtex'
        ? 'application/x-bibtex'
        : format === 'ris'
        ? 'application/x-research-info-systems'
        : format === 'json'
        ? 'application/json'
        : 'text/markdown';

    const filename = isBatch
      ? `research_pack_${workspaceName.replace(/\s+/g, '_').toLowerCase()}.${ext}`
      : `paper_${(papers[0].doi || papers[0].id).replace(/[^a-zA-Z0-9]/g, '_')}.${ext}`;

    const blob = new Blob([generatedContent], { type: mime });
    const url = URL.createObjectURL(blob);
    const a = document.createElement('a');
    a.href = url;
    a.download = filename;
    document.body.appendChild(a);
    a.click();
    a.remove();
    URL.revokeObjectURL(url);

    setDownloaded(true);
    setTimeout(() => setDownloaded(false), 2000);
  };

  return (
    <div className="modal-overlay" onClick={onClose} role="dialog" aria-modal="true">
      <div
        className="modal-dialog"
        onClick={(e) => e.stopPropagation()}
        style={{
          maxWidth: 840,
          width: '94vw',
          maxHeight: '90vh',
          display: 'flex',
          flexDirection: 'column',
          padding: 0,
          overflow: 'hidden',
        }}
      >
        {/* Modal Header */}
        <div
          className="modal-header"
          style={{
            padding: '16px 24px',
            borderBottom: '1px solid var(--cockpit-border)',
            background: 'var(--cockpit-card)',
          }}
        >
          <div style={{ display: 'flex', alignItems: 'center', gap: 10 }}>
            <Bot size={22} style={{ color: 'var(--primary-cyan)' }} />
            <div>
              <div className="modal-title" style={{ fontSize: 16 }}>
                Send data to an AI agent project
              </div>
              <div style={{ fontSize: 12, color: 'var(--text-muted)' }}>
                {isBatch
                  ? `Export ${papers.length} papers from the "${workspaceName}" library`
                  : `Export paper: "${papers[0].title}"`}
              </div>
            </div>
          </div>
          <button className="modal-close-btn" onClick={onClose} aria-label="Close dialog">
            <X size={18} />
          </button>
        </div>

        {/* Content Body */}
        <div style={{ flex: 1, overflowY: 'auto', padding: '20px 24px', display: 'flex', flexDirection: 'column', gap: 16 }}>
          {/* Target Agent Selection Cards */}
          <div>
            <div style={{ fontSize: 12.5, fontWeight: 700, color: 'var(--text-main)', marginBottom: 8 }}>
              1. Choose the target AI agent / system
            </div>
            <div
              style={{
                display: 'grid',
                gridTemplateColumns: 'repeat(auto-fill, minmax(230px, 1fr))',
                gap: 8,
              }}
            >
              {AGENT_OPTIONS.map((agent) => {
                const isSelected = selectedAgent === agent.id;
                return (
                  <button
                    key={agent.id}
                    type="button"
                    onClick={() => {
                      setSelectedAgent(agent.id);
                      if (agent.id === 'obsidian') setFormat('obsidian_note');
                      else if (format === 'obsidian_note') setFormat('prompt_pack');
                    }}
                    style={{
                      display: 'flex',
                      alignItems: 'flex-start',
                      gap: 10,
                      padding: '10px 12px',
                      borderRadius: 'var(--radius-md)',
                      border: `1px solid ${isSelected ? 'var(--primary-cyan)' : 'var(--cockpit-border)'}`,
                      background: isSelected ? 'var(--primary-cyan-bg)' : 'var(--cockpit-card)',
                      textAlign: 'left',
                      cursor: 'pointer',
                      transition: 'all 0.15s ease',
                    }}
                  >
                    <span style={{ fontSize: 18 }}>{agent.icon}</span>
                    <div style={{ minWidth: 0 }}>
                      <div
                        style={{
                          fontSize: 13,
                          fontWeight: 600,
                          color: isSelected ? 'var(--primary-cyan-hover)' : 'var(--text-main)',
                        }}
                      >
                        {agent.name}
                      </div>
                      <div style={{ fontSize: 11, color: 'var(--text-muted)', lineHeight: 1.3, marginTop: 2 }}>
                        {agent.desc}
                      </div>
                    </div>
                  </button>
                );
              })}
            </div>
          </div>

          {/* Export Format Selector */}
          <div>
            <div style={{ fontSize: 12.5, fontWeight: 700, color: 'var(--text-main)', marginBottom: 8 }}>
              2. Data format
            </div>
            <div className="segmented" style={{ alignSelf: 'flex-start' }} role="tablist">
              <button
                type="button"
                role="tab"
                aria-selected={format === 'prompt_pack'}
                className={`segmented-item ${format === 'prompt_pack' ? 'active' : ''}`}
                onClick={() => setFormat('prompt_pack')}
              >
                <Sparkles size={13} />
                <span>AI Prompt Pack (.md)</span>
              </button>
              <button
                type="button"
                role="tab"
                aria-selected={format === 'obsidian_note'}
                className={`segmented-item ${format === 'obsidian_note' ? 'active' : ''}`}
                onClick={() => setFormat('obsidian_note')}
              >
                <FileText size={13} />
                <span>Obsidian Note (.md)</span>
              </button>
              <button
                type="button"
                role="tab"
                aria-selected={format === 'bibtex'}
                className={`segmented-item ${format === 'bibtex' ? 'active' : ''}`}
                onClick={() => setFormat('bibtex')}
              >
                <FileCode size={13} />
                <span>BibTeX (.bib)</span>
              </button>
              <button
                type="button"
                role="tab"
                aria-selected={format === 'ris'}
                className={`segmented-item ${format === 'ris' ? 'active' : ''}`}
                onClick={() => setFormat('ris')}
              >
                <Bookmark size={13} />
                <span>Zotero RIS (.ris)</span>
              </button>
            </div>
          </div>

          {/* Live Preview Textarea */}
          <div>
            <div style={{ display: 'flex', justifyContent: 'space-between', alignItems: 'center', marginBottom: 6 }}>
              <span style={{ fontSize: 12, fontWeight: 600, color: 'var(--text-muted)' }}>
                Preview of the exported content ({generatedContent.split('\n').length} lines)
              </span>
              <span style={{ fontSize: 11, color: 'var(--text-dim)', fontFamily: 'var(--font-mono)' }}>
                {new Blob([generatedContent]).size} bytes
              </span>
            </div>
            <textarea
              readOnly
              className="field-input"
              style={{
                width: '100%',
                height: 180,
                fontFamily: 'var(--font-mono)',
                fontSize: 11.5,
                lineHeight: 1.5,
                background: 'var(--cockpit-bg)',
                whiteSpace: 'pre',
              }}
              value={generatedContent}
            />
          </div>
        </div>

        {/* Modal Footer */}
        <div
          className="modal-footer"
          style={{
            padding: '12px 24px',
            borderTop: '1px solid var(--cockpit-border)',
            display: 'flex',
            justifyContent: 'space-between',
            alignItems: 'center',
          }}
        >
          <div style={{ fontSize: 12, color: 'var(--text-muted)' }}>
            Paste it straight into the AI chat window, or drop the file into the agent’s project folder.
          </div>

          <div style={{ display: 'flex', gap: 8 }}>
            <button type="button" className="action-btn" onClick={onClose}>
              Close
            </button>
            <button
              type="button"
              className="action-btn"
              onClick={handleDownloadFile}
              title="Download the file to this computer"
            >
              {downloaded ? <Check size={14} color="var(--status-emerald)" /> : <Download size={14} />}
              <span>{downloaded ? 'Downloaded' : 'Download'}</span>
            </button>
            <button
              type="button"
              className="action-btn action-btn-primary"
              onClick={handleCopy}
              title="Copy the whole text"
            >
              {copied ? <Check size={14} /> : <Copy size={14} />}
              <span>{copied ? 'Prompt copied' : 'Copy prompt'}</span>
            </button>
          </div>
        </div>
      </div>
    </div>
  );
};
