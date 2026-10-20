import { getVersion } from '@tauri-apps/api/app';
import { openUrl } from '@tauri-apps/plugin-opener';
import {
  cancelDownload,
  downloadOcrModels,
  ocrStatus,
  openLogDir,
  pickFolder,
  type OcrStatus,
  type Settings,
  type SettingsLanguage,
  type ZipOutput,
} from '../api';
import { getLang, LANGS, LANGUAGE_NAMES, type Lang, type MessageKey, t } from '../i18n';
import { h, icons, setText, svg } from './dom';

export interface SettingsDialogOptions {
  settings: Settings;
  /** Persists the draft; resolves once stored (the caller re-renders on a language change). */
  onSave(next: Settings): Promise<void>;
  /** User-facing text for a failed action; shown inside the dialog, not behind its backdrop. */
  describeError(error: unknown): string;
}

const AUTHOR_URL = 'https://github.com/KeremV1';
const MIN_WORKERS = 1;
const MAX_WORKERS = 8;

/** The ZIP output choices with their labels, in display order. */
export const ZIP_OUTPUTS: ReadonlyArray<readonly [ZipOutput, MessageKey]> = [
  ['folder', 'settings.zipFolder'],
  ['single', 'settings.zipSingle'],
  ['zip', 'settings.zipZip'],
];

/** Shows `text` in the dialog's error line, or hides the line for `null`. */
export function setDialogError(line: Pick<HTMLElement, 'textContent' | 'hidden'>, text: string | null): void {
  line.textContent = text ?? '';
  line.hidden = text === null;
}

/** "12,2 MB" (tr) or "12.2 MB" (en): decimal megabytes with one decimal. */
export function formatMegabytes(bytes: number, lang: Lang): string {
  return `${new Intl.NumberFormat(lang, { minimumFractionDigits: 1, maximumFractionDigits: 1 }).format(bytes / 1_000_000)} MB`;
}

/** Which engine will run, whether it is ready, and (for ocrs) that it reads Latin-script text. */
export function ocrStatusLines(status: OcrStatus): string[] {
  if (status.engine === null) return [t('ocr.none')];
  if (status.engine === 'windows') return [t('ocr.windowsReady')];
  return [t(status.needsDownload ? 'ocr.ocrsNeedsModel' : 'ocr.ocrsReady'), t('ocr.ocrsLatin')];
}

