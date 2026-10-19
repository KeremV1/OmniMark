import { getCurrentWebview } from '@tauri-apps/api/webview';
import { t } from '../i18n';
import { modKeyLabel } from '../platform';
import logoMarkup from '../assets/logo.svg?raw';
import type { Actions, View } from './context';
import { h, icons, setText, svg } from './dom';

export interface Dropzone {
  /** The empty state: the whole content area is the drop target. */
  el: HTMLElement;
  /** Shown over a non-empty list while files are dragged over the window. */
  overlay: HTMLElement;
  update(view: View): void;
}

// The bundled logo, recoloured to follow the text colour so CSS can tint the silhouette.
const silhouette = logoMarkup.replace(/#1B1D21/gi, 'currentColor');

export function createDropzone(actions: Actions): Dropzone {
  const hint = h('p', { class: 'drop-hint' });
  const add = h(
    'button',
    { class: 'btn btn-primary', type: 'button', title: `${modKeyLabel()}+O`, onclick: () => actions.addFiles() },
    svg(icons.plus),
    t('action.addFiles'),
  );
  const el = h(
    'section',
    { class: 'dropzone', hidden: true },
    h(
      'div',
      { class: 'drop-inner' },
      svg(silhouette, 'drop-mark'),
      hint,
      h('p', { class: 'drop-formats' }, t('drop.formats')),
      h('div', { class: 'drop-add' }, h('span', { class: 'muted' }, t('drop.orAdd')), add, h('kbd', {}, `${modKeyLabel()}+O`)),
    ),
  );
  const overlay = h('div', { class: 'drop-overlay', hidden: true, 'aria-hidden': 'true' }, h('p', {}, t('drop.active')));

  return {
    el,
    overlay,
    update(view) {
      const empty = view.queue.items.length === 0;
      el.hidden = !empty;
      el.classList.toggle('is-dragging', view.dragging);
      setText(hint, view.dragging ? t('drop.active') : t('drop.hint'));
      overlay.hidden = empty || !view.dragging;
    },
  };
}

/** OS-level drag and drop (the webview reports real paths); HTML5 drag events are not used. */
export async function listenDragDrop(actions: Actions): Promise<() => void> {
  return getCurrentWebview().onDragDropEvent((event) => {
    switch (event.payload.type) {
      case 'enter':
      case 'over':
        actions.setDragging(true);
        break;
      case 'leave':
        actions.setDragging(false);
        break;
      case 'drop':
        actions.setDragging(false);
        if (event.payload.paths.length > 0) actions.addPaths(event.payload.paths);
        break;
    }
  });
}
