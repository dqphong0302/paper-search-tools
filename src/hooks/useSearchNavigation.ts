import { useCallback, useState } from 'react';

/**
 * The shell omnibox owns the draft; a submitted query becomes the Explorer's
 * query and bumps `nonce` so re-submitting the same text re-runs the search.
 */
export function useSearchNavigation(onSubmit: () => void) {
  const [draft, setDraft] = useState('');
  const [query, setQuery] = useState('');
  const [nonce, setNonce] = useState(0);

  const submit = useCallback((text: string) => {
    const next = text.trim();
    if (!next) return;
    setQuery(next);
    setDraft(next);
    setNonce((value) => value + 1);
    onSubmit();
  }, [onSubmit]);

  const clear = useCallback(() => {
    setQuery('');
    setDraft('');
    setNonce(0);
  }, []);

  return { draft, setDraft, query, nonce, submit, clear };
}
