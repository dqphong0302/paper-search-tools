// @vitest-environment jsdom
import { describe, it, expect, vi, beforeEach, afterEach } from 'vitest';
import { act } from 'react';
import { createRoot, Root } from 'react-dom/client';
import { SourceLimiterModal } from './components/SourceLimiterModal';
import { FulltextViewerModal } from './components/FulltextViewerModal';
import { AiAgentExportModal } from './components/AiAgentExportModal';
import { buildMeshQuery, originalPaperUrl, suggestedKeywords } from './components/Explorer';
import { Paper } from './types';

// @ts-expect-error React 19 act environment flag
globalThis.IS_REACT_ACT_ENVIRONMENT = true;

let container: HTMLDivElement | null = null;
let root: Root | null = null;

const samplePaper: Paper = {
  id: '10.1016/j.cell.2024.01.001',
  title: 'CRISPR Cas9 Gene Editing Breakthroughs in Oncology',
  authors: ['Nguyen Van A', 'John Smith', 'Tran Thi B'],
  year: 2024,
  venue: 'Cell',
  abstract: 'This study investigates targeted CRISPR Cas9 delivery mechanisms in solid tumors.',
  doi: '10.1016/j.cell.2024.01.001',
  source: 'PubMed / MEDLINE',
  source_url: 'https://pubmed.ncbi.nlm.nih.gov/12345678/',
  pdf_url: 'https://example.com/paper.pdf',
  citations: 42,
  open_access: true,
  quartile: 'Q1',
};

beforeEach(() => {
  container = document.createElement('div');
  document.body.appendChild(container);
  root = createRoot(container);
  Object.assign(navigator, {
    clipboard: {
      writeText: vi.fn().mockResolvedValue(undefined),
    },
  });
});

afterEach(async () => {
  if (root) {
    await act(async () => {
      root?.unmount();
    });
  }
  if (container) {
    container.remove();
    container = null;
  }
  vi.restoreAllMocks();
});

