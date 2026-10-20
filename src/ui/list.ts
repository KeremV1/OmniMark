import { getLang, type MessageKey, t, warningsLabel } from '../i18n';
import { formatBytes } from '../platform';
import { type QueueItem, selectionState, visibleItems } from '../queue';
import type { Actions, View } from './context';
import { h, icons, setText, svg, toggleAttr } from './dom';

export interface List {
  el: HTMLElement;
  update(view: View): void;
  /** True while keyboard focus is inside the list (Delete removes the selection then). */
  hasFocus(): boolean;
  /** Closes the row menu; returns whether one was open. */
  closeMenu(): boolean;
  /** Removes the global listeners, the ResizeObserver and the menu element. */
  destroy(): void;
}

interface Row {
  el: HTMLElement;
  check: HTMLInputElement;
  name: HTMLElement;
  kind: HTMLElement;
  size: HTMLElement;
  status: HTMLElement;
  primary: HTMLButtonElement;
  more: HTMLButtonElement;
  /** Signature of the last rendered status cell, to skip needless rebuilds. */
  statusKey: string;
  bar?: { track: HTMLElement; fill: HTMLElement; pct: HTMLElement };
}

/** Width below which the type and size columns fold away (narrow window or open preview). */
const COMPACT_WIDTH = 600;

/**
 * Toggles `compact` outside the ResizeObserver callback: changing layout inside it makes the browser
 * report "ResizeObserver loop completed with undelivered notifications". Skips unchanged values.
 */
export function deferredCompact(
  el: Pick<HTMLElement, 'classList'>,
  width: () => number,
  raf: (cb: () => void) => number,
  caf: (id: number) => void,
): { schedule(): void; cancel(): void } {
  let pending: number | null = null;
  return {
    schedule() {
      if (pending !== null) return;
      pending = raf(() => {
        pending = null;
        const compact = width() < COMPACT_WIDTH;
        if (el.classList.contains('compact') !== compact) el.classList.toggle('compact', compact);
      });
    },
    cancel() {
      if (pending !== null) caf(pending);
      pending = null;
    },
  };
}

