import React, { useEffect, useMemo, useRef, useState } from 'react';
import { Bot, Check, ChevronLeft, ChevronRight, Copy, FileDown, Loader2, ScanText, Search, X, ZoomIn, ZoomOut } from 'lucide-react';
import type { PDFDocumentProxy } from 'pdfjs-dist';
import { gatewayFetch } from '../lib/gateway';
import { ExtractedPages, extractPages, loadPdf, OcrMode, saveFulltext, toMarkdown } from '../lib/pdfText';
import { Paper } from '../types';

interface PdfDocument {
  id: string;
  title: string;
  paper_id?: string;
  source?: string | null;
  year?: number | null;
}

/** The download record carries enough to label the Markdown when no full paper record is at hand. */
const paperFor = (document: PdfDocument): Paper => ({
  id: document.paper_id || document.id,
  title: document.title,
  authors: [],
  year: document.year ?? undefined,
  source: document.source || 'download',
  open_access: false,
});

export const PdfReaderModal: React.FC<{ document: PdfDocument | null; onClose: () => void }> = ({ document, onClose }) => {
  const canvasRef = useRef<HTMLCanvasElement>(null);
  const [pdf, setPdf] = useState<PDFDocumentProxy | null>(null);
  const [pageNumber, setPageNumber] = useState(1);
  const [scale, setScale] = useState(1.2);
  const [pageText, setPageText] = useState<string[]>([]);
  const [query, setQuery] = useState('');
  const [loading, setLoading] = useState(false);
  const [extracting, setExtracting] = useState(false);
  const [progress, setProgress] = useState('');
  const [extracted, setExtracted] = useState<ExtractedPages | null>(null);
  const [savedForAgents, setSavedForAgents] = useState(false);
  const [copied, setCopied] = useState(false);
  const [error, setError] = useState('');

  useEffect(() => {
    if (!document) return;
    let active = true;
    setLoading(true);
    setError('');
    setPdf(null);
    setPageText([]);
    setPageNumber(1);
    void (async () => {
      try {
        const response = await gatewayFetch(`/api/downloads/${encodeURIComponent(document.id)}/content`);
        if (!response.ok) throw new Error(`PDF could not be loaded (HTTP ${response.status})`);
        const loaded = await loadPdf(await response.arrayBuffer());
        if (active) setPdf(loaded);
      } catch (cause) {
        if (active) setError((cause as Error).message);
      } finally {
        if (active) setLoading(false);
      }
    })();
    return () => { active = false; };
  }, [document]);

  useEffect(() => {
    if (!pdf || !canvasRef.current) return;
    let cancelled = false;
    let renderTask: { cancel: () => void; promise: Promise<void> } | null = null;
    void pdf.getPage(pageNumber).then((page) => {
      if (cancelled || !canvasRef.current) return;
      const viewport = page.getViewport({ scale });
      const canvas = canvasRef.current;
      const ratio = window.devicePixelRatio || 1;
      canvas.width = Math.floor(viewport.width * ratio);
      canvas.height = Math.floor(viewport.height * ratio);
      canvas.style.width = `${Math.floor(viewport.width)}px`;
      canvas.style.height = `${Math.floor(viewport.height)}px`;
      const context = canvas.getContext('2d');
      if (!context) return;
      renderTask = page.render({ canvas, canvasContext: context, viewport, transform: ratio === 1 ? undefined : [ratio, 0, 0, ratio, 0, 0] });
      return renderTask.promise;
    }).catch((cause) => {
      if (!cancelled && (cause as Error).name !== 'RenderingCancelledException') setError((cause as Error).message);
    });
    return () => { cancelled = true; renderTask?.cancel(); };
  }, [pdf, pageNumber, scale]);

  const extract = async (ocr: OcrMode = 'auto') => {
    if (!pdf || extracting) return;
    setExtracting(true);
    setError('');
    setSavedForAgents(false);
    try {
      const result = await extractPages(pdf, {
        ocr,
        onProgress: ({ page, total, stage }) => setProgress(`${stage === 'ocr' ? 'OCR' : 'Reading'} page ${page}/${total}`),
      });
      setExtracted(result);
      setPageText(result.pages);
      if (!result.pages.some(Boolean)) setError('No text could be recognised in this PDF.');
    } catch (cause) {
      setError(`Text extraction failed: ${(cause as Error).message}`);
    } finally {
      setExtracting(false);
      setProgress('');
    }
  };

  const saveForAgents = async () => {
    if (!document || !extracted) return;
    try {
      await saveFulltext(paperFor(document), extractedMarkdown, extracted);
      setSavedForAgents(true);
    } catch (cause) {
      setError(`Could not save the Markdown for agents: ${(cause as Error).message}`);
    }
  };

  const matches = useMemo(() => {
    const needle = query.trim().toLocaleLowerCase();
    if (!needle || !pageText.length) return [];
    return pageText.flatMap((text, index) => text.toLocaleLowerCase().includes(needle) ? [index + 1] : []);
  }, [pageText, query]);

  const extractedMarkdown = useMemo(
    () => (document && extracted ? toMarkdown(paperFor(document), extracted) : ''),
    [document, extracted]
  );
  const saveExtraction = () => {
    const blob = new Blob([extractedMarkdown], { type: 'text/markdown;charset=utf-8' });
    const url = URL.createObjectURL(blob);
    const anchor = window.document.createElement('a');
    anchor.href = url;
    anchor.download = `${(document?.title ?? 'document').replace(/[^a-z0-9]+/gi, '-').replace(/^-|-$/g, '').slice(0, 70) || 'document'}-extracted.md`;
    anchor.click();
    URL.revokeObjectURL(url);
  };

  if (!document) return null;
  return (
    <div className="modal-overlay" role="dialog" aria-modal="true" aria-label={`PDF reader: ${document.title}`} onClick={onClose}>
      <div className="modal-dialog" onClick={(event) => event.stopPropagation()} style={{ width: '96vw', maxWidth: 1380, height: '94vh', padding: 0, overflow: 'hidden', display: 'flex', flexDirection: 'column' }}>
        <div className="modal-header" style={{ padding: '12px 16px', gap: 12 }}>
          <div style={{ minWidth: 0, flex: 1 }}><strong>PDF Reader & Extraction</strong><div style={{ overflow: 'hidden', textOverflow: 'ellipsis', whiteSpace: 'nowrap', fontSize: 12, color: 'var(--text-muted)' }}>{document.title}</div></div>
          <button className="modal-close-btn" onClick={onClose} aria-label="Close PDF reader"><X size={18} /></button>
        </div>
        <div style={{ padding: 10, borderBottom: '1px solid var(--cockpit-border)', display: 'flex', flexWrap: 'wrap', gap: 8, alignItems: 'center' }}>
          <button className="action-btn" disabled={!pdf || pageNumber <= 1} onClick={() => setPageNumber((page) => Math.max(1, page - 1))}><ChevronLeft size={14} /></button>
          <span style={{ fontSize: 12, minWidth: 90, textAlign: 'center' }}>Page {pageNumber} / {pdf?.numPages ?? '—'}</span>
          <button className="action-btn" disabled={!pdf || pageNumber >= pdf.numPages} onClick={() => setPageNumber((page) => Math.min(pdf?.numPages ?? page, page + 1))}><ChevronRight size={14} /></button>
          <button className="action-btn" disabled={!pdf || scale <= 0.6} onClick={() => setScale((value) => Math.max(0.6, value - 0.2))}><ZoomOut size={14} /></button>
          <span style={{ fontSize: 12 }}>{Math.round(scale * 100)}%</span>
          <button className="action-btn" disabled={!pdf || scale >= 2.4} onClick={() => setScale((value) => Math.min(2.4, value + 0.2))}><ZoomIn size={14} /></button>
          <button id="extract-pdf-text" className="action-btn action-btn-primary" disabled={!pdf || extracting} onClick={() => void extract('auto')} title="Reads the text layer; scanned pages are OCR'd automatically">{extracting ? <Loader2 size={14} className="animate-spin" /> : <FileDown size={14} />}<span>{extracting ? progress || 'Extracting…' : pageText.length ? 'Extract again' : 'Extract text'}</span></button>
          <button id="ocr-pdf-text" className="action-btn" disabled={!pdf || extracting} onClick={() => void extract('always')} title="Run OCR on every page (English + Vietnamese), for scans or broken text layers"><ScanText size={14} /><span>Force OCR</span></button>
          {pageText.length > 0 && <>
            {extracted && extracted.method !== 'text' && <span className="badge badge-cyan" title="Pages recognised by OCR">OCR {extracted.ocrPages}/{extracted.pages.length}</span>}
            <button id="save-fulltext-for-agents" className="action-btn" onClick={() => void saveForAgents()} title="Store this Markdown so MCP agents can read it with get_paper_fulltext">{savedForAgents ? <Check size={14} /> : <Bot size={14} />}<span>{savedForAgents ? 'Saved for agents' : 'Save for AI agents'}</span></button>
            <button className="action-btn" onClick={() => { void navigator.clipboard.writeText(extractedMarkdown).then(() => { setCopied(true); window.setTimeout(() => setCopied(false), 1800); }); }}>{copied ? <Check size={14} /> : <Copy size={14} />}<span>{copied ? 'Copied' : 'Copy text'}</span></button>
            <button className="action-btn" onClick={saveExtraction}><FileDown size={14} /><span>Save Markdown</span></button>
            <label style={{ marginLeft: 'auto', display: 'flex', gap: 6, alignItems: 'center' }}><Search size={14} /><input className="field-input" aria-label="Search extracted PDF text" placeholder="Search extracted text" value={query} onChange={(event) => setQuery(event.target.value)} style={{ width: 220 }} /></label>
          </>}
        </div>
        {error && <div className="alert alert-warning" role="alert" style={{ margin: 10 }}>{error}</div>}
        <div style={{ flex: 1, minHeight: 0, display: 'grid', gridTemplateColumns: pageText.length ? 'minmax(0, 2fr) minmax(300px, 1fr)' : '1fr' }}>
          <div style={{ overflow: 'auto', background: '#525659', padding: 20, textAlign: 'center' }}>
            {loading ? <div role="status" style={{ color: 'white' }}><Loader2 className="animate-spin" /> Loading PDF…</div> : <canvas ref={canvasRef} style={{ background: 'white', boxShadow: '0 3px 18px rgba(0,0,0,.4)', maxWidth: 'none' }} />}
          </div>
          {pageText.length > 0 && <aside style={{ overflow: 'auto', padding: 16, borderLeft: '1px solid var(--cockpit-border)' }}>
            <div style={{ fontSize: 12, color: 'var(--text-muted)', marginBottom: 10 }}>{query.trim() ? `${matches.length} matching page(s)` : `${pageText.length} pages extracted locally`}</div>
            {(query.trim() ? matches : pageText.map((_, index) => index + 1)).map((page) => <button key={page} type="button" className="cockpit-card" onClick={() => setPageNumber(page)} style={{ display: 'block', width: '100%', textAlign: 'left', padding: 10, marginBottom: 8, cursor: 'pointer', borderColor: page === pageNumber ? 'var(--primary-cyan)' : undefined }}><strong>Page {page}</strong><div style={{ fontSize: 12, color: 'var(--text-muted)', marginTop: 5 }}>{pageText[page - 1]?.slice(0, 280) || 'No extractable text'}</div></button>)}
          </aside>}
        </div>
      </div>
    </div>
  );
};
