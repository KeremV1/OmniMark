import { convertFileSrc } from '@tauri-apps/api/core';
import { openUrl } from '@tauri-apps/plugin-opener';
import DOMPurify from 'dompurify';
import { type OutputText, readOutput } from '../api';
import { t } from '../i18n';
import { fileName } from '../platform';
import type { Actions, View } from './context';
import { h, icons, setText, svg } from './dom';
import { assetPath, isRelativeSrc, markdownToHtml } from './markdown';

export interface Preview {
  el: HTMLElement;
  update(view: View): void;
}

/** Markdown → sanitized HTML: `html: false` already escapes raw HTML, DOMPurify is the second guard. */
function renderMarkdown(text: string): string {
  return DOMPurify.sanitize(markdownToHtml(text));
}

type Tab = 'view' | 'source';

export function createPreview(actions: Actions): Preview {
  let path: string | null = null;
  let output: string | null = null;
  let tab: Tab = 'view';
  let loaded: OutputText | null = null;
  let request = 0;

  const title = h('h2', { class: 'preview-title' });
  const tabView = h('button', { class: 'tab', type: 'button', role: 'tab', id: 'tab-view', 'aria-controls': 'preview-body' }, t('preview.view'));
  const tabSource = h('button', { class: 'tab', type: 'button', role: 'tab', id: 'tab-source', 'aria-controls': 'preview-body' }, t('preview.source'));
  const copy = h('button', { class: 'btn btn-small', type: 'button' }, t('action.copy')) as HTMLButtonElement;
  const reveal = h('button', { class: 'btn btn-quiet btn-small', type: 'button' }, t('action.showFile'));
  const close = h(
    'button',
    { class: 'btn-icon', type: 'button', title: `${t('action.close')} (Esc)`, 'aria-label': t('action.close'), onclick: () => actions.closePreview() },
    svg(icons.close),
  );
  const truncated = h('p', { class: 'preview-note', hidden: true }, svg(icons.info), h('span', {}, t('preview.truncated')));
  const body = h('div', { class: 'preview-body', id: 'preview-body', role: 'tabpanel', tabindex: 0 });

  const el = h(
    'aside',
    { class: 'preview', hidden: true, 'aria-label': t('preview.label') },
    h('div', { class: 'preview-head' }, title, close),
    h(
      'div',
      { class: 'preview-bar' },
      h('div', { class: 'tabs', role: 'tablist' }, tabView, tabSource),
      h('div', { class: 'preview-actions' }, reveal, copy),
    ),
    truncated,
    body,
  );

  function setTab(next: Tab): void {
    tab = next;
    tabView.setAttribute('aria-selected', String(tab === 'view'));
    tabSource.setAttribute('aria-selected', String(tab === 'source'));
    tabView.tabIndex = tab === 'view' ? 0 : -1;
    tabSource.tabIndex = tab === 'source' ? 0 : -1;
    body.setAttribute('aria-labelledby', tab === 'view' ? 'tab-view' : 'tab-source');
    paint();
  }

  function paint(): void {
    truncated.hidden = !loaded?.truncated;
    copy.disabled = !loaded;
    if (!loaded || !output) {
      body.replaceChildren();
      return;
    }
    if (loaded.text.trim() === '') {
      body.replaceChildren(h('p', { class: 'muted' }, t('preview.empty')));
      return;
    }
    if (tab === 'source') {
      body.replaceChildren(h('pre', { class: 'source' }, loaded.text));
      return;
    }
    const article = h('article', { class: 'markdown' });
    // Sanitized by DOMPurify inside renderMarkdown.
    article.innerHTML = renderMarkdown(loaded.text);
    for (const img of Array.from(article.querySelectorAll('img'))) {
      const src = img.getAttribute('src') ?? '';
      if (isRelativeSrc(src)) img.src = convertFileSrc(assetPath(output, src));
      img.loading = 'lazy';
    }
    body.replaceChildren(article);
  }

  async function load(nextOutput: string): Promise<void> {
    const id = ++request;
    loaded = null;
    paint();
    try {
      const text = await readOutput(nextOutput);
      if (id !== request) return;
      loaded = text;
      paint();
    } catch (err) {
      if (id === request) actions.fail(err);
    }
  }

  tabView.addEventListener('click', () => setTab('view'));
  tabSource.addEventListener('click', () => setTab('source'));
  for (const tb of [tabView, tabSource]) {
    tb.addEventListener('keydown', (ev) => {
      if (ev.key === 'ArrowLeft' || ev.key === 'ArrowRight') {
        ev.preventDefault();
        setTab(tab === 'view' ? 'source' : 'view');
        (tab === 'view' ? tabView : tabSource).focus();
      }
    });
  }
  reveal.addEventListener('click', () => output && actions.reveal(output));
  copy.addEventListener('click', async () => {
    if (!loaded) return;
    try {
      await navigator.clipboard.writeText(loaded.text);
      setText(copy, t('action.copied'));
      window.setTimeout(() => setText(copy, t('action.copy')), 1500);
    } catch (err) {
      actions.fail(err);
    }
  });
  // Links never navigate the app window: web and mail links open in the default app, others are ignored.
  body.addEventListener('click', (ev) => {
    const a = (ev.target as HTMLElement).closest('a');
    if (!a) return;
    ev.preventDefault();
    const href = a.getAttribute('href') ?? '';
    if (/^(https?:|mailto:)/i.test(href)) openUrl(href).catch((err: unknown) => actions.fail(err));
  });

  setTab('view');

  return {
    el,
    update(view) {
      const item = view.preview ? view.queue.items.find((i) => i.path === view.preview) : undefined;
      const nextOutput = item?.status === 'done' && item.output ? item.output : null;
      el.hidden = nextOutput === null;
      if (nextOutput === null) {
        path = null;
        output = null;
        loaded = null;
        request++;
        return;
      }
      if (item && (item.path !== path || nextOutput !== output)) {
        path = item.path;
        output = nextOutput;
        setText(title, fileName(nextOutput));
        title.title = nextOutput;
        setTab('view');
        void load(nextOutput);
      }
    },
  };
}
