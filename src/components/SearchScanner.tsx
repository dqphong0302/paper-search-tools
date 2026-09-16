import React, { useEffect, useState } from 'react';

interface SearchScannerProps {
  query: string;
  scope: string;
}

export const SearchScanner: React.FC<SearchScannerProps> = ({ query, scope }) => {
  const [elapsed, setElapsed] = useState(0);

  useEffect(() => {
    const start = Date.now();
    setElapsed(0);
    const timer = setInterval(() => setElapsed(Math.floor((Date.now() - start) / 1000)), 1000);
    return () => clearInterval(timer);
  }, [query, scope]);

  return (
    <div style={{ padding: '20px 0', color: 'var(--text-muted)', overflowWrap: 'anywhere' }}>
      <span role="status">Searching for “{query}”…</span>
      <span aria-hidden="true" style={{ marginLeft: 8, color: 'var(--text-dim)' }}>{elapsed}s</span>
    </div>
  );
};
