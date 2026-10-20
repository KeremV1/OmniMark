import {
  type AppError,
  cancel,
  convert,
  getSettings,
  inspectFiles,
  openOutputDir,
  pickFiles,
  pickFolder,
  reveal,
  type Settings,
  setSettings,
} from './api';
import { hasMessage, loadLang, type MessageKey, resolveLang, t } from './i18n';
import {
  addEntries,
  applyEvent,
  beginBatch,
  convertiblePaths,
  emptyQueue,
  endBatch,
  failBatch,
  removablePaths,
  removePaths,
  setAllSelected,
  setFilter,
  setSelected,
} from './queue';
import type { Actions, View } from './ui/context';
import { openDetailsDialog } from './ui/details';
import { createDropzone, listenDragDrop } from './ui/dropzone';
import { createFooter } from './ui/footer';
import { createList } from './ui/list';
import { createPreview } from './ui/preview';
import { openSettingsDialog } from './ui/settings';
import { installShortcuts } from './ui/shortcuts';
import { createToolbar } from './ui/toolbar';

/** Most files one folder drop may add (mirrors the backend limit). */
const FOLDER_FILE_LIMIT = 1000;

const FALLBACK_SETTINGS: Settings = {
  outputDir: '',
  extractImages: true,
  workers: 2,
  language: 'auto',
  ocr: { enabled: false },
  zipOutput: 'folder',
};

let view: View = {
  queue: emptyQueue(),
  settings: FALLBACK_SETTINGS,
  preview: null,
  notice: null,
  dragging: false,
};

function isAppError(e: unknown): e is AppError {
  return typeof e === 'object' && e !== null && 'kind' in e && 'message' in e;
}

function errorText(e: unknown): string {
  if (isAppError(e)) {
    const key = `error.${e.kind}` as MessageKey;
    return hasMessage(key) ? t(key) : t('error.generic');
  }
  return t('error.generic');
}

/** Loads the chosen locale (English until it arrives or if it fails) and tags the document. */
async function applyLanguage(settings: Settings): Promise<void> {
  const systemLanguages = navigator.languages?.length ? navigator.languages : [navigator.language];
  const lang = await loadLang(resolveLang(settings.language, systemLanguages));
  document.documentElement.lang = lang;
}

type Ui = { update(view: View): void; listHasFocus(): boolean; closeMenu(): boolean; destroy(): void };
let ui: Ui | null = null;

function setView(next: Partial<View>): void {
  view = { ...view, ...next };
  ui?.update(view);
}

function addPaths(paths: string[]): void {
  inspectFiles(paths)
    .then(({ entries, truncated }) => {
      setView({
        queue: addEntries(view.queue, entries),
        notice: truncated ? { tone: 'info', text: t('notice.truncated', { n: FOLDER_FILE_LIMIT }) } : view.notice,
      });
    })
    .catch(actions.fail);
}

function convertPaths(paths: string[]): void {
  if (paths.length === 0 || view.queue.running) return;
  setView({ queue: beginBatch(view.queue, paths), notice: null });
  convert(paths, (event) => setView({ queue: applyEvent(view.queue, event) }))
    .then(() => setView({ queue: endBatch(view.queue) }))
    .catch((err: unknown) => {
      const error: AppError = isAppError(err) ? err : { kind: 'io', message: String(err) };
      setView({ queue: failBatch(view.queue, error), notice: { tone: 'error', text: errorText(err) } });
    });
}

