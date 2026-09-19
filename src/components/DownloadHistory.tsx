import React, { useState, useEffect } from 'react';
import {
  DownloadCloud,
  FileText,
  FolderOpen,
  Trash2,
  RefreshCw,
  Clock,
  HardDrive,
  Copy,
  Check,
  Search,
  CheckCircle2,
  AlertTriangle
} from 'lucide-react';
import { gatewayFetch } from '../lib/gateway';
import { PdfReaderModal } from './PdfReaderModal';

export interface DownloadRecord {
  id: string;
  paper_id: string;
  title: string;
  pdf_url: string;
  local_path: string;
  file_size_bytes: number;
  source?: string;
  year?: number;
  downloaded_at: number;
  workspace_id?: string;
}

interface DownloadHistoryProps {
  port: number;
  workspaceId?: string;
}

export const DownloadHistory: React.FC<DownloadHistoryProps> = ({ port, workspaceId }) => {
  const [downloads, setDownloads] = useState<DownloadRecord[]>([]);
  const [loading, setLoading] = useState(false);
  const [filterText, setFilterText] = useState('');
  const [copiedId, setCopiedId] = useState<string | null>(null);
  const [openSuccessId, setOpenSuccessId] = useState<string | null>(null);
  const [error, setError] = useState<string | null>(null);
  const [readerDocument, setReaderDocument] = useState<DownloadRecord | null>(null);

  const fetchDownloads = async () => {
    setLoading(true);
    setError(null);
    try {
      const suffix = workspaceId ? `?workspace_id=${encodeURIComponent(workspaceId)}` : '';
      const res = await gatewayFetch(`/api/history/downloads${suffix}`);
      if (!res.ok) throw new Error(`Server returned status code ${res.status}`);
      setDownloads(await res.json());
    } catch (e) {
      setError(
        e instanceof TypeError
          ? `Unable to connect to local gateway server at 127.0.0.1:${port}.`
          : `Failed to load downloads: ${(e as Error).message}`
      );
    } finally {
      setLoading(false);
    }
  };

  useEffect(() => {
    fetchDownloads();
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [port, workspaceId]);

  const handleOpenFile = async (id: string, path: string) => {
    setError(null);
    try {
      const res = await gatewayFetch('/api/open-file', {
        method: 'POST',
        headers: { 'Content-Type': 'application/json' },
        body: JSON.stringify({ path }),
      });
      if (!res.ok) throw new Error(`Server returned status code ${res.status}`);
      const data = await res.json().catch(() => null);
      if (data?.success !== true) throw new Error(data?.error || 'Unable to open file');
      setOpenSuccessId(id);
      setTimeout(() => setOpenSuccessId(null), 2000);
    } catch (e) {
      setError(`Unable to open file (${path}). The file may have been moved or deleted.`);
    }
  };

  const handleDelete = async (id: string) => {
    if (!window.confirm('Remove this record from download history? (The local PDF file on disk will be kept)'))
      return;
    setError(null);
    try {
      const res = await gatewayFetch(`/api/history/downloads/${id}`, {
        method: 'DELETE',
      });
      if (!res.ok) throw new Error(`Server returned status code ${res.status}`);
      setDownloads((prev) => prev.filter((d) => d.id !== id));
    } catch (e) {
      setError(`Failed to delete download record: ${(e as Error).message}`);
    }
  };

  // Read the real folder off the records rather than printing a fixed path:
  // the directory is configurable, and an install that predates the rename
  // keeps using its old folder, so a hardcoded default would be wrong twice.
  const storageFolder = React.useMemo(() => {
    const path = downloads[0]?.local_path;
    if (!path) return null;
    const cut = Math.max(path.lastIndexOf('/'), path.lastIndexOf('\\'));
    return cut > 0 ? path.slice(0, cut) : null;
  }, [downloads]);

  const handleClearAll = async () => {
    // Wording matters here: this forgets records, it does not touch the user's
    // files. The single-record delete already makes the same promise.
    if (!window.confirm(
      `Clear all ${downloads.length} download records? The PDF files on disk are kept. This cannot be undone.`
    )) return;
    setError(null);
    try {
      const suffix = workspaceId ? `?workspace_id=${encodeURIComponent(workspaceId)}` : '';
      const res = await gatewayFetch(`/api/history/downloads${suffix}`, { method: 'DELETE' });
      if (!res.ok) throw new Error(`Server returned status code ${res.status}`);
      setDownloads([]);
    } catch (e) {
      setError(`Failed to clear download history: ${(e as Error).message}`);
    }
  };

  const copyPath = (id: string, path: string) => {
    navigator.clipboard.writeText(path).then(() => {
      setCopiedId(id);
      setTimeout(() => setCopiedId(null), 2000);
    }).catch(() => setError('Unable to copy file path to clipboard.'));
  };

  const formatBytes = (bytes: number) => {
    if (!bytes || bytes === 0) return '0 B';
    const k = 1024;
    const sizes = ['B', 'KB', 'MB', 'GB'];
    const i = Math.floor(Math.log(bytes) / Math.log(k));
    return parseFloat((bytes / Math.pow(k, i)).toFixed(1)) + ' ' + sizes[i];
  };

  const formatTime = (timestamp: number) => {
    if (!timestamp) return 'Recent';
    const date = new Date(timestamp * 1000);
    return date.toLocaleTimeString([], { hour: '2-digit', minute: '2-digit' }) + ' • ' + date.toLocaleDateString();
  };

  const totalBytes = downloads.reduce((acc, cur) => acc + (cur.file_size_bytes || 0), 0);

  const filtered = downloads.filter((d) =>
    d.title.toLowerCase().includes(filterText.toLowerCase()) ||
    (d.source && d.source.toLowerCase().includes(filterText.toLowerCase()))
  );

  return (
    <div className="page-container">
      {/* Header */}
      <div
        style={{
          display: 'flex',
          alignItems: 'center',
          justifyContent: 'space-between',
          borderBottom: '1px solid var(--cockpit-border)',
          paddingBottom: 16,
        }}
      >
        <div>
          <h2 style={{ fontSize: 18, fontWeight: 700, color: 'var(--text-main)', display: 'flex', alignItems: 'center', gap: 8 }}>
            <DownloadCloud size={18} style={{ color: 'var(--status-emerald)' }} />
            <span>Downloaded PDFs</span>
            <span className="cockpit-badge badge-emerald">{downloads.length} Documents</span>
          </h2>
          <p style={{ fontSize: 12, color: 'var(--text-muted)', marginTop: 2 }}>
            {storageFolder ? (
              <>
                Stored locally in{' '}
                <code style={{ color: 'var(--primary-cyan)', fontFamily: 'var(--font-mono)' }}>
                  {storageFolder}
                </code>
              </>
            ) : (
              'Downloaded PDFs are stored in the folder set under Settings → Gateway & Security.'
            )}
          </p>
        </div>

        <div style={{ display: 'flex', alignItems: 'center', gap: 12 }}>
          {/* Storage counter */}
          <div
            style={{
              display: 'flex',
              alignItems: 'center',
              gap: 8,
              padding: '6px 12px',
              background: 'var(--cockpit-card)',
              border: '1px solid var(--cockpit-border)',
              borderRadius: 'var(--radius-sm)',
              fontSize: 12,
              fontFamily: 'var(--font-mono)',
              color: 'var(--text-main)',
            }}
          >
            <HardDrive size={14} style={{ color: 'var(--primary-cyan)' }} />
            <span>Total storage: <b>{formatBytes(totalBytes)}</b></span>
          </div>

          <button className="action-btn" onClick={fetchDownloads} title="Refresh downloads list">
            <RefreshCw size={14} className={loading ? 'animate-spin' : ''} />
            <span>Sync</span>
          </button>

          {downloads.length > 0 && (
            <button
              id="clear-download-history"
              className="action-btn action-btn-danger"
              onClick={handleClearAll}
              title="Forget every download record (the PDF files on disk are kept)"
            >
              <Trash2 size={14} />
              <span>Clear All</span>
            </button>
          )}
        </div>
      </div>

      {error && (
        <div className="alert alert-danger">
          <AlertTriangle size={16} style={{ flexShrink: 0, marginTop: 1 }} />
          <div>{error}</div>
        </div>
      )}

      {/* Filter Bar */}
      {downloads.length > 0 && (
        <div style={{ display: 'flex', gap: 10, alignItems: 'center' }}>
          <div
            style={{
              flex: 1,
              display: 'flex',
              alignItems: 'center',
              background: 'var(--cockpit-card)',
              border: '1px solid var(--cockpit-border)',
              borderRadius: 'var(--radius-sm)',
              padding: '6px 12px',
              gap: 8,
            }}
          >
            <Search size={15} style={{ color: 'var(--text-dim)' }} />
            <input
              id="filter-downloads"
              aria-label="Filter downloaded PDFs"
              type="text"
              placeholder="Filter downloads by title or source..."
              value={filterText}
              onChange={(e) => setFilterText(e.target.value)}
              style={{
                background: 'transparent',
                border: 'none',
                color: 'var(--text-main)',
                fontSize: 13,
                outline: 'none',
                width: '100%',
              }}
            />
          </div>
        </div>
      )}

      {/* List / Empty State */}
      {filtered.length === 0 ? (
        <div className="empty-state">
          <div
            className="empty-state-icon"
            style={{
              background: 'var(--status-emerald-bg)',
              borderColor: 'var(--status-emerald-border)',
              color: 'var(--status-emerald)',
            }}
          >
            <DownloadCloud size={24} />
          </div>
          <div className="empty-state-title">
            {filterText ? 'No downloaded PDFs match your filter' : 'No downloaded papers yet'}
          </div>
          <div className="empty-state-text">
            {filterText
              ? 'Try another keyword or clear the search filter.'
              : 'Click "Download Fulltext PDF" on any paper in Search to save it locally for offline study.'}
          </div>
        </div>
      ) : (
        <div style={{ display: 'flex', flexDirection: 'column', gap: 12 }}>
          {filtered.map((item) => (
            <div
              key={item.id}
              className="cockpit-card"
              style={{
                display: 'flex',
                alignItems: 'flex-start',
                justifyContent: 'space-between',
                padding: '16px 18px',
                gap: 16,
              }}
            >
              <div style={{ display: 'flex', gap: 14, flex: 1, minWidth: 0 }}>
                <div
                  style={{
                    width: 38,
                    height: 38,
                    borderRadius: 'var(--radius-sm)',
                    background: 'rgba(244, 63, 94, 0.12)',
                    color: '#f43f5e',
                    display: 'flex',
                    alignItems: 'center',
                    justifyContent: 'center',
                    flexShrink: 0,
                  }}
                >
                  <FileText size={20} />
                </div>

                <div style={{ display: 'flex', flexDirection: 'column', gap: 6, flex: 1, minWidth: 0 }}>
                  <div
                    style={{
                      fontSize: 14,
                      fontWeight: 600,
                      color: 'var(--text-main)',
                      lineHeight: 1.4,
                    }}
                  >
                    {item.title}
                  </div>

                  <div style={{ display: 'flex', flexWrap: 'wrap', alignItems: 'center', gap: 8, fontSize: 11 }}>
                    {item.source && (
                      <span className="cockpit-badge badge-cyan">{item.source}</span>
                    )}
                    {item.year && (
                      <span className="cockpit-badge badge-emerald">Year {item.year}</span>
                    )}
                    <span style={{ fontFamily: 'var(--font-mono)', color: 'var(--text-muted)' }}>
                      Size: {formatBytes(item.file_size_bytes)}
                    </span>
                    <span style={{ color: 'var(--text-dim)' }}>•</span>
                    <span style={{ display: 'flex', alignItems: 'center', gap: 4, color: 'var(--text-dim)', fontFamily: 'var(--font-mono)' }}>
                      <Clock size={11} />
                      {formatTime(item.downloaded_at)}
                    </span>
                  </div>

                  {/* Local Path snippet */}
                  <div className="code-chip" style={{ marginTop: 2 }}>
                    <span style={{ overflow: 'hidden', textOverflow: 'ellipsis', whiteSpace: 'nowrap' }}>
                      {item.local_path}
                    </span>
                    <button
                      onClick={() => copyPath(item.id, item.local_path)}
                      title="Copy file path"
                      style={{
                        background: 'transparent',
                        border: 'none',
                        color: 'var(--text-muted)',
                        cursor: 'pointer',
                        padding: 2,
                        marginLeft: 8,
                        display: 'flex',
                        alignItems: 'center',
                      }}
                    >
                      {copiedId === item.id ? <Check size={12} color="var(--status-emerald)" /> : <Copy size={12} />}
                    </button>
                  </div>
                </div>
              </div>

              {/* Actions */}
              <div style={{ display: 'flex', alignItems: 'center', gap: 8, flexShrink: 0 }}>
                <button
                  id={`read-pdf-${item.id}`}
                  className="action-btn action-btn-primary"
                  onClick={() => setReaderDocument(item)}
                  title="Read, search and extract this PDF inside ScholarGate"
                  style={{ padding: '7px 14px' }}
                >
                  <FileText size={14} />
                  <span>Read & Extract</span>
                </button>
                <button
                  className="action-btn"
                  onClick={() => handleOpenFile(item.id, item.local_path)}
                  title="Open PDF file with default system application"
                  style={{ padding: '7px 14px' }}
                >
                  {openSuccessId === item.id ? (
                    <>
                      <CheckCircle2 size={14} color="var(--status-emerald)" />
                      <span>Opening...</span>
                    </>
                  ) : (
                    <>
                      <FolderOpen size={14} />
                      <span>Open PDF</span>
                    </>
                  )}
                </button>

                <button
                  onClick={() => handleDelete(item.id)}
                  title="Remove from history"
                  aria-label="Remove from history"
                  className="action-btn action-btn-danger"
                  style={{ padding: '7px 10px' }}
                >
                  <Trash2 size={14} />
                </button>
              </div>
            </div>
          ))}
        </div>
      )}
      <PdfReaderModal document={readerDocument} onClose={() => setReaderDocument(null)} />
    </div>
  );
};
