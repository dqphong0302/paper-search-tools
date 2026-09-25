import { Paper } from '../types';
import { gatewayFetch } from './gateway';

/**
 * True when the gateway can try to fetch a full text: a direct PDF link, or an
 * open-access DOI whose repository copies it can look up.
 */
export function canDownloadPdf(paper: Paper): boolean {
  return Boolean(paper.pdf_url || (paper.open_access && paper.doi));
}

/** Asks the gateway to download a paper's PDF; the DOI lets it fall back to other OA copies. */
export async function requestPdfDownload(
  paper: Paper,
  workspaceId?: string
): Promise<{ ok: true } | { ok: false; error: string }> {
  try {
    const res = await gatewayFetch('/api/download', {
      method: 'POST',
      headers: { 'Content-Type': 'application/json' },
      body: JSON.stringify({
        paper_id: paper.id,
        title: paper.title,
        pdf_url: paper.pdf_url || '',
        doi: paper.doi,
        source: paper.source,
        year: paper.year,
        workspace_id: workspaceId,
      }),
    });
    const json = await res.json().catch(() => null);
    if (!res.ok || !json || json.success === false) {
      return { ok: false, error: json?.error || `The gateway returned status ${res.status}` };
    }
    return { ok: true };
  } catch (err) {
    return { ok: false, error: `PDF download failed: ${(err as Error).message}` };
  }
}
