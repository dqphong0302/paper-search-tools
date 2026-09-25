import React from 'react';
import { Bot } from 'lucide-react';

interface SelectionBarProps {
  shownCount: number;
  selectedCount: number;
  allVisibleSelected: boolean;
  visibleSelectedCount: number;
  onToggleAll: () => void;
  onClear: () => void;
  onExportRis: () => void;
  onExportBibtex: () => void;
  onSendToAgent: () => void;
}

/** Tick results to export them as references without saving them first. */
export const SelectionBar: React.FC<SelectionBarProps> = (props) => (
  <div className="selection-bar sticky-toolbar">
    <label className="check-label">
      <input
        id="select-all-results"
        type="checkbox"
        checked={props.allVisibleSelected}
        ref={(node) => {
          if (node) node.indeterminate = props.visibleSelectedCount > 0 && !props.allVisibleSelected;
        }}
        onChange={props.onToggleAll}
        aria-label="Select all shown results"
      />
      <span>Select all ({props.shownCount})</span>
    </label>

    {props.selectedCount > 0 ? (
      <>
        <span className="selection-count">{props.selectedCount} selected</span>
        <button id="export-selected-ris" type="button" className="action-btn" onClick={props.onExportRis} title="RIS is the shared import format of Zotero, EndNote and Mendeley">
          Export .RIS — Zotero / EndNote
        </button>
        <button id="export-selected-bibtex" type="button" className="action-btn" onClick={props.onExportBibtex}>
          Export BibTeX
        </button>
        <button type="button" className="action-btn" onClick={props.onSendToAgent}>
          <Bot size={13} /> Send to AI agent
        </button>
        <button type="button" className="action-btn" onClick={props.onClear}>
          Clear
        </button>
      </>
    ) : (
      <span className="text-dim">Tick results to export them as references, without saving them first.</span>
    )}
  </div>
);
