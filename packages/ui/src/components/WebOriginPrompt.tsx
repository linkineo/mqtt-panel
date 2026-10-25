import { useEffect, useState } from 'react';
import { useTranslation } from 'react-i18next';
import type { BrokerOriginEvent } from '@mqttpanel/engine-client';
import { engine } from '../services';
import { useApp } from '../store/app';
import { Modal } from './ui';

/** How long the broker waits for an answer (SPEC 6); the prompt closes itself then (the broker refused). */
const PROMPT_MS = 30_000;

/**
 * The web page prompt (SPEC 6, allowed web origins): while Anonymous is on, a browser page whose origin is not on the
 * listener's list asks "A web page at … wants to connect …" with Allow and Deny. Allow adds the origin to the list;
 * Deny, closing the dialog or no answer within 30 s refuses the page. Mounted once in App.
 */
export function WebOriginLayer() {
  const [queue, setQueue] = useState<BrokerOriginEvent[]>([]);
  useEffect(
    () =>
      engine().onEvent((e) => {
        if (e.type === 'brokerOrigin') setQueue((q) => [...q.filter((x) => x.id !== e.id), e]);
        if (e.type === 'brokerOriginDone') setQueue((q) => q.filter((x) => x.id !== e.id));
      }),
    [],
  );
  const req = queue[0];
  if (!req) return null;
  const answer = (allow: boolean) => {
    setQueue((q) => q.filter((x) => x.id !== req.id));
    void engine()
      .answerOrigin(req.id, allow)
      .then(() => useApp.getState().refreshBrokerConfig())
      .catch(() => undefined);
  };
  return <WebOriginDialog key={req.id} req={req} onAnswer={answer} />;
}

function WebOriginDialog(props: { req: BrokerOriginEvent; onAnswer(allow: boolean): void }) {
  const { t } = useTranslation();
  const { req, onAnswer } = props;
  useEffect(() => {
    const timer = setTimeout(() => onAnswer(false), PROMPT_MS);
    return () => clearTimeout(timer);
  }, [onAnswer]);
  return (
    <Modal
      title={t('broker.originPromptTitle')}
      onClose={() => onAnswer(false)}
      width={520}
      testId="web-origin-prompt"
      footer={
        <>
          <button type="button" className="btn" data-autofocus data-testid="origin-deny" onClick={() => onAnswer(false)}>
            {t('broker.deny')}
          </button>
          <button type="button" className="btn pri" data-testid="origin-allow" onClick={() => onAnswer(true)}>
            {t('broker.allow')}
          </button>
        </>
      }
    >
      <p data-testid="web-origin-text" style={{ margin: 0 }}>
        {t('broker.originPrompt', { origin: req.origin })}
      </p>
    </Modal>
  );
}