const actions: Actions = {
  addFiles() {
    pickFiles(t('dialog.allFiles'))
      .then((paths) => paths.length > 0 && addPaths(paths))
      .catch(actions.fail);
  },
  addPaths,
  removeSelected() {
    const paths = removablePaths(view.queue);
    const queue = removePaths(view.queue, paths);
    setView({ queue, preview: view.preview && paths.includes(view.preview) ? null : view.preview });
  },
  removePath(path) {
    setView({ queue: removePaths(view.queue, [path]), preview: view.preview === path ? null : view.preview });
  },
  setSelected(path, value) {
    setView({ queue: setSelected(view.queue, path, value) });
  },
  setAllSelected(value) {
    setView({ queue: setAllSelected(view.queue, value) });
  },
  setFilter(filter) {
    setView({ queue: setFilter(view.queue, filter) });
  },
  convertSelected() {
    convertPaths(convertiblePaths(view.queue));
  },
  convertPaths,
  cancel() {
    cancel().catch(actions.fail);
  },
  openPreview(path) {
    setView({ preview: path });
  },
  closePreview() {
    setView({ preview: null });
  },
  showDetails(path) {
    const item = view.queue.items.find((i) => i.path === path);
    if (item) openDetailsDialog(item);
  },
  reveal(path) {
    reveal(path).catch(actions.fail);
  },
  changeOutputDir() {
    pickFolder()
      .then((dir) => (dir ? saveSettings({ ...view.settings, outputDir: dir }) : undefined))
      .catch(actions.fail);
  },
  openOutputDir() {
    openOutputDir().catch(actions.fail);
  },
  openSettings() {
    openSettingsDialog({ settings: view.settings, onSave: saveSettings, describeError: errorText });
  },
  dismissNotice() {
    setView({ notice: null });
  },
  setDragging(value) {
    if (view.dragging !== value) setView({ dragging: value });
  },
  fail(error) {
    console.error(error);
    setView({ notice: { tone: 'error', text: errorText(error) } });
  },
};

async function saveSettings(next: Settings): Promise<void> {
  const stored = await setSettings(next);
  const languageChanged = stored.language !== view.settings.language;
  view = { ...view, settings: stored };
  if (languageChanged) {
    await applyLanguage(stored);
    mount();
  } else {
    ui?.update(view);
  }
}

function createNotice() {
  const text = document.createElement('span');
  const close = document.createElement('button');
  close.type = 'button';
  close.className = 'btn-icon btn-icon-small';
  close.textContent = '×';
  close.addEventListener('click', () => actions.dismissNotice());
  const el = document.createElement('div');
  el.className = 'notice';
  el.setAttribute('role', 'status');
  el.append(text, close);
  return {
    el,
    update(v: View) {
      el.hidden = v.notice === null;
      if (v.notice) {
        el.dataset.tone = v.notice.tone;
        text.textContent = v.notice.text;
        close.setAttribute('aria-label', t('action.close'));
        close.title = t('action.close');
      }
    },
  };
}

/** Builds (or rebuilds, after a language change) the whole window from `view`. */
function mount(): void {
  const app = document.querySelector<HTMLElement>('#app');
  if (!app) return;
  // Tear down the previous window's global listeners and observers before rebuilding.
  ui?.destroy();

  const toolbar = createToolbar(actions);
  const notice = createNotice();
  const list = createList(actions);
  const dropzone = createDropzone(actions);
  const preview = createPreview(actions);
  const footer = createFooter(actions);

  const pane = document.createElement('div');
  pane.className = 'pane';
  pane.append(list.el, dropzone.el, dropzone.overlay);
  const content = document.createElement('main');
  content.className = 'content';
  content.append(pane, preview.el);

  app.replaceChildren(toolbar.el, notice.el, content, footer.el);
  document.title = t('app.title');

  ui = {
    update(v) {
      document.body.classList.toggle('is-dragging', v.dragging);
      list.el.hidden = v.queue.items.length === 0;
      toolbar.update(v);
      notice.update(v);
      list.update(v);
      dropzone.update(v);
      preview.update(v);
      footer.update(v);
    },
    listHasFocus: () => list.hasFocus(),
    closeMenu: () => list.closeMenu(),
    destroy: () => list.destroy(),
  };
  ui.update(view);
}

async function boot(): Promise<void> {
  let scenario: ((a: Actions) => void) | null = null;
  if (import.meta.env.DEV && !('__TAURI_INTERNALS__' in window)) {
    const mock = await import('./dev-mock');
    mock.installDevMock();
    scenario = mock.runScenario;
  }

  try {
    view = { ...view, settings: await getSettings() };
  } catch (err) {
    console.error(err);
  }
  await applyLanguage(view.settings);
  mount();

  installShortcuts({
    addFiles: () => actions.addFiles(),
    convert: () => actions.convertSelected(),
    removeSelected: () => {
      if (!ui?.listHasFocus()) return false;
      actions.removeSelected();
      return true;
    },
    escape: () => {
      if (ui?.closeMenu()) return true;
      if (view.preview) {
        actions.closePreview();
        return true;
      }
      return false;
    },
  });

  listenDragDrop(actions).catch((err: unknown) => console.error('drag and drop unavailable', err));
  scenario?.(actions);
}

void boot();
