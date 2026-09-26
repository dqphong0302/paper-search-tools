import type { PDFDocumentProxy, PDFPageProxy } from 'pdfjs-dist';
import type { TextItem } from 'pdfjs-dist/types/src/display/api';
// The legacy build polyfills newer JS (e.g. Map.getOrInsertComputed) that
// older system WebViews lack; the modern build fails there on image-only pages.
import pdfWorkerUrl from 'pdfjs-dist/legacy/build/pdf.worker.min.mjs?url';
import ocrWorkerUrl from 'tesseract.js/dist/worker.min.js?url';
import ocrCoreSimdUrl from 'tesseract.js-core/tesseract-core-simd-lstm.wasm.js?url';
import ocrCoreUrl from 'tesseract.js-core/tesseract-core-lstm.wasm.js?url';
import { Paper } from '../types';
import { gatewayFetch, gatewayUrl } from './gateway';

/**
 * PDF → Markdown for AI agents.
 *
 * Each page is read from the PDF's own text layer. A page with (almost) no text
 * is a scan, so it is rendered and run through Tesseract OCR instead. The
 * result is Markdown with YAML front matter carrying the paper's metadata, so
 * an agent reading the file knows exactly what it is and how it was produced.
 */

export type OcrMode = 'auto' | 'always' | 'never';
export type ExtractionMethod = 'text' | 'ocr' | 'mixed';

export interface ExtractOptions {
  ocr?: OcrMode;
  /** Tesseract language codes joined with "+", e.g. "eng+vie". */
  languages?: string;
  onProgress?: (progress: { page: number; total: number; stage: 'text' | 'ocr' }) => void;
}

export interface ExtractedPages {
  pages: string[];
  method: ExtractionMethod;
  ocrPages: number;
}

/** A page with fewer characters than this in its text layer is treated as a scan. */
const MIN_TEXT_CHARS = 40;

export const DEFAULT_OCR_LANGUAGES = 'eng+vie';

export async function loadPdf(data: ArrayBuffer): Promise<PDFDocumentProxy> {
  const pdfjs = await import('pdfjs-dist/legacy/build/pdf.mjs');
  pdfjs.GlobalWorkerOptions.workerSrc = pdfWorkerUrl;
  return pdfjs.getDocument({ data }).promise;
}

/**
 * Rebuilds lines and paragraphs from positioned text items: a new line when
 * the baseline moves, a paragraph break when the gap is clearly larger than
 * the usual line spacing.
 */
export function textFromItems(items: Pick<TextItem, 'str' | 'hasEOL' | 'transform' | 'height'>[]): string {
  const lines: { y: number; height: number; text: string }[] = [];
  for (const item of items) {
    const y = item.transform[5];
    const last = lines[lines.length - 1];
    if (!last || Math.abs(last.y - y) > Math.max(2, (item.height || last.height) * 0.5)) {
      lines.push({ y, height: item.height || 10, text: item.str });
    } else {
      last.text += item.str;
    }
    if (item.hasEOL && item.str) lines.push({ y: y - 0.01, height: item.height || 10, text: '' });
  }
  const content = lines.filter((line) => line.text.trim());
  const gaps = content.slice(1).map((line, index) => Math.abs(content[index].y - line.y)).sort((a, b) => a - b);
  const typicalGap = gaps.length ? gaps[Math.floor(gaps.length / 2)] : 0;

  let out = '';
  content.forEach((line, index) => {
    const text = line.text.replace(/\s+/g, ' ').trim();
    if (index === 0) {
      out = text;
      return;
    }
    const gap = Math.abs(content[index - 1].y - line.y);
    if (typicalGap && gap > typicalGap * 1.6) out += `\n\n${text}`;
    else out += `\n${text}`;
  });
  return tidyText(out);
}

/**
 * Joins wrapped lines into paragraphs and repairs words hyphenated across a
 * line break, which otherwise split terms an agent would search for.
 */
