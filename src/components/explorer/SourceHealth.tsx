import React from 'react';
import { AlertTriangle, CheckCircle2, PauseCircle, Settings, XCircle } from 'lucide-react';
import { SourceStatus } from '../../types';
import { sourceMessage } from './searchConfig';

/**
 * Per-source outcome of a search. Sources that need credentials or are cooling
 * down are reported separately so they never read as live failures.
 */
export const SourceHealth: React.FC<{ sources: SourceStatus[] }> = ({ sources }) => {
  const queried = sources.filter((s) => s.queried);
  const needsSetup = queried.filter((s) => !s.ok && s.needs_setup);
  const cooling = queried.filter((s) => !s.ok && s.cooling_down);
  const failed = queried.filter((s) => !s.ok && !s.needs_setup && !s.cooling_down);
  const reachable = queried.length - needsSetup.length - cooling.length;

  return (
    <div className="u-stack u-gap-10">
      {failed.length > 0 && (
        <div className="alert alert-warning">
          <AlertTriangle size={16} />
          <div>
            <div className="alert-title">{failed.length}/{reachable} sources unresponsive — results may be partial</div>
            <div>{failed.map((s) => sourceMessage(s.name, s.error)).join(' • ')}</div>
          </div>
        </div>
      )}

      {cooling.length > 0 && (
        <div className="alert">
          <PauseCircle size={16} />
          <div>
            <div className="alert-title">
              {cooling.length} {cooling.length === 1 ? 'source is' : 'sources are'} paused after repeated failures
            </div>
            <div>{cooling.map((s) => sourceMessage(s.name, s.error, 'cooling down')).join(' • ')}</div>
          </div>
        </div>
      )}

      {needsSetup.length > 0 && (
        <div className="alert">
          <Settings size={16} />
          <div>
            <div className="alert-title">
              {needsSetup.length} {needsSetup.length === 1 ? 'source needs' : 'sources need'} setup — skipped, not failed
            </div>
            <div>{needsSetup.map((s) => s.name).join(' • ')} — add the key or sign in from Settings.</div>
          </div>
        </div>
      )}

      <div className="u-row u-gap-6">
        <span className="eyebrow-label eyebrow-xs">
          ACTIVE SOURCES ({queried.filter((s) => s.count > 0).length}/{reachable}):
        </span>
        {sources
          .filter((s) => s.queried && (s.count > 0 || (!s.ok && !s.needs_setup && !s.cooling_down)))
          .map((s) => {
            const Icon = s.ok ? CheckCircle2 : XCircle;
            return (
              <span key={s.id} className={`source-chip ${s.ok ? 'ok' : 'fail'}`} title={s.ok ? `${s.count} results` : s.error || 'Unknown error'}>
                <Icon size={12} />
                <span>{s.name}</span>
                {s.ok && <b>{s.count}</b>}
              </span>
            );
          })}
      </div>
    </div>
  );
};
