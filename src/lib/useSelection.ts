import { useCallback, useMemo, useState } from 'react';

/**
 * Checkbox selection over a list of ids, shared by the Library and the search
 * results so both behave the same way.
 *
 * `visibleIds` is the currently filtered list rather than everything, so
 * "select all" means "everything I can see" — the usual expectation when a
 * filter is applied. Ids that scroll out of the filter keep their selection,
 * which is what lets someone gather papers across several searches before
 * exporting them in one go.
 */
export interface Selection {
  selected: Set<string>;
  isSelected: (id: string) => boolean;
  toggle: (id: string) => void;
  /** Selects every visible id, or clears them when all are already selected. */
  toggleAllVisible: () => void;
  clear: () => void;
  /** True when every visible id is selected and there is at least one. */
  allVisibleSelected: boolean;
  /** How many of the visible ids are selected. */
  visibleSelectedCount: number;
  count: number;
}

export function useSelection(visibleIds: string[]): Selection {
  const [selected, setSelected] = useState<Set<string>>(new Set());

  const toggle = useCallback((id: string) => {
    setSelected((prev) => {
      const next = new Set(prev);
      if (!next.delete(id)) next.add(id);
      return next;
    });
  }, []);

  const clear = useCallback(() => setSelected(new Set()), []);

  const visibleSelectedCount = useMemo(
    () => visibleIds.reduce((total, id) => total + (selected.has(id) ? 1 : 0), 0),
    [visibleIds, selected]
  );
  const allVisibleSelected = visibleIds.length > 0 && visibleSelectedCount === visibleIds.length;

  const toggleAllVisible = useCallback(() => {
    setSelected((prev) => {
      const next = new Set(prev);
      const everyVisibleSelected =
        visibleIds.length > 0 && visibleIds.every((id) => next.has(id));
      for (const id of visibleIds) {
        if (everyVisibleSelected) next.delete(id);
        else next.add(id);
      }
      return next;
    });
  }, [visibleIds]);

  const isSelected = useCallback((id: string) => selected.has(id), [selected]);

  return {
    selected,
    isSelected,
    toggle,
    toggleAllVisible,
    clear,
    allVisibleSelected,
    visibleSelectedCount,
    count: selected.size,
  };
}
