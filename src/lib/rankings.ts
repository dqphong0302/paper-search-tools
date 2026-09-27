import { gatewayFetch } from './gateway';

/** Status of the SCImago journal ranking that powers the Q1–Q4 badges. */
export interface RankingStatus {
  journals: number;
  year?: number | null;
  imported_at?: number | null;
  download_url: string;
}

async function parse(res: Response): Promise<RankingStatus> {
  const json = await res.json().catch(() => null);
  if (!res.ok) throw new Error(json?.error || `The gateway returned status ${res.status}`);
  return json as RankingStatus;
}

export const rankingStatus = async () => parse(await gatewayFetch('/api/rankings'));

/** Downloads the current SCImago table through the gateway. */
export const updateRankings = async () => parse(await gatewayFetch('/api/rankings/update', { method: 'POST' }));

/** Imports a SCImago CSV the user downloaded themselves. */
export const importRankingsCsv = async (file: File) =>
  parse(await gatewayFetch('/api/rankings/import', {
    method: 'POST',
    headers: { 'Content-Type': 'application/json' },
    body: JSON.stringify({ csv: await file.text() }),
  }));
