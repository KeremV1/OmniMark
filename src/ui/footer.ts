import { t } from '../i18n';
import { modKeyLabel } from '../platform';
import { convertiblePaths, counts, type Filter } from '../queue';
import type { Actions, View } from './context';
import { h, setText } from './dom';
import { updateNotice } from './update';

export interface Footer {
  el: HTMLElement;
  update(view: View): void;
}

const FILTERS: Filter[] = ['all', 'done', 'failed', 'pending'];

export function createFooter(actions: Actions): Footer {
  const buttons = FILTERS.map((f) => {
    const count = h('span', { class: 'filter-count' });
    const btn = h(
      'button',
      { class: 'filter', type: 'button', 'aria-pressed': 'false', onclick: () => actions.setFilter(f) },
      h('span', {}, t(`filter.${f}`)),
      count,
    );
    return { f, btn, count };
  });

  const cancel = h('button', { class: 'btn', type: 'button', hidden: true, onclick: () => actions.cancel() }, t('action.cancel'));
  const create = h('button', {
    class: 'btn btn-primary',
    type: 'button',
    title: `${modKeyLabel()}+Enter`,
    onclick: () => actions.convertSelected(),
  }) as HTMLButtonElement;

  const updateText = h('span', { class: 'update-text' });
  const updateButton = h('button', {
    class: 'btn btn-small',
    type: 'button',
    onclick: () => actions.installUpdate(),
  }) as HTMLButtonElement;
  const updateEl = h('div', { class: 'update-notice', role: 'status', hidden: true }, updateText, updateButton);

  const filters = h('div', { class: 'filters', role: 'group', 'aria-label': t('filter.label') }, ...buttons.map((b) => b.btn));
  const el = h(
    'footer',
    { class: 'footer' },
    filters,
    updateEl,
    h('div', { class: 'footer-actions' }, cancel, create),
  );

  return {
    el,
    update(view) {
      const c = counts(view.queue);
      filters.hidden = c.all === 0;
      for (const { f, btn, count } of buttons) {
        setText(count, String(c[f]));
        btn.setAttribute('aria-pressed', String(view.queue.filter === f));
      }
      const n = convertiblePaths(view.queue).length;
      setText(create, t('action.convertCount', { n }));
      create.disabled = n === 0 || view.queue.running;
      cancel.hidden = !view.queue.running;
      const notice = updateNotice(view.update);
      updateEl.hidden = notice === null;
      if (notice) {
        setText(updateText, t(notice.textKey, { version: notice.version }));
        setText(updateButton, t(notice.buttonKey));
        updateButton.disabled = notice.busy;
      }
    },
  };
}
