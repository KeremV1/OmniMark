import { type MessageKey, t } from '../i18n';
import type { QueueItem } from '../queue';
import { h } from './dom';

/** Modal with the full error message and warnings of one item. Removes itself when closed. */
export function openDetailsDialog(item: QueueItem): HTMLDialogElement {
  const summary = item.error ? t(`error.${item.error.kind}` as MessageKey) : t(`status.${item.status}` as MessageKey);

  const rows: [string, Node | string][] = [[t('details.file'), item.path]];
  if (item.output) rows.push([t('details.output'), item.output]);
  if (item.error) rows.push([t('details.message'), h('code', { class: 'details-message' }, item.error.message)]);

  const warnings =
    item.warnings.length > 0
      ? h('ul', { class: 'details-warnings' }, ...item.warnings.map((w) => h('li', {}, w)))
      : h('p', { class: 'muted' }, t('details.none'));

  const close = h('button', { class: 'btn btn-primary', type: 'submit', autofocus: true }, t('action.close'));
  const dialog = h(
    'dialog',
    { class: 'dialog', 'aria-labelledby': 'details-title' },
    h(
      'form',
      { method: 'dialog', class: 'details' },
      h('h2', { class: 'dialog-title', id: 'details-title' }, item.name),
      h('p', { class: item.status === 'failed' ? 'details-summary is-failed' : 'details-summary' }, summary),
      h('dl', { class: 'details-list' }, ...rows.flatMap(([k, v]) => [h('dt', {}, k), h('dd', {}, v)])),
      h('h3', { class: 'details-sub' }, t('details.warnings')),
      warnings,
      h('div', { class: 'dialog-actions' }, close),
    ),
  ) as HTMLDialogElement;
  dialog.addEventListener('close', () => dialog.remove());
  document.body.append(dialog);
  dialog.showModal();
  return dialog;
}
