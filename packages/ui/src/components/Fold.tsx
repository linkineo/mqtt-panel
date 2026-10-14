import type { ReactNode } from 'react';
import { useApp, type Fold } from '../store/app';

/**
 * A fold header (change set 4): chevron, title and a muted one-line summary of what is set inside (empty when
 * everything is at its default). Closed when the app starts; the state is kept while it runs.
 *
 * `off` (change set 5): the fold does not apply (Security with mqtt or ws). The header is greyed, has no chevron and
 * cannot be opened; it reads as closed while off, but the stored open state is kept for when it applies again. The
 * summary slot then carries the reason.
 */
export function FoldHeader({ id, title, summary, disabled, off }: { id: Fold; title: string; summary?: ReactNode; disabled?: boolean; off?: boolean }) {
  const stored = useApp((s) => s.folds[id]);
  const open = stored && !off;
  return (
    <button
      type="button"
      className={off ? 'fold off' : 'fold'}
      aria-expanded={open}
      aria-disabled={off || undefined}
      data-testid={`fold-${id}`}
      onClick={() => {
        if (!off) useApp.getState().toggleFold(id);
      }}
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
