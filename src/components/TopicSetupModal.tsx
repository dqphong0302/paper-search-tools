import React, { useMemo, useState } from 'react';
import { Check, Compass, Loader2 } from 'lucide-react';
import searchCatalog from '../lib/searchCatalog.json';

interface TopicSetupModalProps {
  onComplete: (preset: string, sources: string[]) => Promise<void>;
}

const TOPIC_IDS = ['vietnam', 'biomedical', 'ai_cs', 'stem_nature', 'social_humanities', 'evidence_review', 'patents_gov', 'global_regional'];

export const TopicSetupModal: React.FC<TopicSetupModalProps> = ({ onComplete }) => {
  const availableIds = useMemo(() => new Set(
    searchCatalog.sources.filter((source) => source.available !== false).map((source) => source.id),
  ), []);
  const topics = useMemo(() => TOPIC_IDS.map((id) => searchCatalog.presets.find((preset) => preset.id === id))
    .filter((preset): preset is NonNullable<typeof preset> => Boolean(preset))
    .map((preset) => ({ ...preset, sources: preset.sources.filter((id) => availableIds.has(id)) }))
    .filter((preset) => preset.sources.length > 0), [availableIds]);
  const [selected, setSelected] = useState('auto');
  const [saving, setSaving] = useState(false);
  const [error, setError] = useState('');

  const save = async () => {
    const preset = selected === 'auto'
      ? searchCatalog.presets.find((item) => item.id === 'auto')
      : topics.find((item) => item.id === selected);
    const sources = (preset?.sources ?? []).filter((id) => availableIds.has(id));
    setSaving(true);
    setError('');
    try {
      await onComplete(selected, sources);
    } catch (reason) {
      setError(`Could not save your choice: ${(reason as Error).message}`);
      setSaving(false);
    }
  };

  return <div className="modal-overlay" role="dialog" aria-modal="true" aria-labelledby="topic-setup-title">
    <div className="modal-dialog" style={{ maxWidth: 760, width: '92vw', maxHeight: '88vh', overflowY: 'auto' }}>
      <div className="modal-header">
        <div><div id="topic-setup-title" className="modal-title">Which field are you searching in?</div>
          <div style={{ fontSize: 12, color: 'var(--text-muted)', marginTop: 4 }}>ScholarGate will limit the search to this group’s active sources. You can change it again in Settings.</div>
        </div>
      </div>
      <div style={{ padding: 20, display: 'grid', gridTemplateColumns: 'repeat(auto-fit, minmax(220px, 1fr))', gap: 10 }}>
        <button id="topic-auto" type="button" className={`discipline-card ${selected === 'auto' ? 'active' : ''}`} onClick={() => setSelected('auto')} aria-pressed={selected === 'auto'}>
          <span><Compass size={15} /> Auto-detect</span><small>Good for cross-disciplinary searches</small>
        </button>
        {topics.map((topic) => <button id={`topic-${topic.id}`} key={topic.id} type="button" className={`discipline-card ${selected === topic.id ? 'active' : ''}`} onClick={() => setSelected(topic.id)} aria-pressed={selected === topic.id}>
          <span>{topic.label}</span><small>{topic.sources.length} sources available {selected === topic.id && <Check size={13} />}</small>
        </button>)}
      </div>
      {error && <div className="alert alert-warning" role="alert" style={{ margin: '0 20px' }}>{error}</div>}
      <div className="modal-footer"><button id="topic-setup-save" type="button" className="action-btn action-btn-primary" disabled={saving} onClick={() => void save()}>
        {saving && <Loader2 size={14} className="animate-spin" />}<span>{saving ? 'Saving…' : 'Start searching'}</span>
      </button></div>
    </div>
  </div>;
};
