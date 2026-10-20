import type { Settings } from '../api';
import { t } from '../i18n';
import type { Actions, View } from './context';
import { h, setText } from './dom';

export interface FirstRun {
  el: HTMLElement;
  update(view: View): void;
}

/** The card shows until the user answers it once (also for settings files from before it existed). */
export function showFirstRun(settings: Pick<Settings, 'onboarded'>): boolean {
  return !settings.onboarded;
}

export function createFirstRun(actions: Actions): FirstRun {
  const path = h('span', { class: 'first-run-path' });
  const ok = h('button', { class: 'btn btn-primary', type: 'button', onclick: () => actions.dismissFirstRun() }, t('action.ok'));
  const change = h('button', { class: 'btn', type: 'button', onclick: () => actions.changeOutputDir() }, t('action.change'));
  const el = h(
    'section',
    { class: 'first-run', role: 'region', 'aria-labelledby': 'first-run-text', hidden: true },
    h('p', { class: 'first-run-text', id: 'first-run-text' }, h('span', {}, t('firstRun.text')), path),
    h('div', { class: 'first-run-actions' }, change, ok),
  );
  return {
    el,
    update(view) {
      el.hidden = !showFirstRun(view.settings);
      setText(path, view.settings.outputDir);
      path.title = view.settings.outputDir;
    },
  };
}
