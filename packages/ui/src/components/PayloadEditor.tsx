import { useEffect, useRef } from 'react';
import { EditorState, Compartment } from '@codemirror/state';
import { EditorView, keymap, placeholder as cmPlaceholder } from '@codemirror/view';
import { defaultKeymap, history, historyKeymap } from '@codemirror/commands';
import { json } from '@codemirror/lang-json';
import { HighlightStyle, syntaxHighlighting, bracketMatching } from '@codemirror/language';
import { tags } from '@lezer/highlight';
import type { PayloadEncoding } from '@mqttpanel/engine-client';

// Colours follow the mockup's .json classes: keys slate, strings sage, numbers clay.
const style = HighlightStyle.define([
  { tag: tags.propertyName, color: 'var(--slate)' },
  { tag: tags.string, color: 'var(--sage)' },
  { tag: tags.number, color: 'var(--clay)' },
  { tag: [tags.bool, tags.null], color: 'var(--clay)' },
  { tag: tags.punctuation, color: 'var(--ink)' },
]);

/** The composer payload editor (CodeMirror 6) with JSON highlighting; ⌘↵ / Ctrl+↵ sends. */
export function PayloadEditor(props: { value: string; format: PayloadEncoding; onChange(v: string): void; onSend(): void; label: string; placeholder?: string }) {
  const host = useRef<HTMLDivElement>(null);
  const view = useRef<EditorView | null>(null);
  const lang = useRef(new Compartment());
  const cb = useRef(props);
  cb.current = props;

  useEffect(() => {
    if (!host.current) return;
    const v = new EditorView({
      parent: host.current,
      state: EditorState.create({
        doc: props.value,
        extensions: [
          history(),
          bracketMatching(),
          keymap.of([
            {
              key: 'Mod-Enter',
              run: () => {
                cb.current.onSend();
                return true;
              },
            },
            ...defaultKeymap,
            ...historyKeymap,
          ]),
          lang.current.of(props.format === 'json' ? json() : []),
          syntaxHighlighting(style),
          EditorView.lineWrapping,
          cmPlaceholder(props.placeholder ?? ''),
          EditorView.contentAttributes.of({ 'aria-label': props.label, 'data-testid': 'payload-editor', autocapitalize: 'off', autocorrect: 'off', spellcheck: 'false' }),
          EditorView.updateListener.of((u) => {
            if (u.docChanged) cb.current.onChange(u.state.doc.toString());
          }),
        ],
      }),
    });
    view.current = v;
    return () => {
      v.destroy();
      view.current = null;
    };
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, []);

  useEffect(() => {
    const v = view.current;
    if (!v) return;
    const cur = v.state.doc.toString();
    if (cur !== props.value) v.dispatch({ changes: { from: 0, to: cur.length, insert: props.value } });
  }, [props.value]);

  useEffect(() => {
    view.current?.dispatch({ effects: lang.current.reconfigure(props.format === 'json' ? json() : []) });
  }, [props.format]);

  return <div className="editor" ref={host} />;
}