export function tidyText(text: string): string {
  return text
    .replace(/\r/g, '')
    .split(/\n{2,}/)
    .map((paragraph) =>
      paragraph
        .split('\n')
        .map((line) => line.trim())
        .filter(Boolean)
        .reduce((acc, line) => {
          if (!acc) return line;
          if (/[a-zà-ỹ]-$/i.test(acc) && /^[a-zà-ỹ]/.test(line)) return acc.slice(0, -1) + line;
          return `${acc} ${line}`;
        }, '')
    )
    .filter(Boolean)
    .join('\n\n');
}

async function pageText(page: PDFPageProxy): Promise<string> {
  const content = await page.getTextContent();
  return textFromItems(content.items.filter((item): item is TextItem => 'str' in item));
}

// ---- OCR --------------------------------------------------------------------

type OcrWorker = Awaited<ReturnType<typeof import('tesseract.js')['createWorker']>>;
let ocrWorker: { languages: string; worker: Promise<OcrWorker> } | null = null;

const supportsWasmSimd = () => {
  try {
    // The smallest module using a v128 instruction; valid only with SIMD support.
    return WebAssembly.validate(new Uint8Array([
      0, 97, 115, 109, 1, 0, 0, 0, 1, 5, 1, 96, 0, 1, 123, 3, 2, 1, 0, 10, 10, 1, 8, 0, 65, 0, 253, 15, 253, 98, 11,
    ]));
  } catch {
    return false;
  }
};

/** One shared Tesseract worker; models are served (and cached) by the local gateway. */
function getOcrWorker(languages: string): Promise<OcrWorker> {
  if (ocrWorker?.languages === languages) return ocrWorker.worker;
  const previous = ocrWorker;
  const worker = import('tesseract.js').then(async ({ createWorker, OEM }) => {
    if (previous) await (await previous.worker).terminate().catch(() => undefined);
    return createWorker(languages.split('+'), OEM.LSTM_ONLY, {
      workerPath: ocrWorkerUrl,
      corePath: supportsWasmSimd() ? ocrCoreSimdUrl : ocrCoreUrl,
      langPath: gatewayUrl('/api/ocr/lang'),
      workerBlobURL: false,
      gzip: true,
    });
  });
  ocrWorker = { languages, worker };
  worker.catch(() => { if (ocrWorker?.worker === worker) ocrWorker = null; });
  return worker;
}

export async function terminateOcr(): Promise<void> {
  const current = ocrWorker;
  ocrWorker = null;
  if (current) await (await current.worker).terminate().catch(() => undefined);
}

async function ocrPage(page: PDFPageProxy, languages: string): Promise<string> {
  // ~200 dpi is where Tesseract's accuracy levels off for body text.
  const viewport = page.getViewport({ scale: 200 / 72 });
  const canvas = document.createElement('canvas');
  canvas.width = Math.ceil(viewport.width);
  canvas.height = Math.ceil(viewport.height);
  const context = canvas.getContext('2d');
  if (!context) throw new Error('Canvas is not available for OCR');
  await page.render({ canvas, canvasContext: context, viewport }).promise;
  const worker = await getOcrWorker(languages);
  const { data } = await worker.recognize(canvas);
  canvas.width = 0;
  canvas.height = 0;
  return tidyText(data.text);
}

// ---- Extraction ---------------------------------------------------------------