export function createList(actions: Actions): List {
  const selectAll = h('input', { type: 'checkbox', 'aria-label': t('list.selectAll') }) as HTMLInputElement;
  selectAll.addEventListener('change', () => actions.setAllSelected(selectAll.checked));

  const header = h(
    'div',
    { class: 'row row-head', role: 'row' },
    h('div', { class: 'cell cell-check', role: 'columnheader' }, selectAll),
    h('div', { class: 'cell cell-name', role: 'columnheader' }, t('list.name')),
    h('div', { class: 'cell cell-kind', role: 'columnheader' }, t('list.kind')),
    h('div', { class: 'cell cell-size', role: 'columnheader' }, t('list.size')),
    h('div', { class: 'cell cell-status', role: 'columnheader' }, t('list.status')),
    h('div', { class: 'cell cell-actions', role: 'columnheader' }),
  );
  const body = h('div', { class: 'rows', role: 'rowgroup' });
  const noMatch = h('p', { class: 'no-match', hidden: true }, t('list.noMatch'));
  const grid = h(
    'div',
    { class: 'grid', role: 'grid', 'aria-label': t('list.label'), 'aria-multiselectable': 'true' },
    h('div', { role: 'rowgroup', class: 'head-group' }, header),
    body,
  );
  const el = h('section', { class: 'list' }, grid, noMatch);

  // Everything global this list installs is torn down by `destroy()` (on re-mount).
  const lifetime = new AbortController();
  const applyCompact = deferredCompact(el, () => el.clientWidth, requestAnimationFrame, cancelAnimationFrame);
  const resize = new ResizeObserver(applyCompact.schedule);
  resize.observe(el);
  lifetime.signal.addEventListener('abort', () => {
    resize.disconnect();
    applyCompact.cancel();
  });

  const rows = new Map<string, Row>();
  let current: View | null = null;
  const menu = createMenu(actions, () => current, lifetime.signal);

  function itemOf(path: string): QueueItem | undefined {
    return current?.queue.items.find((i) => i.path === path);
  }

  function createRow(item: QueueItem): Row {
    const check = h('input', { type: 'checkbox', 'aria-label': t('list.select', { name: item.name }) }) as HTMLInputElement;
    check.addEventListener('change', () => actions.setSelected(item.path, check.checked));
    const primary = h('button', { class: 'btn-link', type: 'button' }) as HTMLButtonElement;
    primary.addEventListener('click', () => {
      const it = itemOf(item.path);
      if (it?.status === 'done') actions.openPreview(it.path);
      else if (it?.status === 'failed') actions.showDetails(it.path);
    });
    const more = h(
      'button',
      { class: 'btn-icon btn-icon-small', type: 'button', title: t('action.more'), 'aria-label': t('action.more'), 'aria-haspopup': 'menu' },
      svg(icons.more),
    ) as HTMLButtonElement;
    more.addEventListener('click', (ev) => {
      ev.stopPropagation();
      menu.toggle(item.path, more);
    });
    const row: Row = {
      el: h('div', { class: 'row', role: 'row' }),
      check,
      name: h('div', { class: 'cell cell-name', role: 'gridcell' }),
      kind: h('div', { class: 'cell cell-kind', role: 'gridcell' }),
      size: h('div', { class: 'cell cell-size', role: 'gridcell' }),
      status: h('div', { class: 'cell cell-status', role: 'gridcell' }),
      primary,
      more,
      statusKey: '',
    };
    row.el.append(
      h('div', { class: 'cell cell-check', role: 'gridcell' }, check),
      row.name,
      row.kind,
      row.size,
      row.status,
      h('div', { class: 'cell cell-actions', role: 'gridcell' }, primary, more),
    );
    // A click on the row body toggles the selection, like a file manager.
    row.el.addEventListener('click', (ev) => {
      const target = ev.target as HTMLElement;
      if (target.closest('button, input')) return;
      const it = itemOf(item.path);
      if (it && it.status !== 'unsupported') actions.setSelected(it.path, !it.selected);
    });
    return row;
  }

  function updateRow(row: Row, item: QueueItem): void {
    row.el.className = `row is-${item.status}`;
    row.el.setAttribute('aria-selected', String(item.selected));
    row.check.checked = item.selected;
    row.check.disabled = item.status === 'unsupported';
    setText(row.name, item.name);
    row.name.title = item.path;
    setText(row.kind, t(`kind.${item.kind}` as MessageKey));
    setText(row.size, formatBytes(item.size, getLang()));
    renderStatus(row, item);

    const primaryKey = item.status === 'done' ? 'action.preview' : item.status === 'failed' ? 'action.details' : null;
    row.primary.hidden = primaryKey === null;
    if (primaryKey) setText(row.primary, t(primaryKey));
  }

  function renderStatus(row: Row, item: QueueItem): void {
    if (item.status === 'converting') {
      if (row.statusKey !== 'converting' || !row.bar) {
        const fill = h('span', { class: 'bar-fill' });
        const track = h(
          'span',
          { class: 'bar', role: 'progressbar', 'aria-valuemin': 0, 'aria-valuemax': 100, 'aria-label': `${t('status.converting')}: ${item.name}` },
          fill,
        );
        const pct = h('span', { class: 'pct' });
        const stop = h(
          'button',
          { class: 'btn-icon btn-icon-small row-cancel', type: 'button', title: t('action.cancelRun'), 'aria-label': t('action.cancelRun') },
          svg(icons.cross),
        );
        stop.addEventListener('click', (ev) => {
          ev.stopPropagation();
          actions.cancel();
        });
        row.status.replaceChildren(track, pct, stop);
        row.bar = { track, fill, pct };
        row.statusKey = 'converting';
      }
      const n = Math.round(Math.min(1, Math.max(0, item.progress)) * 100);
      row.bar.track.setAttribute('aria-valuenow', String(n));
      // CSSOM writes are allowed under the `style-src 'self'` CSP; style attributes are not.
      row.bar.fill.style.transform = `scaleX(${n / 100})`;
      setText(row.bar.pct, t('status.percent', { n }));
      return;
    }
    row.bar = undefined;
    const key = `${item.status}|${item.error?.kind ?? ''}|${item.warnings.length}|${getLang()}`;
    if (key === row.statusKey) return;
    row.statusKey = key;
    switch (item.status) {
      case 'pending':
        row.status.replaceChildren(h('span', { class: 'muted' }, t('status.pending')));
        break;
      case 'cancelled':
        row.status.replaceChildren(h('span', { class: 'muted' }, t('status.cancelled')));
        break;
      case 'unsupported':
        row.status.replaceChildren(h('span', { class: 'muted', title: t('status.unsupportedLater') }, t('status.unsupportedLater')));
        break;
      case 'done':
        row.status.replaceChildren(h('span', { class: 'state state-done' }, svg(icons.check, 'state-icon'), t('status.done')));
        if (item.warnings.length > 0) {
          row.status.append(h('span', { class: 'warn-count' }, warningsLabel(item.warnings.length)));
        }
        break;
      case 'failed': {
        const summary = item.error ? t(`error.${item.error.kind}` as MessageKey) : t('status.failed');
        row.status.replaceChildren(
          h('span', { class: 'state state-failed', title: summary }, svg(icons.cross, 'state-icon'), h('span', { class: 'state-text' }, summary)),
        );
        break;
      }
    }
  }

  return {
    el,
    update(view) {
      current = view;
      const items = visibleItems(view.queue);
      const seen = new Set<string>();
      let prev: Element | null = null;
      for (const item of items) {
        seen.add(item.path);
        let row = rows.get(item.path);
        if (!row) {
          row = createRow(item);
          rows.set(item.path, row);
        }
        updateRow(row, item);
        const expected: Element | null = prev ? prev.nextElementSibling : body.firstElementChild;
        if (expected !== row.el) body.insertBefore(row.el, expected);
        prev = row.el;
      }
      for (const [path, row] of rows) {
        if (!seen.has(path)) {
          row.el.remove();
          if (!view.queue.items.some((i) => i.path === path)) rows.delete(path);
        }
      }
      const sel = selectionState(view.queue);
      selectAll.checked = sel === 'all';
      selectAll.indeterminate = sel === 'some';
      selectAll.disabled = !view.queue.items.some((i) => i.status !== 'unsupported');
      toggleAttr(noMatch, 'hidden', items.length > 0);
      grid.hidden = items.length === 0;
      menu.refresh();
    },
    hasFocus() {
      return el.contains(document.activeElement);
    },
    closeMenu() {
      return menu.close();
    },
    destroy() {
      lifetime.abort();
    },
  };
}