describe('New Search & AI Features', () => {
  describe('MeSH query builder', () => {
    it('groups synonyms, fields, Boolean operators and exclusions', () => {
      expect(
        buildMeshQuery(
          [
            { terms: 'heart failure\ncardiac failure', field: 'mh' },
            { terms: 'drug therapy', field: 'tiab' },
          ],
          'AND',
          'animals',
          true
        )
      ).toBe('("heart failure"[mh] OR "cardiac failure"[mh]) AND "drug therapy"[tiab] NOT animals');
    });

    it('supports MeSH no-explode and OR between concept groups', () => {
      expect(
        buildMeshQuery(
          [
            { terms: 'neoplasms', field: 'mh' },
            { terms: 'immunotherapy', field: 'majr' },
          ],
          'OR',
          '',
          false
        )
      ).toBe('neoplasms[mh:noexp] OR immunotherapy[majr]');
    });
  });

  describe('paper preview metadata', () => {
    it('keeps a traceable original link and derives concise preview keywords', () => {
      expect(originalPaperUrl(samplePaper)).toBe(samplePaper.source_url);
      expect(suggestedKeywords(samplePaper)).toEqual(
        expect.arrayContaining(['crispr', 'cas9', 'gene', 'editing'])
      );
    });

    it('falls back from source URL to DOI and then PDF', () => {
      expect(originalPaperUrl({ ...samplePaper, source_url: undefined })).toBe(
        `https://doi.org/${samplePaper.doi}`
      );
      expect(originalPaperUrl({ ...samplePaper, source_url: undefined, doi: undefined })).toBe(
        samplePaper.pdf_url
      );
    });
  });

  describe('SourceLimiterModal', () => {
    it('renders all sources and supports search filtering', async () => {
      const applySpy = vi.fn();
      const closeSpy = vi.fn();

      await act(async () => {
        root?.render(
          <SourceLimiterModal
            isOpen={true}
            onClose={closeSpy}
            activeScope="biomedical"
            selectedSources={['pubmed']}
            onApplySources={applySpy}
          />
        );
      });

      expect(container?.textContent).toContain('Academic source limiter');
      expect(container?.textContent).toContain('PubMed / MEDLINE');

      // Filter by search input
      const searchInput = container?.querySelector<HTMLInputElement>('input[placeholder*="Find a source"]');
      expect(searchInput).not.toBeNull();

      await act(async () => {
        if (searchInput) {
          searchInput.value = 'VJOL';
          searchInput.dispatchEvent(new Event('input', { bubbles: true }));
        }
      });

      expect(container?.textContent).toContain('VJOL');

      // Click Apply
      const applyButton = container?.querySelector<HTMLButtonElement>('button.action-btn-primary');
      expect(applyButton).not.toBeNull();

      await act(async () => {
        applyButton?.click();
      });

      expect(applySpy).toHaveBeenCalledWith(['pubmed']);
      expect(closeSpy).toHaveBeenCalled();
    });
  });

  describe('FulltextViewerModal', () => {
    it('displays abstract, metadata, and allows copying citation and downloading PDF', async () => {
      const closeSpy = vi.fn();
      const saveSpy = vi.fn();
      const favSpy = vi.fn();
      const downloadSpy = vi.fn();

      const clipboardSpy = vi.spyOn(navigator.clipboard, 'writeText');

      await act(async () => {
        root?.render(
          <FulltextViewerModal
            isOpen={true}
            onClose={closeSpy}
            paper={samplePaper}
            isSaved={false}
            isFavorite={false}
            onSavePaper={saveSpy}
            onToggleFavorite={favSpy}
            onDownloadPdf={downloadSpy}
          />
        );
      });

      expect(container?.textContent).toContain('CRISPR Cas9 Gene Editing');
      expect(container?.textContent).toContain('Nguyen Van A, John Smith, Tran Thi B');
      expect(container?.textContent).toContain('This study investigates targeted CRISPR');
      expect(container?.textContent).toContain('OPEN ACCESS');

      // Click Copy APA
      const copyApaBtn = [...(container?.querySelectorAll('button') ?? [])].find((btn) =>
        btn.textContent?.includes('Copy APA')
      );
      expect(copyApaBtn).toBeDefined();

      await act(async () => {
        copyApaBtn?.click();
      });

      expect(clipboardSpy).toHaveBeenCalled();

      // Click Download PDF
      const downloadBtn = [...(container?.querySelectorAll('button') ?? [])].find((btn) =>
        btn.textContent?.includes('Download full PDF')
      );
      expect(downloadBtn).toBeDefined();

      await act(async () => {
        downloadBtn?.click();
      });

      expect(downloadSpy).toHaveBeenCalledWith(samplePaper);
    });
  });

  describe('AiAgentExportModal', () => {
    it('generates markdown prompt packs for Google Antigravity, Claude, Codex, OpenCode, and Obsidian', async () => {
      const closeSpy = vi.fn();
      const clipboardSpy = vi.spyOn(navigator.clipboard, 'writeText');

      await act(async () => {
        root?.render(
          <AiAgentExportModal
            isOpen={true}
            onClose={closeSpy}
            papers={[samplePaper]}
            workspaceName="Test Workspace"
          />
        );
      });

      expect(container?.textContent).toContain('Send data to an AI agent project');
      expect(container?.textContent).toContain('Google Antigravity');
      expect(container?.textContent).toContain('Claude Desktop');
      expect(container?.textContent).toContain('OpenCode');
      expect(container?.textContent).toContain('Obsidian Local Vault');

      // Textarea preview contains paper title
      const textarea = container?.querySelector('textarea');
      expect(textarea?.value).toContain('CRISPR Cas9 Gene Editing Breakthroughs');
      expect(textarea?.value).toContain('Google Antigravity');

      // Click Copy Prompt
      const copyBtn = [...(container?.querySelectorAll('button') ?? [])].find((btn) =>
        btn.textContent?.includes('Copy prompt')
      );
      expect(copyBtn).toBeDefined();

      await act(async () => {
        copyBtn?.click();
      });

      expect(clipboardSpy).toHaveBeenCalled();
    });
  });
});
