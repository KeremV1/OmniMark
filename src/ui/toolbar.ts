import { t } from '../i18n';
import { modKeyLabel } from '../platform';
import { removablePaths } from '../queue';
import logoDark from '../assets/logo.svg';
import logoLight from '../assets/logo-light.svg';
import { type Actions, type View, shortPath } from './context';
import { h, icons, setText, svg, toggleAttr } from './dom';

export interface Toolbar {
  el: HTMLElement;
  update(view: View): void;
}

export function createToolbar(actions: Actions): Toolbar {
  const mod = modKeyLabel();

  // The OS theme picks the ink (CSS shows one): dark mark on light chrome, white mark on dark chrome.
  // Two <img>s instead of <picture>: Vite may inline the SVGs as data URIs, which srcset cannot hold.
  const logo = h(
    'span',
    { class: 'brand-mark' },
    h('img', { class: 'on-light', src: logoDark, alt: '', width: 24, height: 24 }),
    h('img', { class: 'on-dark', src: logoLight, alt: '', width: 24, height: 24 }),
  );

  const add = h(
    'button',
    { class: 'btn', type: 'button', title: `${t('action.addFiles')} (${mod}+O)`, onclick: () => actions.addFiles() },
    svg(icons.plus),
    t('action.addFiles'),
  );
  const remove = h(
    'button',
    { class: 'btn btn-quiet', type: 'button', title: `${t('action.remove')} (Delete)`, onclick: () => actions.removeSelected() },
    t('action.remove'),
  );

  const outputPath = h('button', { class: 'output-path', type: 'button', onclick: () => actions.openOutputDir() });
  const output = h(
    'div',
    { class: 'output' },
    h('span', { class: 'output-label' }, t('output.short')),
    outputPath,
    h('button', { class: 'btn btn-quiet btn-small', type: 'button', onclick: () => actions.changeOutputDir() }, t('action.change')),
  );
  const settings = h(
    'button',
    {
      class: 'btn-icon',
      type: 'button',
      title: t('action.settings'),
      'aria-label': t('action.settings'),
      onclick: () => actions.openSettings(),
    },
    svg(icons.gear),
  );

  const el = h(
    'header',
    { class: 'toolbar' },
    h('div', { class: 'brand' }, logo, h('span', { class: 'brand-name' }, t('app.title'))),
    h('div', { class: 'toolbar-actions' }, add, remove),
    h('div', { class: 'toolbar-end' }, output, settings),
  );

  return {
    el,
    update(view) {
      toggleAttr(remove, 'disabled', removablePaths(view.queue).length === 0);
      setText(outputPath, shortPath(view.settings.outputDir));
      outputPath.title = `${t('action.openFolder')}: ${view.settings.outputDir}`;
    },
  };
}