interface Menu {
  toggle(path: string, anchor: HTMLElement): void;
  refresh(): void;
  close(): boolean;
}

/** The row `⋯` menu: one shared popover, keyboard navigable. */
export interface MenuDismissal {
  pointerdown(target: Node): void;
  resize(): void;
  scroll(target: Node): void;
}

/**
 * Window-wide listeners of the row menu. They live until `signal` aborts, so a re-mount
 * (language change) does not leave dead listeners holding the old DOM. Exported for tests.
 */
export function bindMenuDismissal(doc: EventTarget, win: EventTarget, signal: AbortSignal, on: MenuDismissal): void {
  doc.addEventListener('pointerdown', (ev) => on.pointerdown(ev.target as Node), { signal });
  win.addEventListener('resize', () => on.resize(), { signal });
  doc.addEventListener('scroll', (ev) => on.scroll(ev.target as Node), { capture: true, signal });
}

function createMenu(actions: Actions, view: () => View | null, signal: AbortSignal): Menu {
  const showFile = h('button', { class: 'menu-item', type: 'button', role: 'menuitem' }, t('action.showFile')) as HTMLButtonElement;
  const retry = h('button', { class: 'menu-item', type: 'button', role: 'menuitem' }, t('action.retry')) as HTMLButtonElement;
  const remove = h('button', { class: 'menu-item', type: 'button', role: 'menuitem' }, t('action.removeFromList')) as HTMLButtonElement;
  const el = h('div', { class: 'menu', role: 'menu', hidden: true }, showFile, retry, remove);
  document.body.append(el);

  let path: string | null = null;
  let anchor: HTMLElement | null = null;

  const item = () => view()?.queue.items.find((i) => i.path === path);

  function refresh(): void {
    const it = item();
    if (!path) return;
    if (!it) {
      close();
      return;
    }
    const running = view()?.queue.running ?? false;
    showFile.disabled = false;
    retry.disabled = running || it.status === 'unsupported' || it.status === 'converting';
    remove.disabled = it.status === 'converting';
  }

  function place(): void {
    if (!anchor) return;
    const r = anchor.getBoundingClientRect();
    const w = el.offsetWidth;
    const hgt = el.offsetHeight;
    const left = Math.max(8, Math.min(window.innerWidth - w - 8, r.right - w));
    const below = r.bottom + 4;
    const top = below + hgt > window.innerHeight - 8 ? Math.max(8, r.top - hgt - 4) : below;
    el.style.left = `${left}px`;
    el.style.top = `${top}px`;
  }

  function enabledItems(): HTMLButtonElement[] {
    return [showFile, retry, remove].filter((b) => !b.disabled);
  }

  function open(p: string, a: HTMLElement): void {
    path = p;
    anchor = a;
    a.setAttribute('aria-expanded', 'true');
    el.hidden = false;
    refresh();
    place();
    enabledItems()[0]?.focus({ preventScroll: true });
  }

  function close(): boolean {
    if (path === null) return false;
    el.hidden = true;
    anchor?.removeAttribute('aria-expanded');
    const back = anchor;
    path = null;
    anchor = null;
    if (back && el.contains(document.activeElement)) back.focus();
    return true;
  }

  function run(fn: (p: string) => void): void {
    const p = path;
    const it = item();
    close();
    if (p && it) fn(p);
  }

  // A converted file shows its Markdown output; anything else shows the source file.
  showFile.addEventListener('click', () => {
    const it = item();
    close();
    if (it) actions.reveal(it.status === 'done' && it.output ? it.output : it.path);
  });
  retry.addEventListener('click', () => run((p) => actions.convertPaths([p])));
  remove.addEventListener('click', () => run((p) => actions.removePath(p)));

  el.addEventListener('keydown', (ev) => {
    const items = enabledItems();
    const i = items.indexOf(document.activeElement as HTMLButtonElement);
    if (ev.key === 'ArrowDown') {
      ev.preventDefault();
      items[(i + 1) % items.length]?.focus();
    } else if (ev.key === 'ArrowUp') {
      ev.preventDefault();
      items[(i - 1 + items.length) % items.length]?.focus();
    } else if (ev.key === 'Tab') {
      close();
    }
  });
  bindMenuDismissal(document, window, signal, {
    pointerdown(target) {
      if (path && !el.contains(target) && target !== anchor && !anchor?.contains(target)) close();
    },
    resize: () => place(),
    scroll(target) {
      if (!el.contains(target)) close();
    },
  });
  signal.addEventListener('abort', () => el.remove());

  return {
    toggle(p, a) {
      if (path === p) close();
      else {
        close();
        open(p, a);
      }
    },
    refresh,
    close,
  };
}
