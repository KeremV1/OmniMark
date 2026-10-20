import { getVersion } from '@tauri-apps/api/app';
import { openUrl } from '@tauri-apps/plugin-opener';
import {
  cancelDownload,
  downloadOcrModels,
  ocrStatus,
  openLogDir,
  openTesseractPage,
  pickFolder,
  pickProgram,
  type OcrEngineChoice,
  type OcrLanguage,
  type OcrSettings,
  type OcrStatus,
  type Settings,
  type SettingsLanguage,
} from '../api';
import { getLang, t, type Lang } from '../i18n';
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

/** Shows `text` in the dialog's error line, or hides the line for `null`. */
export function setDialogError(line: Pick<HTMLElement, 'textContent' | 'hidden'>, text: string | null): void {
  line.textContent = text ?? '';
  line.hidden = text === null;
}

/** "12,2 MB" (tr) or "12.2 MB" (en): decimal megabytes with one decimal. */
export function formatMegabytes(bytes: number, lang: Lang): string {
  return `${new Intl.NumberFormat(lang, { minimumFractionDigits: 1, maximumFractionDigits: 1 }).format(bytes / 1_000_000)} MB`;
}

/** One line per engine, then a note when nothing is usable or ocrs is chosen for Turkish. `language` is the resolved OCR language. */
export function ocrStatusLines(status: OcrStatus, language: 'tr' | 'en' | 'auto'): string[] {
  const lines: string[] = [];
  if (status.windowsBuilt) {
    lines.push(status.windowsLanguages.length > 0 ? t('ocr.windowsReady', { langs: status.windowsLanguages.join(', ') }) : t('ocr.windowsNoLanguage'));
  }
  lines.push(
    status.tesseractPath && status.tesseractLanguages.length > 0
      ? t('ocr.tesseractReady', { path: status.tesseractPath, langs: status.tesseractLanguages.join(', ') })
      : t(status.windowsBuilt && status.windowsLanguages.length > 0 ? 'ocr.tesseractOptional' : 'ocr.tesseractMissing'),
  );
  if (status.ocrsBuilt) lines.push(t(status.ocrsModels ? 'ocr.ocrsReady' : 'ocr.ocrsMissing'));
  if (status.selected === null) {
    const anyAvailable =
      (status.windowsBuilt && status.windowsLanguages.length > 0) ||
      (status.tesseractPath !== null && status.tesseractLanguages.length > 0) ||
      (status.ocrsBuilt && status.ocrsModels);
    lines.push(t(anyAvailable ? 'ocr.selectedUnavailable' : 'ocr.none'));
  }
  if (status.selected === 'ocrs' && language === 'tr') lines.push(t('ocr.ocrsTurkish'));
  return lines;
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
    h('option', { value: 'tr', lang: 'tr' }, t('settings.languageTr')),
    h('option', { value: 'en', lang: 'en' }, t('settings.languageEn')),
  ) as HTMLSelectElement;
  language.value = draft.language;

  // ---- OCR section ----
  const ocrEnabled = h('input', { type: 'checkbox', id: 'set-ocr' }) as HTMLInputElement;
  ocrEnabled.checked = draft.ocr.enabled;

  const engine = h(
    'select',
    { id: 'set-ocr-engine' },
    h('option', { value: 'auto' }, t('settings.ocrEngineAuto')),
    h('option', { value: 'windows' }, t('settings.ocrEngineWindows')),
    h('option', { value: 'tesseract' }, t('settings.ocrEngineTesseract')),
    h('option', { value: 'ocrs' }, t('settings.ocrEngineOcrs')),
  ) as HTMLSelectElement;
  engine.value = draft.ocr.engine;

  const tesseractPathText = h('span', { class: 'field-path' });
  const showTesseractPath = (): void => {
    const path = draft.ocr.tesseractPath;
    setText(tesseractPathText, path ?? t('settings.tesseractPathAuto'));
    tesseractPathText.title = path ?? '';
    tesseractPathText.classList.toggle('is-placeholder', path === null);
  };
  const chooseTesseract = h('button', { class: 'btn btn-small', type: 'button' }, t('settings.tesseractChoose'));
  const clearTesseract = h('button', { class: 'btn btn-quiet btn-small', type: 'button' }, t('settings.tesseractClear'));
  const tesseractField = h(
    'div',
    { class: 'field' },
    h('span', { class: 'field-label', id: 'set-tesseract-label' }, t('settings.tesseractPath')),
    h('div', { class: 'field-row', role: 'group', 'aria-labelledby': 'set-tesseract-label' }, tesseractPathText, chooseTesseract, clearTesseract),
  );
  const refreshTesseractField = (): void => {
    tesseractField.hidden = engine.value !== 'auto' && engine.value !== 'tesseract';
    clearTesseract.hidden = draft.ocr.tesseractPath === null;
    showTesseractPath();
  };

  const ocrLanguage = h(
    'select',
    { id: 'set-ocr-lang' },
    h('option', { value: 'auto' }, t('settings.ocrLanguageAuto')),
    h('option', { value: 'tr', lang: 'tr' }, t('settings.languageTr')),
    h('option', { value: 'en', lang: 'en' }, t('settings.languageEn')),
  ) as HTMLSelectElement;
  ocrLanguage.value = draft.ocr.language;

  const statusList = h('ul', { class: 'ocr-status', 'aria-live': 'polite' }, h('li', {}, t('ocr.statusLoading')));
  const installLink = h('a', { href: '#', class: 'about-link', hidden: true }, t('ocr.tesseractInstall'));
  installLink.addEventListener('click', (ev) => {
    ev.preventDefault();
    setDialogError(errorLine, null);
    openTesseractPage().catch(onError);
  });
  const installHint = h('small', { class: 'hint', hidden: true }, t('ocr.tesseractInstallHint'));
  const downloadButton = h('button', { class: 'btn btn-small', type: 'button', hidden: true }) as HTMLButtonElement;
  const progress = h('progress', { max: 100, value: 0, 'aria-label': t('settings.ocr') }) as HTMLProgressElement;
  const progressText = h('span', { class: 'ocr-progress-text' });
  const stopButton = h('button', { class: 'btn btn-quiet btn-small', type: 'button' }, t('ocr.downloadCancel'));
  const progressRow = h('div', { class: 'ocr-progress', hidden: true }, progress, progressText, stopButton);

  let status: OcrStatus | null = null;
  let downloading = false;
  const resolvedOcrLanguage = (): 'tr' | 'en' | 'auto' => (ocrLanguage.value === 'auto' ? getLang() : (ocrLanguage.value as 'tr' | 'en'));
  const renderStatus = (): void => {
    if (!status) return;
    statusList.replaceChildren(...ocrStatusLines(status, resolvedOcrLanguage()).map((line) => h('li', {}, line)));
    // Tesseract is optional; on Windows the built-in OCR covers it, so no install prompt there.
    const showInstall = !status.windowsBuilt && !(status.tesseractPath !== null && status.tesseractLanguages.length > 0);
    installLink.hidden = !showInstall;
    installHint.hidden = !showInstall;
    const needsModel = status.ocrsBuilt && !status.ocrsModels;
    downloadButton.hidden = !needsModel || downloading;
    progressRow.hidden = !downloading;
    setText(downloadButton, t('ocr.download', { size: formatMegabytes(status.ocrsDownloadBytes, getLang()) }));
    // The Windows engine cannot be chosen where it is not built in.
    (engine.querySelector('option[value="windows"]') as HTMLOptionElement).disabled = !status.windowsBuilt;
    if (!status.windowsBuilt && engine.value === 'windows') {
      engine.value = 'auto';
      refreshTesseractField();
    }
  };
  let statusSeq = 0;
  let statusTimer: ReturnType<typeof setTimeout> | undefined;
  const draftOcr = (): OcrSettings => ({
    enabled: ocrEnabled.checked,
    engine: engine.value as OcrEngineChoice,
    language: ocrLanguage.value as OcrLanguage,
    tesseractPath: draft.ocr.tesseractPath,
  });
  /** Asks for the status of the dialog's current (unsaved) choices; a newer call makes older answers stale. */
  const refreshStatus = (): void => {
    const seq = ++statusSeq;
    ocrStatus(draftOcr())
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
  const refreshStatusSoon = (): void => {
    clearTimeout(statusTimer);
    statusTimer = setTimeout(refreshStatus, 300);
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
  chooseTesseract.addEventListener('click', async () => {
    setDialogError(errorLine, null);
    try {
      const picked = await pickProgram();
      if (picked) {
        draft.ocr.tesseractPath = picked;
        refreshTesseractField();
        refreshStatusSoon();
      }
    } catch (err) {
      onError(err);
    }
  });
  clearTesseract.addEventListener('click', () => {
    draft.ocr.tesseractPath = null;
    refreshTesseractField();
    refreshStatusSoon();
  });
  engine.addEventListener('change', () => {
    refreshTesseractField();
    refreshStatusSoon();
  });
  ocrLanguage.addEventListener('change', refreshStatusSoon);
  refreshTesseractField();
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
    h('div', { class: 'field field-inline' }, h('label', { for: 'set-ocr-engine' }, t('settings.ocrEngine')), engine),
    tesseractField,
    h('div', { class: 'field field-inline' }, h('label', { for: 'set-ocr-lang' }, t('settings.ocrLanguage')), ocrLanguage),
    h('div', { class: 'field' }, statusList, installLink, installHint, downloadButton, progressRow),
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
    clearTimeout(statusTimer);
    if (downloading) cancelDownload().catch((err: unknown) => console.warn('cancel download failed', err));
    dialog.remove();
  });
  form.addEventListener('submit', async (ev) => {
    ev.preventDefault();
    const n = Math.round(Number(workers.value));
    draft.workers = Number.isFinite(n) ? Math.min(MAX_WORKERS, Math.max(MIN_WORKERS, n)) : settings.workers;
    draft.extractImages = images.checked;
    draft.language = language.value as SettingsLanguage;
    if (engine.selectedOptions[0]?.disabled) engine.value = 'auto';
    draft.ocr = draftOcr();
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
