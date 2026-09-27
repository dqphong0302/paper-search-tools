import React from 'react';

/** A paper's journal quartile (SCImago SJR best quartile), colour-coded Q1→Q4. */
export const QuartileBadge: React.FC<{ quartile?: string | null; style?: React.CSSProperties }> = ({ quartile, style }) => {
  const q = quartile?.toUpperCase();
  if (!q || !/^Q[1-4]$/.test(q)) return null;
  return (
    <span className={`badge badge-quartile ${q.toLowerCase()}`} style={style} title={`Journal quartile ${q} (SCImago Journal Rank, best category)`}>
      {q}
    </span>
  );
};