/** Opens the modal settings dialog. The dialog removes itself when closed. */
export function openSettingsDialog({ settings, onSave, describeError }: SettingsDialogOptions): HTMLDialogElement {
  const draft: Settings = { ...settings, ocr: { ...settings.ocr } };
  const errorLine = h('p', { class: 'dialog-error', role: 'alert', hidden: true });
  const onError = (error: unknown): void => {
    console.error(error);
    setDialogError(errorLine, describeError(error));
  };

  const folder = h('span', { class: 'field-path' }, draft.outputDir);
  folder.title = draft.outputDir;
  const changeFolder = h('button', { class: 'btn btn-small', type: 'button' }, t('action.change'));
  changeFolder.addEventListener('click', async () => {
    setDialogError(errorLine, null);
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
    ...LANGS.map((code) => h('option', { value: code, lang: code }, LANGUAGE_NAMES[code])),
  ) as HTMLSelectElement;
  language.value = draft.language;

  const zipOutput = h(
    'select',
    { id: 'set-zip' },
    ...ZIP_OUTPUTS.map(([value, key]) => h('option', { value }, t(key))),
  ) as HTMLSelectElement;
  zipOutput.value = draft.zipOutput;

  // ---- OCR section ----
  const ocrEnabled = h('input', { type: 'checkbox', id: 'set-ocr' }) as HTMLInputElement;
  ocrEnabled.checked = draft.ocr.enabled;

  const statusList = h('ul', { class: 'ocr-status', 'aria-live': 'polite' }, h('li', {}, t('ocr.statusLoading')));
  const downloadButton = h('button', { class: 'btn btn-small', type: 'button', hidden: true }) as HTMLButtonElement;
  const progress = h('progress', { max: 100, value: 0, 'aria-label': t('settings.ocr') }) as HTMLProgressElement;
  const progressText = h('span', { class: 'ocr-progress-text' });
  const stopButton = h('button', { class: 'btn btn-quiet btn-small', type: 'button' }, t('ocr.downloadCancel'));
  const progressRow = h('div', { class: 'ocr-progress', hidden: true }, progress, progressText, stopButton);

  let status: OcrStatus | null = null;
  let downloading = false;
  const renderStatus = (): void => {
    if (!status) return;
    statusList.replaceChildren(...ocrStatusLines(status).map((line) => h('li', {}, line)));
    downloadButton.hidden = !status.needsDownload || downloading;
    progressRow.hidden = !downloading;
    setText(downloadButton, t('ocr.download', { size: formatMegabytes(status.ocrsDownloadBytes, getLang()) }));
  };
  let statusSeq = 0;
  /** Asks for the OCR status; a newer call makes older answers stale. */
  const refreshStatus = (): void => {
    const seq = ++statusSeq;
    ocrStatus()
      .then((next) => {
        if (seq !== statusSeq) return;
        status = next;
        renderStatus();
      })
      .catch((err: unknown) => {
        if (seq !== statusSeq) return;
        console.warn('OCR status unavailable', err);
        status = null;
        const reason = typeof err === 'object' && err !== null && 'message' in err ? String((err as { message: unknown }).message) : String(err);
        statusList.replaceChildren(h('li', {}, t('ocr.statusUnavailable', { error: reason })));
      });
  };
  const setProgress = (percent: number): void => {
    progress.value = percent;
    setText(progressText, t('ocr.downloading', { percent }));
  };
  downloadButton.addEventListener('click', () => {
    setDialogError(errorLine, null);
    downloading = true;
    setProgress(0);
    renderStatus();
    downloadOcrModels((e) => {
      if (e.event === 'progress') {
        setProgress(e.data.total > 0 ? Math.min(100, Math.floor((e.data.downloaded * 100) / e.data.total)) : 0);
      } else if (e.event === 'failed') {
        setDialogError(errorLine, t('ocr.downloadFailed', { message: e.data.message }));
      }
    })
      .catch(onError)
      .finally(() => {
        downloading = false;
        renderStatus();
        refreshStatus();
      });
  });
  stopButton.addEventListener('click', () => {
    cancelDownload().catch(onError);
  });
  refreshStatus();

  const ocrSection = h(
    'fieldset',
    { class: 'ocr' },
    h('legend', {}, t('settings.ocr')),
    h(
      'div',
      { class: 'field field-check' },
      ocrEnabled,
      h('label', { for: 'set-ocr' }, h('span', {}, t('settings.ocrEnabled')), h('small', { class: 'hint' }, t('settings.ocrEnabledHint'))),
    ),
    h('div', { class: 'field' }, statusList, downloadButton, progressRow),
  );

  const logs = h('button', { class: 'btn btn-quiet btn-small', type: 'button' }, svg(icons.folder), t('settings.openLogs'));
  logs.addEventListener('click', () => {
    setDialogError(errorLine, null);
    openLogDir().catch(onError);
  });

  const version = h('span', { class: 'about-version' });
  getVersion()
    .then((v) => {
      if (v) setText(version, ` ${v}`);
    })
    .catch((err: unknown) => console.warn('app version unavailable', err));
  const author = h('a', { href: AUTHOR_URL, class: 'about-link' }, 'github.com/KeremV1');
  author.addEventListener('click', (ev) => {
    ev.preventDefault();
    setDialogError(errorLine, null);
    openUrl(AUTHOR_URL).catch(onError);
  });
  const about = h(
    'section',
    { class: 'about', 'aria-labelledby': 'about-title' },
    h('h3', { class: 'about-title', id: 'about-title' }, t('settings.about')),
    h('p', {}, h('strong', {}, 'OmniMark'), version),
    h('p', {}, 'MoNoR · ', author),
    h('p', {}, t('settings.license')),
  );

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
    h('div', { class: 'field field-inline' }, h('label', { for: 'set-zip' }, t('settings.zipOutput')), zipOutput),
    ocrSection,
    h('div', { class: 'field' }, logs),
    about,
    errorLine,
    h('div', { class: 'dialog-actions' }, cancel, save),
  );

  const dialog = h('dialog', { class: 'dialog', 'aria-labelledby': 'settings-title' }, form) as HTMLDialogElement;
  form.querySelector('h2')?.setAttribute('id', 'settings-title');

  cancel.addEventListener('click', () => dialog.close());
  dialog.addEventListener('close', () => {
    if (downloading) cancelDownload().catch((err: unknown) => console.warn('cancel download failed', err));
    dialog.remove();
  });
  form.addEventListener('submit', async (ev) => {
    ev.preventDefault();
    const n = Math.round(Number(workers.value));
    draft.workers = Number.isFinite(n) ? Math.min(MAX_WORKERS, Math.max(MIN_WORKERS, n)) : settings.workers;
    draft.extractImages = images.checked;
    draft.language = language.value as SettingsLanguage;
    draft.zipOutput = zipOutput.value as ZipOutput;
    draft.ocr = { enabled: ocrEnabled.checked };
    save.disabled = true;
    setDialogError(errorLine, null);
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
