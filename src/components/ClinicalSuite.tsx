import React, { useState, useMemo, useEffect } from 'react';
import { GitBranch, BarChart3, Copy, Check, Plus, Trash2, AlertTriangle, RotateCcw } from 'lucide-react';

/* ==========================================================================
   Tool 1 — PRISMA 2020 flow
   Downstream boxes are derived from the ones above so the diagram can never
   show arithmetic that does not add up.
   ========================================================================== */

interface PrismaState {
  sourceLabel: string;
  identified: number;
  duplicates: number;
  excludedScreen: number;
  excludedFulltext: number;
  notRetrieved: number;
}

// Starts empty on purpose: pre-filled counts would look like a real systematic
// review flow a researcher could screenshot or export by mistake.
const PRISMA_DEFAULT: PrismaState = {
  sourceLabel: 'International Databases & Local Journals',
  identified: 0,
  duplicates: 0,
  excludedScreen: 0,
  excludedFulltext: 0,
  notRetrieved: 0,
};

const num = (v: string) => Math.max(0, Math.floor(Number(v) || 0));

const PrismaTool: React.FC = () => {
  const [s, setS] = useState<PrismaState>(() => {
    try {
      const raw = localStorage.getItem('sg_prisma');
      return raw ? { ...PRISMA_DEFAULT, ...JSON.parse(raw) } : PRISMA_DEFAULT;
    } catch {
      return PRISMA_DEFAULT;
    }
  });
  const [copied, setCopied] = useState(false);

  useEffect(() => {
    localStorage.setItem('sg_prisma', JSON.stringify(s));
  }, [s]);

  const set = (patch: Partial<PrismaState>) => setS((prev) => ({ ...prev, ...patch }));

  // Derived counts — always internally consistent.
  const screened = s.identified - s.duplicates;
  const sought = screened - s.excludedScreen;
  const assessed = sought - s.notRetrieved;
  const included = assessed - s.excludedFulltext;

  const errors: string[] = [];
  if (screened < 0) errors.push('Duplicate count exceeds total identified records.');
  if (sought < 0) errors.push('Records excluded during title/abstract screening exceed screened count.');
  if (assessed < 0) errors.push('Reports not retrieved exceed reports sought for retrieval.');
  if (included < 0) errors.push('Full-text reports excluded exceed reports assessed for eligibility.');
  const valid = errors.length === 0 && s.identified > 0;

  const mermaid = `flowchart TD
  A["Records identified from ${s.sourceLabel}<br/>(n = ${s.identified})"] --> B["Records after duplicates removed<br/>(n = ${screened})"]
  A --- A1["Duplicate records removed<br/>(n = ${s.duplicates})"]
  B --> C["Records screened (title/abstract)<br/>(n = ${screened})"]
  C --- C1["Records excluded<br/>(n = ${s.excludedScreen})"]
  C --> D["Reports sought for retrieval<br/>(n = ${sought})"]
  D --- D1["Reports not retrieved<br/>(n = ${s.notRetrieved})"]
  D --> E["Reports assessed for eligibility<br/>(n = ${assessed})"]
  E --- E1["Reports excluded<br/>(n = ${s.excludedFulltext})"]
  E --> F["Studies included in review<br/>(n = ${included})"]`;

  const copyMermaid = () => {
    navigator.clipboard.writeText(mermaid);
    setCopied(true);
    setTimeout(() => setCopied(false), 2000);
  };

  const inputs: { key: keyof PrismaState; label: string }[] = [
    { key: 'identified', label: 'Total records identified (databases)' },
    { key: 'duplicates', label: 'Duplicate records removed' },
    { key: 'excludedScreen', label: 'Excluded during title/abstract screening' },
    { key: 'notRetrieved', label: 'Reports not retrieved (full text)' },
    { key: 'excludedFulltext', label: 'Full-text reports excluded after appraisal' },
  ];

  const stages = [
    {
      title: '1. Identification',
      main: `Identified via ${s.sourceLabel}: ${s.identified} records`,
      note: `Duplicates removed: ${s.duplicates} records`,
      bg: 'var(--primary-cyan-bg)',
      border: 'var(--primary-cyan-border)',
      color: 'var(--primary-cyan)',
    },
    {
      title: '2. Screening',
      main: `Screened (title/abstract): ${screened} records`,
      note: `Excluded: ${s.excludedScreen} records`,
      bg: 'var(--status-violet-bg)',
      border: '#ddd6fe',
      color: 'var(--status-violet)',
    },
    {
      title: '3. Eligibility',
      main: `Assessed for eligibility: ${assessed} reports`,
      note: `Not retrieved: ${s.notRetrieved} • Excluded full-text: ${s.excludedFulltext}`,
      bg: 'var(--status-amber-bg)',
      border: 'var(--status-amber-border)',
      color: 'var(--status-amber)',
    },
    {
      title: '4. Included',
      main: `${included} studies included in review`,
      note: 'Synthesized in systematic review / meta-analysis',
      bg: 'var(--status-emerald-bg)',
      border: 'var(--status-emerald-border)',
      color: 'var(--status-emerald)',
      strong: true,
    },
  ];

  return (
    <div style={{ display: 'grid', gridTemplateColumns: 'repeat(auto-fit, minmax(310px, 1fr))', gap: 20, alignItems: 'start' }}>
      <div className="cockpit-card" style={{ padding: 18, minWidth: 0 }}>
        <div className="cockpit-card-title" style={{ marginBottom: 4 }}>
          <GitBranch size={15} className="text-accent" />
          <span>PRISMA 2020 Flow Parameters</span>
        </div>
        <div className="cockpit-card-subtitle mb-14">
          Downstream counts are calculated automatically to maintain mathematical consistency.
        </div>

        <div className="u-stack">
          <div>
            <label className="field-label">Data sources label (displayed on chart)</label>
            <input
              type="text"
              className="field-input"
              value={s.sourceLabel}
              onChange={(e) => set({ sourceLabel: e.target.value })}
            />
          </div>

          {inputs.map((f) => (
            <div key={f.key}>
              <label className="field-label">{f.label}</label>
              <input
                type="number"
                min={0}
                className="field-input"
                value={s[f.key] as number}
                onChange={(e) => set({ [f.key]: num(e.target.value) } as Partial<PrismaState>)}
              />
            </div>
          ))}

          {s.identified === 0 && (
            <div className="alert alert-info m-0">
              No data entered yet. Input actual record counts from your systematic review workflow to render the diagram.
            </div>
          )}

          <div
            style={{
              display: 'flex',
              justifyContent: 'space-between',
              alignItems: 'center',
              padding: '10px 12px',
              background: 'var(--status-emerald-bg)',
              border: '1px solid var(--status-emerald-border)',
              borderRadius: 'var(--radius-sm)',
              fontSize: 13,
            }}
          >
            <span style={{ color: 'var(--text-muted)' }}>Studies included in review</span>
            <b style={{ color: 'var(--status-emerald)', fontSize: 16 }}>{valid ? included : '—'}</b>
          </div>

          {/* `valid` is also false for an untouched form; that case is covered by
              the info notice above, so only render this when there is a message. */}
          {errors.length > 0 && (
            <div className="alert alert-danger">
              <AlertTriangle size={15} className="icon-inline" />
              <div>{errors.join(' ')}</div>
            </div>
          )}

          <div className="u-wrap">
            <button className="action-btn" onClick={copyMermaid} disabled={!valid}>
              {copied ? <Check size={14} color="var(--status-emerald)" /> : <Copy size={14} />}
              <span>{copied ? 'Copied Mermaid code' : 'Copy Mermaid code'}</span>
            </button>
            <button className="action-btn" onClick={() => setS(PRISMA_DEFAULT)}>
              <RotateCcw size={14} />
              <span>Reset</span>
            </button>
          </div>
        </div>
      </div>

      <div className="cockpit-card" style={{ padding: 20, minWidth: 0 }}>
        <div
          style={{
            fontSize: 11,
            color: 'var(--text-dim)',
            fontFamily: 'var(--font-mono)',
            textAlign: 'center',
            marginBottom: 18,
          }}
        >
          PRISMA 2020 FLOWCHART — PREVIEW
        </div>

        <div style={{ display: 'flex', flexDirection: 'column', alignItems: 'center', gap: 10 }}>
          {stages.map((st, i) => (
            <React.Fragment key={st.title}>
              <div
                style={{
                  width: '100%',
                  maxWidth: 420,
                  padding: '12px 16px',
                  background: st.bg,
                  border: `1px solid ${st.border}`,
                  borderRadius: 'var(--radius-md)',
                  textAlign: 'center',
                }}
              >
                <div
                  style={{
                    fontSize: 11,
                    fontWeight: 700,
                    color: st.color,
                    textTransform: 'uppercase',
                    letterSpacing: '0.02em',
                  }}
                >
                  {st.title}
                </div>
                <div
                  style={{
                    fontSize: st.strong ? 16 : 13,
                    fontWeight: st.strong ? 700 : 400,
                    color: 'var(--text-main)',
                    marginTop: 4,
                  }}
                >
                  {valid ? st.main : '—'}
                </div>
                <div style={{ fontSize: 11, color: 'var(--text-muted)', marginTop: 2 }}>{st.note}</div>
              </div>
              {i < stages.length - 1 && (
                <div style={{ color: 'var(--text-dim)', fontSize: 16, lineHeight: 1 }}>↓</div>
              )}
            </React.Fragment>
          ))}
        </div>
      </div>
    </div>
  );
};

