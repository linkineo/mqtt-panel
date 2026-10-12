import { useEffect, useState } from 'react';
import { useTranslation } from 'react-i18next';
import type { FieldInfo } from '@mqttpanel/engine-client';
import { engine } from '../../services';
import { useCharts, type PickerRequest } from '../../store/charts';
import { useApp } from '../../store/app';
import { Check, Modal, Seg } from '../../components/ui';
import { pathLabel } from '../../components/JsonView';
import { shortTopic } from '../../lib/format';

export function castLabel(f: Pick<FieldInfo, 'cast' | 'plottable'>, t: (k: string) => string): string {
  if (f.cast === 'number') return t('cast.number');
  if (f.cast === 'parsedNumber') return t('cast.parsedNumber');
  if (f.cast === 'boolean') return t('cast.boolean');
  if (f.cast === 'null') return t('cast.null');
  return t('field.notPlottable');
}

/** Mockup 10b: pick which fields of a topic become series. Nothing preselected unless opened from a field (D-16). */
export function FieldPickerView(props: {
  req: PickerRequest;
  fields: FieldInfo[];
  charts: { id: string; name: string }[];
  onAdd(chartId: string | 'new', items: { path: string; label: string; step: boolean }[]): void;
  onClose(): void;
}) {
  const { t } = useTranslation();
  // plottable fields first, text fields greyed at the bottom (mockup 10b); payload order kept within each group
  const leaves = props.fields
    .filter((f) => f.cast !== 'object' && f.cast !== 'array')
    .map((f, i) => ({ f, i }))
    .sort((a, b) => Number(b.f.plottable) - Number(a.f.plottable) || a.i - b.i)
    .map((x) => x.f);
  const [checked, setChecked] = useState<Set<string>>(() => {
    const pre = props.req.preselect;
    return new Set(pre && leaves.some((f) => f.path === pre && f.plottable) ? [pre] : pre && !leaves.some((f) => f.path === pre) ? [pre] : []);
  });
  const [extra, setExtra] = useState<string[]>(() => (props.req.preselect && !leaves.some((f) => f.path === props.req.preselect) ? [props.req.preselect] : []));
  const [other, setOther] = useState('');
  const [target, setTarget] = useState<string>(
    props.req.chartId && props.charts.some((c) => c.id === props.req.chartId) ? props.req.chartId : (props.charts[0]?.id ?? 'new'),
  );
  const toggle = (p: string) => {
    const n = new Set(checked);
    if (n.has(p)) n.delete(p);
    else n.add(p);
    setChecked(n);
  };
  const addOther = () => {
    let p = other.trim();
    if (!p) return;
    if (!p.startsWith('$')) p = '$.' + p.replace(/^\./, '');
    if (!extra.includes(p) && !leaves.some((f) => f.path === p)) setExtra([...extra, p]);
    setChecked(new Set([...checked, p]));
    setOther('');
  };
  const n = checked.size;
  return (
    <Modal
      title={
        <>
          {t('picker.title')} <span className="muted">· {props.req.topic}</span>
        </>
      }
      width={560}
      onClose={props.onClose}
      testId="field-picker"
      footer={
        <>
          <button type="button" className="btn" onClick={props.onClose}>
            {t('common.cancel')}
          </button>
          <button
            type="button"
            className="btn pri"
            disabled={n === 0}
            data-testid="picker-add"
            onClick={() => {
              const items = Array.from(checked).map((p) => {
                const f = leaves.find((x) => x.path === p);
                return { path: p, label: pathLabel(p), step: f?.cast === 'boolean' };
              });
              props.onAdd(target, items);
              props.onClose();
            }}
          >
            {t('picker.addN', { count: n })}
          </button>
        </>
      }
    >
      <div className="muted small">{t('picker.help')}</div>
      <table className="tbl">
        <thead>
          <tr>
            <th style={{ width: 28 }} />
            <th>{t('picker.field')}</th>
            <th>{t('picker.lastValue')}</th>
            <th>{t('picker.cast')}</th>
          </tr>
        </thead>
        <tbody>
          {leaves.map((f) => (
            <tr key={f.path} className={f.plottable ? 'click' : 'dim'} data-testid="picker-row" data-plottable={f.plottable} onClick={() => f.plottable && toggle(f.path)}>
              <td>
                {f.plottable ? (
                  <Check on={checked.has(f.path)} label="" ariaLabel={f.label} testId={`picker-check-${f.label}`} onChange={() => undefined} />
                ) : null}
              </td>
              <td className="mono">{f.label}</td>
              <td className="mono">{f.lastValue}</td>
              <td>{castLabel(f, t)}</td>
            </tr>
          ))}
          {extra.map((p) => (
            <tr key={p} className="click" onClick={() => toggle(p)}>
              <td>
                <Check on={checked.has(p)} label="" ariaLabel={pathLabel(p)} onChange={() => undefined} />
              </td>
              <td className="mono">{pathLabel(p)}</td>
              <td className="muted">—</td>
              <td>{t('picker.typed')}</td>
            </tr>
          ))}
        </tbody>
      </table>
      <div className="row">
        <span className="muted small nowrap">{t('picker.otherPath')}</span>
        <input
          className="input mono"
          style={{ flex: 1 }}
          placeholder="$.supply.pressure"
          value={other}
          aria-label={t('picker.otherPath')}
          data-testid="picker-other"
          onChange={(e) => setOther(e.target.value)}
          onKeyDown={(e) => e.key === 'Enter' && addOther()}
        />
        <button type="button" className="btn sm" onClick={addOther}>
          {t('common.add')}
        </button>
      </div>
      <div className="row" style={{ minWidth: 0 }}>
        <span className="muted small nowrap">{t('picker.addTo')}</span>
        <div style={{ overflow: 'auto', minWidth: 0 }}>
          <Seg
            label={t('picker.addTo')}
            value={target}
            onChange={setTarget}
            options={[...props.charts.map((c) => ({ value: c.id, label: c.name })), { value: 'new', label: t('picker.newChart'), testId: 'picker-new-chart' }]}
          />
        </div>
      </div>
    </Modal>
  );
}

export function FieldPicker() {
  const req = useCharts((s) => s.picker);
  const charts = useCharts((s) => s.charts);
  const [fields, setFields] = useState<FieldInfo[] | null>(null);
  const { t } = useTranslation();
  useEffect(() => {
    setFields(null);
    if (!req) return;
    void engine()
      .topicDetail(req.connId, req.topic)
      .then((d) => setFields(d.fields))
      .catch(() => setFields([]));
  }, [req]);
  if (!req || !fields) return null;
  return (
    <FieldPickerView
      req={req}
      fields={fields}
      charts={charts.filter((c) => c.connId === req.connId).map((c) => ({ id: c.id, name: c.name }))}
      onClose={() => useCharts.getState().closePicker()}
      onAdd={(chartId, items) => {
        const st = useCharts.getState();
        const id = chartId === 'new' ? st.createChart(req.connId, shortTopic(req.topic, 2).replace(/^…\//, '').replace('/', ' · ')) : chartId;
        st.addSeries(
          id,
          items.map((i) => ({ topic: req.topic, path: i.path, name: i.label === 'value' ? req.topic.split('/').pop()! : i.label, step: i.step })),
        );
        st.setActive(id);
        useApp.getState().toast({ kind: 'info', text: t('picker.added', { count: items.length }) });
      }}
    />
  );
}
