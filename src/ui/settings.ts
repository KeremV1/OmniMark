import { openLogDir, pickFolder, type Settings, type SettingsLanguage } from '../api';
import { t } from '../i18n';
import { h, icons, setText, svg } from './dom';

export interface SettingsDialogOptions {
  settings: Settings;
  /** Persists the draft; resolves once stored (the caller re-renders on a language change). */
  onSave(next: Settings): Promise<void>;
  onError(error: unknown): void;
}

const MIN_WORKERS = 1;
const MAX_WORKERS = 8;

/** Opens the modal settings dialog. The dialog removes itself when closed. */
export function openSettingsDialog({ settings, onSave, onError }: SettingsDialogOptions): HTMLDialogElement {
  const draft: Settings = { ...settings };

  const folder = h('span', { class: 'field-path' }, draft.outputDir);
  folder.title = draft.outputDir;
  const changeFolder = h('button', { class: 'btn btn-small', type: 'button' }, t('action.change'));
  changeFolder.addEventListener('click', async () => {
    try {
      const picked = await pickFolder();
      if (picked) {
        draft.outputDir = picked;
        setText(folder, picked);
        folder.title = picked;
      }
    } catch (err) {
      onError(err);
    }
  });

  const images = h('input', { type: 'checkbox', id: 'set-images' }) as HTMLInputElement;
  images.checked = draft.extractImages;

  const workers = h('input', {
    type: 'number',
    id: 'set-workers',
    min: MIN_WORKERS,
    max: MAX_WORKERS,
    step: 1,
    inputmode: 'numeric',
  }) as HTMLInputElement;
  workers.value = String(draft.workers);

  const language = h(
    'select',
    { id: 'set-lang' },
    h('option', { value: 'auto' }, t('settings.languageAuto')),
    h('option', { value: 'tr', lang: 'tr' }, t('settings.languageTr')),
    h('option', { value: 'en', lang: 'en' }, t('settings.languageEn')),
  ) as HTMLSelectElement;
  language.value = draft.language;

  const logs = h('button', { class: 'btn btn-quiet btn-small', type: 'button' }, svg(icons.folder), t('settings.openLogs'));
  logs.addEventListener('click', () => openLogDir().catch(onError));

  const cancel = h('button', { class: 'btn', type: 'button' }, t('action.cancel'));
  const save = h('button', { class: 'btn btn-primary', type: 'submit' }, t('action.save')) as HTMLButtonElement;

  const form = h(
    'form',
    { method: 'dialog', class: 'settings' },
    h('h2', { class: 'dialog-title' }, t('action.settings')),
    h(
      'div',
      { class: 'field' },
      h('span', { class: 'field-label', id: 'set-folder-label' }, t('settings.outputDir')),
      h('div', { class: 'field-row', role: 'group', 'aria-labelledby': 'set-folder-label' }, folder, changeFolder),
    ),
    h(
      'div',
      { class: 'field field-check' },
      images,
      h('label', { for: 'set-images' }, h('span', {}, t('settings.extractImages')), h('small', { class: 'hint' }, t('settings.extractImagesHint'))),
    ),
    h(
      'div',
      { class: 'field field-inline' },
      h('label', { for: 'set-workers' }, h('span', {}, t('settings.workers')), h('small', { class: 'hint' }, t('settings.workersHint'))),
      workers,
    ),
    h('div', { class: 'field field-inline' }, h('label', { for: 'set-lang' }, t('settings.language')), language),
    h('div', { class: 'field' }, logs),
    h('div', { class: 'dialog-actions' }, cancel, save),
  );

  const dialog = h('dialog', { class: 'dialog', 'aria-labelledby': 'settings-title' }, form) as HTMLDialogElement;
  form.querySelector('h2')?.setAttribute('id', 'settings-title');

  cancel.addEventListener('click', () => dialog.close());
  dialog.addEventListener('close', () => dialog.remove());
  form.addEventListener('submit', async (ev) => {
    ev.preventDefault();
    const n = Math.round(Number(workers.value));
    draft.workers = Number.isFinite(n) ? Math.min(MAX_WORKERS, Math.max(MIN_WORKERS, n)) : settings.workers;
    draft.extractImages = images.checked;
    draft.language = language.value as SettingsLanguage;
    save.disabled = true;
    try {
      await onSave(draft);
      dialog.close();
    } catch (err) {
      onError(err);
    } finally {
      save.disabled = false;
    }
  });

  document.body.append(dialog);
  dialog.showModal();
  return dialog;
}