export async function extractPages(pdf: PDFDocumentProxy, options: ExtractOptions = {}): Promise<ExtractedPages> {
  const mode = options.ocr ?? 'auto';
  const languages = options.languages || DEFAULT_OCR_LANGUAGES;
  const pages: string[] = [];
  let ocrPages = 0;
  for (let number = 1; number <= pdf.numPages; number += 1) {
    const page = await pdf.getPage(number);
    options.onProgress?.({ page: number, total: pdf.numPages, stage: 'text' });
    let text = mode === 'always' ? '' : await pageText(page);
    if (mode !== 'never' && text.replace(/\s/g, '').length < MIN_TEXT_CHARS) {
      options.onProgress?.({ page: number, total: pdf.numPages, stage: 'ocr' });
      const recognised = await ocrPage(page, languages);
      if (recognised.replace(/\s/g, '').length > text.replace(/\s/g, '').length) {
        text = recognised;
        ocrPages += 1;
      }
    }
    pages.push(text);
    page.cleanup();
  }
  const method: ExtractionMethod = ocrPages === 0 ? 'text' : ocrPages === pages.length ? 'ocr' : 'mixed';
  return { pages, method, ocrPages };
}

const yaml = (value: string) => JSON.stringify(value);

/** Markdown with YAML front matter; one "## Page n" section per page. */
export function toMarkdown(paper: Paper, extracted: ExtractedPages): string {
  const front = [
    '---',
    `title: ${yaml(paper.title)}`,
    paper.authors.length ? `authors: [${paper.authors.map(yaml).join(', ')}]` : '',
    paper.year ? `year: ${paper.year}` : '',
    paper.venue ? `journal: ${yaml(paper.venue)}` : '',
    paper.quartile ? `quartile: ${paper.quartile}` : '',
    paper.biblio?.volume ? `volume: ${yaml(paper.biblio.volume)}` : '',
    paper.biblio?.issue ? `issue: ${yaml(paper.biblio.issue)}` : '',
    paper.biblio?.pages ? `pages: ${yaml(paper.biblio.pages)}` : '',
    paper.doi ? `doi: ${yaml(paper.doi)}` : '',
    paper.source_url ? `url: ${yaml(paper.source_url)}` : '',
    paper.biblio?.topic ? `topic: ${yaml(paper.biblio.topic)}` : '',
    `paper_id: ${yaml(paper.id)}`,
    `extraction: ${extracted.method}`,
    `pages_total: ${extracted.pages.length}`,
    extracted.ocrPages ? `pages_ocr: ${extracted.ocrPages}` : '',
    '---',
  ].filter(Boolean);
  const body = extracted.pages
    .map((text, index) => `## Page ${index + 1}\n\n${text.trim() || '*No text recognised on this page.*'}`)
    .join('\n\n');
  const abstract = paper.abstract?.trim() ? `\n## Abstract (from metadata)\n\n${paper.abstract.trim()}\n` : '';
  return `${front.join('\n')}\n\n# ${paper.title}\n${abstract}\n${body}\n`;
}

/** Stores a paper's Markdown with the gateway so MCP agents can read it. */
export async function saveFulltext(paper: Paper, markdown: string, extracted: ExtractedPages): Promise<void> {
  const res = await gatewayFetch(`/api/fulltext?paper_id=${encodeURIComponent(paper.id)}`, {
    method: 'PUT',
    headers: { 'Content-Type': 'application/json' },
    body: JSON.stringify({ title: paper.title, markdown, method: extracted.method, pages: extracted.pages.length }),
  });
  if (!res.ok) {
    const json = await res.json().catch(() => null);
    throw new Error(json?.error || `The gateway returned status ${res.status}`);
  }
}

/** Downloaded PDF → Markdown → stored for agents. Returns the Markdown. */
export async function convertDownloadToMarkdown(
  downloadId: string,
  paper: Paper,
  options: ExtractOptions = {}
): Promise<{ markdown: string; extracted: ExtractedPages }> {
  const res = await gatewayFetch(`/api/downloads/${encodeURIComponent(downloadId)}/content`);
  if (!res.ok) throw new Error(`Could not open the downloaded PDF (HTTP ${res.status})`);
  const pdf = await loadPdf(await res.arrayBuffer());
  try {
    const extracted = await extractPages(pdf, options);
    const markdown = toMarkdown(paper, extracted);
    await saveFulltext(paper, markdown, extracted);
    return { markdown, extracted };
  } finally {
    await pdf.loadingTask.destroy();
  }
}