/* ==========================================================================
   Tool 2 — Random-effects meta-analysis (DerSimonian–Laird)
   All numbers below are computed from the rows the user enters.
   ========================================================================== */

interface Study {
  id: string;
  name: string;
  or: string;
  lower: string;
  upper: string;
}

const EXAMPLE_STUDIES: Study[] = [
  { id: 's1', name: 'Nguyen et al. (2024)', or: '0.62', lower: '0.45', upper: '0.86' },
  { id: 's2', name: 'Tran & Le (2023)', or: '0.74', lower: '0.52', upper: '1.05' },
  { id: 's3', name: 'Smith et al. (2023)', or: '0.55', lower: '0.38', upper: '0.79' },
  { id: 's4', name: 'Kumar et al. (2022)', or: '0.81', lower: '0.61', upper: '1.08' },
];

const emptyStudy = (): Study => ({
  id: Math.random().toString(36).slice(2, 9),
  name: '',
  or: '',
  lower: '',
  upper: '',
});

// Abramowitz & Stegun 26.2.17 normal CDF approximation.
const normalCdf = (z: number) => {
  const t = 1 / (1 + 0.2316419 * Math.abs(z));
  const d = 0.3989422804014327 * Math.exp((-z * z) / 2);
  const p =
    d * t * (0.319381530 + t * (-0.356563782 + t * (1.781477937 + t * (-1.821255978 + t * 1.330274429))));
  return z > 0 ? 1 - p : p;
};

