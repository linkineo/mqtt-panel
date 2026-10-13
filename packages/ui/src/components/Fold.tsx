import type { ReactNode } from 'react';
import { useApp, type Fold } from '../store/app';

/**
 * A fold header (change set 4): chevron, title and a muted one-line summary of what is set inside (empty when
 * everything is at its default). Closed when the app starts; the state is kept while it runs.
 */
export function FoldHeader({ id, title, summary, disabled }: { id: Fold; title: string; summary?: ReactNode; disabled?: boolean }) {
  const open = useApp((s) => s.folds[id]);
  return (
    <button
      type="button"
      className="fold"
      aria-expanded={open}
      data-testid={`fold-${id}`}
      onClick={() => useApp.getState().toggleFold(id)}
      style={disabled ? { opacity: 0.45 } : undefined}
    >
      <span className="chev" aria-hidden="true">
        {open ? '▾' : '▸'}
      </span>
      <span className="ttl">{title}</span>
      {summary ? (
        <span className="sum" data-testid={`fold-${id}-summary`}>
          {summary}
        </span>
      ) : null}
    </button>
  );
}

export function useFold(id: Fold): boolean {
  return useApp((s) => s.folds[id]);
}
