import { useEffect, useState } from 'react';
import { useTranslation } from 'react-i18next';
import { engine } from '../services';
import { run } from '../lib/actions';
import { useApp } from '../store/app';
import { payloadText } from '../lib/topicActions';
import { Modal, Msg } from './ui';

/** Mockup 09: delete one retained message, showing the current retained value first. */
export function DeleteRetainedDialog({ connId, topic, onClose }: { connId: string; topic: string; onClose(): void }) {
  const { t } = useTranslation();
  const [value, setValue] = useState<string | null>(null);
  const [retained, setRetained] = useState<boolean | null>(null);
  useEffect(() => {
    void engine()
      .topicDetail(connId, topic)
      .then((d) => {
        setValue(d.last ? payloadText(d.last) : null);
        setRetained(d.last?.retain ?? false);
      })
      .catch(() => setRetained(false));
  }, [connId, topic]);
  return (
    <Modal
      title={t('delret.title')}
      width={520}
      onClose={onClose}
      testId="delete-retained"
      footer={
        <>
          <button type="button" className="btn" onClick={onClose}>
            {t('common.cancel')}
          </button>
          <button
            type="button"
            className="btn danger solid"
            data-testid="delete-retained-ok"
            onClick={async () => {
              const e = await run(engine().deleteRetained(connId, topic));
              if (e?.status === 'failed') useApp.getState().toast({ kind: 'err', msg: e.reason });
              else if (e) useApp.getState().toast({ kind: 'info', text: t('delret.done', { topic }) });
              onClose();
            }}
          >
            {t('delret.confirm')}
          </button>
        </>
      }
    >
      <div className="mono selectable">{topic}</div>
      <div className="muted small">{t('delret.explain')}</div>
      {retained === false && !value ? (
        <div className="hint">{t('delret.noneKnown')}</div>
      ) : (
        <div className="json wrap" style={{ fontSize: 11 }}>
          {value ?? '…'}
        </div>
      )}
      <div className="muted small">{t('delret.how')}</div>
    </Modal>
  );
}

/** Subtree delete: preview of the affected retained topics, then confirm. */
export function DeleteSubtreeDialog({ connId, prefix, onClose }: { connId: string; prefix: string; onClose(): void }) {
  const { t } = useTranslation();
  const [list, setList] = useState<{ topic: string; preview: string }[] | null>(null);
  useEffect(() => {
    void engine()
      .retainedUnder(connId, prefix)
      .then(setList)
      .catch(() => setList([]));
  }, [connId, prefix]);
  return (
    <Modal
      title={t('delsub.title', { prefix })}
      width={600}
      onClose={onClose}
      testId="delete-subtree"
      footer={
        <>
          <button type="button" className="btn" onClick={onClose}>
            {t('common.cancel')}
          </button>
          <button
            type="button"
            className="btn danger solid"
            disabled={!list || list.length === 0}
            data-testid="delete-subtree-ok"
            onClick={async () => {
              const n = await run(engine().deleteRetainedMany(connId, list!.map((x) => x.topic)));
              if (n !== null) useApp.getState().toast({ kind: 'info', text: t('delsub.done', { count: n }) });
              onClose();
            }}
          >
            {t('delsub.confirm', { count: list?.length ?? 0 })}
          </button>
        </>
      }
    >
      <div className="muted small">{t('delsub.explain')}</div>
      {list === null ? (
        <div className="hint">…</div>
      ) : list.length === 0 ? (
        <div className="hint">
          <Msg msg={{ key: 'delsub.none', params: {}, text: 'No retained messages under this topic.' }} />
        </div>
      ) : (
        <div style={{ maxHeight: 280, overflow: 'auto' }}>
          <table className="tbl">
            <thead>
              <tr>
                <th>{t('col.topic')}</th>
                <th>{t('delsub.value')}</th>
              </tr>
            </thead>
            <tbody>
              {list.map((x) => (
                <tr key={x.topic}>
                  <td className="mono">{x.topic}</td>
                  <td className="mono">{x.preview}</td>
                </tr>
              ))}
            </tbody>
          </table>
        </div>
      )}
    </Modal>
  );
}