const MetaTool: React.FC = () => {
  const [studies, setStudies] = useState<Study[]>(() => {
    try {
      const raw = localStorage.getItem('sg_meta_studies');
      return raw ? JSON.parse(raw) : [emptyStudy()];
    } catch {
      return [emptyStudy()];
    }
  });

  useEffect(() => {
    localStorage.setItem('sg_meta_studies', JSON.stringify(studies));
  }, [studies]);

  const update = (id: string, patch: Partial<Study>) =>
    setStudies((prev) => prev.map((s) => (s.id === id ? { ...s, ...patch } : s)));

  const analysis = useMemo(() => {
    const parsed = studies
      .map((s) => ({
        raw: s,
        or: Number(s.or),
        lower: Number(s.lower),
        upper: Number(s.upper),
      }))
      .filter(
        (s) =>
          Number.isFinite(s.or) &&
          Number.isFinite(s.lower) &&
          Number.isFinite(s.upper) &&
          s.lower > 0 &&
          s.or > 0 &&
          s.upper > 0 &&
          s.lower <= s.or &&
          s.or <= s.upper &&
          s.lower < s.upper
      );

    if (parsed.length === 0) return null;

    const rows = parsed.map((s) => {
      const lnOr = Math.log(s.or);
      const se = (Math.log(s.upper) - Math.log(s.lower)) / (2 * 1.959964);
      return { ...s, lnOr, se, vi: se * se };
    });

    // Fixed-effect weights for Q / tau²
    const wFixed = rows.map((r) => 1 / r.vi);
    const sumW = wFixed.reduce((a, b) => a + b, 0);
    const sumWy = rows.reduce((acc, r, i) => acc + wFixed[i] * r.lnOr, 0);
    const sumWy2 = rows.reduce((acc, r, i) => acc + wFixed[i] * r.lnOr * r.lnOr, 0);
    const sumW2 = wFixed.reduce((a, b) => a + b * b, 0);

    const q = sumWy2 - (sumWy * sumWy) / sumW;
    const df = rows.length - 1;
    const c = sumW - sumW2 / sumW;
    const tau2 = df > 0 && c > 0 ? Math.max(0, (q - df) / c) : 0;
    const i2 = df > 0 && q > df ? ((q - df) / q) * 100 : 0;

    // Random-effects weights
    const wRandom = rows.map((r) => 1 / (r.vi + tau2));
    const sumWr = wRandom.reduce((a, b) => a + b, 0);
    const pooledLn = rows.reduce((acc, r, i) => acc + wRandom[i] * r.lnOr, 0) / sumWr;
    const sePooled = Math.sqrt(1 / sumWr);
    const z = pooledLn / sePooled;
    const p = 2 * (1 - normalCdf(Math.abs(z)));

    const withWeights = rows.map((r, i) => ({ ...r, weight: (wRandom[i] / sumWr) * 100 }));

    const pooled = {
      or: Math.exp(pooledLn),
      lower: Math.exp(pooledLn - 1.959964 * sePooled),
      upper: Math.exp(pooledLn + 1.959964 * sePooled),
    };

    // Log-scale plot domain
    const minVal = Math.min(...rows.map((r) => r.lower), pooled.lower);
    const maxVal = Math.max(...rows.map((r) => r.upper), pooled.upper);
    const lo = Math.log(Math.min(minVal * 0.8, 0.9));
    const hi = Math.log(Math.max(maxVal * 1.2, 1.1));
    const pos = (v: number) => ((Math.log(v) - lo) / (hi - lo)) * 100;

    const ticks = [0.1, 0.25, 0.5, 1, 2, 4, 10].filter(
      (t) => Math.log(t) >= lo && Math.log(t) <= hi
    );

    return { rows: withWeights, pooled, q, df, tau2, i2, p, pos, ticks, k: rows.length };
  }, [studies]);

  const incompleteCount = studies.length - (analysis?.k ?? 0);

  return (
    <div className="cockpit-card" style={{ padding: 20 }}>
      <div className="page-header" style={{ marginBottom: 16 }}>
        <div>
          <div className="page-title" style={{ fontSize: 15 }}>
            <BarChart3 size={16} className="text-accent" />
            <span>Meta-Analysis — Random Effects Model (DerSimonian–Laird)</span>
          </div>
          <div className="page-subtitle">
            Enter Odds Ratio and 95% confidence intervals for each study; pooled effect, I², and weights
            are calculated directly from your input data.
          </div>
        </div>
        <div className="u-flex">
          <button className="action-btn" onClick={() => setStudies(EXAMPLE_STUDIES)}>
            <RotateCcw size={14} />
            <span>Load Example Data</span>
          </button>
          <button className="action-btn" onClick={() => setStudies((p) => [...p, emptyStudy()])}>
            <Plus size={14} />
            <span>Add Study</span>
          </button>
        </div>
      </div>

      {/* Input rows */}
      <div style={{ display: 'flex', flexDirection: 'column', gap: 8, marginBottom: 18 }}>
        <div
          style={{
            display: 'grid',
            gridTemplateColumns: 'minmax(160px, 2fr) repeat(3, minmax(72px, 1fr)) 34px',
            gap: 8,
            fontSize: 11,
            color: 'var(--text-dim)',
            fontWeight: 600,
          }}
        >
          <span>STUDY</span>
          <span>OR</span>
          <span>LOWER CI</span>
          <span>UPPER CI</span>
          <span />
        </div>

        {studies.map((s) => {
          const bad =
            (s.or || s.lower || s.upper) &&
            !(
              Number(s.lower) > 0 &&
              Number(s.lower) <= Number(s.or) &&
              Number(s.or) <= Number(s.upper)
            );
          return (
            <div
              key={s.id}
              style={{
                display: 'grid',
                gridTemplateColumns: 'minmax(160px, 2fr) repeat(3, minmax(72px, 1fr)) 34px',
                gap: 8,
                alignItems: 'center',
              }}
            >
              <input
                className="field-input"
                placeholder="Author (Year)"
                value={s.name}
                onChange={(e) => update(s.id, { name: e.target.value })}
              />
              <input
                className="field-input"
                type="number"
                step="0.01"
                placeholder="0.62"
                aria-invalid={!!bad}
                value={s.or}
                onChange={(e) => update(s.id, { or: e.target.value })}
              />
              <input
                className="field-input"
                type="number"
                step="0.01"
                placeholder="0.45"
                aria-invalid={!!bad}
                value={s.lower}
                onChange={(e) => update(s.id, { lower: e.target.value })}
              />
              <input
                className="field-input"
                type="number"
                step="0.01"
                placeholder="0.86"
                aria-invalid={!!bad}
                value={s.upper}
                onChange={(e) => update(s.id, { upper: e.target.value })}
              />
              <button
                className="action-btn action-btn-danger"
                title="Remove study row"
                onClick={() => setStudies((p) => p.filter((x) => x.id !== s.id))}
                style={{ padding: '7px 8px', justifyContent: 'center' }}
              >
                <Trash2 size={13} />
              </button>
            </div>
          );
        })}

        {incompleteCount > 0 && (
          <div className="alert alert-warning">
            <AlertTriangle size={15} className="icon-inline" />
            <div>
              {incompleteCount} rows are incomplete or invalid and skipped. Each study requires
              0 &lt; Lower CI ≤ OR ≤ Upper CI.
            </div>
          </div>
        )}
      </div>

      {/* Results */}
      {!analysis ? (
        <div className="empty-state">
          <div className="empty-state-icon">
            <BarChart3 size={24} />
          </div>
          <div className="empty-state-title">No study data to analyze</div>
          <div className="empty-state-text">
            Enter at least one study with OR and 95% confidence intervals, or click "Load Example Data"
            to see the interactive forest plot.
          </div>
        </div>
      ) : (
        <>
          <div
            style={{
              display: 'flex',
              flexWrap: 'wrap',
              gap: 10,
              marginBottom: 14,
              fontSize: 12.5,
            }}
          >
            <div
              style={{
                padding: '8px 14px',
                borderRadius: 'var(--radius-sm)',
                background: 'var(--status-emerald-bg)',
                border: '1px solid var(--status-emerald-border)',
                color: 'var(--status-emerald)',
                fontWeight: 600,
              }}
            >
              Pooled OR {analysis.pooled.or.toFixed(2)} [95% CI {analysis.pooled.lower.toFixed(2)} –{' '}
              {analysis.pooled.upper.toFixed(2)}]
            </div>
            <div
              style={{
                padding: '8px 14px',
                borderRadius: 'var(--radius-sm)',
                background: '#f8fafc',
                border: '1px solid var(--cockpit-border)',
                color: 'var(--text-muted)',
                fontFamily: 'var(--font-mono)',
                fontSize: 11.5,
              }}
            >
              k = {analysis.k} • I² = {analysis.i2.toFixed(1)}% • τ² = {analysis.tau2.toFixed(4)} • Q ={' '}
              {analysis.q.toFixed(2)} (df {analysis.df}) • p ={' '}
              {analysis.p < 0.001 ? '<0.001' : analysis.p.toFixed(3)}
            </div>
          </div>

          <div style={{ overflowX: 'auto' }}>
            <table style={{ width: '100%', borderCollapse: 'collapse', fontSize: 12 }}>
              <thead>
                <tr style={{ color: 'var(--text-dim)', textAlign: 'left' }}>
                  <th style={{ padding: '8px 12px', borderBottom: '1px solid var(--cockpit-border)' }}>
                    STUDY
                  </th>
                  <th style={{ padding: '8px 12px', borderBottom: '1px solid var(--cockpit-border)' }}>
                    WEIGHT
                  </th>
                  <th style={{ padding: '8px 12px', borderBottom: '1px solid var(--cockpit-border)' }}>
                    OR [95% CI]
                  </th>
                  <th
                    style={{
                      padding: '8px 12px',
                      minWidth: 300,
                      borderBottom: '1px solid var(--cockpit-border)',
                    }}
                  >
                    EFFECT PLOT (LOG SCALE)
                  </th>
                </tr>
              </thead>
              <tbody>
                {analysis.rows.map((r, idx) => (
                  <tr key={r.raw.id} style={{ borderBottom: '1px solid var(--cockpit-border-subtle)' }}>
                    <td style={{ padding: '10px 12px', fontWeight: 500 }}>
                      {r.raw.name || `Study ${idx + 1}`}
                    </td>
                    <td style={{ padding: '10px 12px', color: 'var(--text-muted)', fontFamily: 'var(--font-mono)' }}>
                      {r.weight.toFixed(1)}%
                    </td>
                    <td style={{ padding: '10px 12px', fontFamily: 'var(--font-mono)' }}>
                      {r.or.toFixed(2)} [{r.lower.toFixed(2)}, {r.upper.toFixed(2)}]
                    </td>
                    <td style={{ padding: '10px 12px' }}>
                      <div className="forest-track">
                        <div className="forest-null-line" style={{ left: `${analysis.pos(1)}%` }} />
                        <div
                          className="forest-ci"
                          style={{
                            left: `${analysis.pos(r.lower)}%`,
                            width: `${Math.max(analysis.pos(r.upper) - analysis.pos(r.lower), 0.5)}%`,
                          }}
                        />
                        <div
                          className="forest-point"
                          style={{
                            left: `${analysis.pos(r.or)}%`,
                            width: Math.max(6, Math.min(14, 4 + r.weight / 4)),
                            height: Math.max(6, Math.min(14, 4 + r.weight / 4)),
                          }}
                        />
                      </div>
                    </td>
                  </tr>
                ))}

                <tr style={{ background: '#f8fafc', fontWeight: 600 }}>
                  <td style={{ padding: '12px', color: 'var(--status-emerald)' }}>
                    Pooled Effect (Random Effects)
                  </td>
                  <td style={{ padding: '12px', fontFamily: 'var(--font-mono)' }}>100%</td>
                  <td style={{ padding: '12px', fontFamily: 'var(--font-mono)', color: 'var(--status-emerald)' }}>
                    {analysis.pooled.or.toFixed(2)} [{analysis.pooled.lower.toFixed(2)},{' '}
                    {analysis.pooled.upper.toFixed(2)}]
                  </td>
                  <td style={{ padding: '12px' }}>
                    <div className="forest-track">
                      <div className="forest-null-line" style={{ left: `${analysis.pos(1)}%` }} />
                      <div
                        className="forest-diamond"
                        style={{
                          left: `${analysis.pos(analysis.pooled.lower)}%`,
                          width: `${Math.max(
                            analysis.pos(analysis.pooled.upper) - analysis.pos(analysis.pooled.lower),
                            2
                          )}%`,
                        }}
                      />
                    </div>
                  </td>
                </tr>

                {/* Axis */}
                <tr>
                  <td colSpan={3} />
                  <td style={{ padding: '2px 12px 10px 12px' }}>
                    <div style={{ position: 'relative', height: 16 }}>
                      {analysis.ticks.map((t) => (
                        <span
                          key={t}
                          style={{
                            position: 'absolute',
                            left: `${analysis.pos(t)}%`,
                            transform: 'translateX(-50%)',
                            fontSize: 10,
                            fontFamily: 'var(--font-mono)',
                            color: t === 1 ? 'var(--status-rose)' : 'var(--text-dim)',
                          }}
                        >
                          {t}
                        </span>
                      ))}
                    </div>
                    <div style={{ display: 'flex', justifyContent: 'space-between', fontSize: 10, color: 'var(--text-dim)' }}>
                      <span>← Favors Intervention</span>
                      <span>Favors Control →</span>
                    </div>
                  </td>
                </tr>
              </tbody>
            </table>
          </div>
        </>
      )}
    </div>
  );
};

/* ========================================================================== */

export const ClinicalSuite: React.FC = () => {
  const [activeTool, setActiveTool] = useState<'prisma' | 'meta'>('prisma');

  return (
    <div className="page-container">
      <div className="segmented u-self-start" role="tablist">
        <button
          role="tab"
          aria-selected={activeTool === 'prisma'}
          className={`segmented-item ${activeTool === 'prisma' ? 'active' : ''}`}
          onClick={() => setActiveTool('prisma')}
          style={{ padding: '7px 14px', fontSize: 13 }}
        >
          <GitBranch size={15} />
          <span>PRISMA 2020 Flow Diagram</span>
        </button>
        <button
          role="tab"
          aria-selected={activeTool === 'meta'}
          className={`segmented-item ${activeTool === 'meta' ? 'active' : ''}`}
          onClick={() => setActiveTool('meta')}
          style={{ padding: '7px 14px', fontSize: 13 }}
        >
          <BarChart3 size={15} />
          <span>Meta-Analysis (Forest Plot)</span>
        </button>
      </div>

      {activeTool === 'prisma' ? <PrismaTool /> : <MetaTool />}
    </div>
  );
};
