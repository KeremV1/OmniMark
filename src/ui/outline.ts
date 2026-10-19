import type { OutlineHeading } from '../api';
import { t } from '../i18n';
import { h } from './dom';

export interface HeadingRef {
  level: number;
  text: string;
}

/** Plain, comparable form of a heading: inline Markdown marks and link targets removed. */
export function normalizeHeading(text: string): string {
  return text
    .replace(/\]\([^)]*\)/g, '')
    .replace(/[*_`~[\]]/g, '')
    .replace(/\s+/g, ' ')
    .trim()
    .toLowerCase();
}

/**
 * For each outline heading, the index of the rendered heading it points at (or `null`).
 * Both lists are in document order, so matching is a single forward pass; rendered headings
 * the outline does not know about (setext headings) are skipped.
 */
export function matchHeadings(outline: HeadingRef[], rendered: HeadingRef[]): (number | null)[] {
  const renderedKeys = rendered.map((r) => normalizeHeading(r.text));
  let next = 0;
  return outline.map((item) => {
    const key = normalizeHeading(item.text);
    for (let k = next; k < rendered.length; k++) {
      const other = renderedKeys[k];
      if (rendered[k].level === item.level && (key.startsWith(other) || other.startsWith(key))) {
        next = k + 1;
        return k;
      }
    }
    return null;
  });
}

/** Collapsed by default when the outline is longer than this. */
export const OUTLINE_OPEN_MAX = 20;

export interface Outline {
  el: HTMLElement;
  /** Shows the outline; `jump(i)` is called for the i-th entry when a clickable one is activated. */
  set(headings: OutlineHeading[], more: boolean, jump: (index: number) => void): void;
  /** Disables entries without a rendered counterpart (the Kaynak tab has none). */
  setActive(active: boolean): void;
  clear(): void;
}

let counter = 0;

export function createOutline(): Outline {
  const listId = `outline-list-${counter++}`;
  const toggle = h('button', { class: 'outline-toggle', type: 'button', 'aria-expanded': 'false', 'aria-controls': listId }, '');
  const list = h('ul', { class: 'outline-list', id: listId });
  const el = h('nav', { class: 'outline', 'aria-label': t('preview.outline'), hidden: true }, toggle, list);
  let open = false;

  function setOpen(next: boolean): void {
    open = next;
    toggle.setAttribute('aria-expanded', String(open));
    list.hidden = !open;
  }
  toggle.addEventListener('click', () => setOpen(!open));

  return {
    el,
    set(headings, more, jump) {
      if (headings.length === 0) {
        this.clear();
        return;
      }
      toggle.textContent = `${t('preview.outline')} (${headings.length}${more ? '+' : ''})`;
      const base = Math.min(...headings.map((x) => x.level));
      const items = headings.map((item, index) => {
        const label = h('span', {}, item.text);
        const entry = item.inPreview
          ? h('button', { class: 'outline-link', type: 'button', onclick: () => jump(index) }, label)
          : h('span', { class: 'outline-dim', title: t('preview.outlineBeyond') }, label);
        const li = h('li', { class: 'outline-item' }, entry);
        li.style.paddingLeft = `${(item.level - base) * 14}px`;
        return li;
      });
      if (more) items.push(h('li', { class: 'outline-more' }, t('preview.outlineMore')));
      list.replaceChildren(...items);
      el.hidden = false;
      setOpen(headings.length <= OUTLINE_OPEN_MAX);
    },
    setActive(active) {
      for (const b of Array.from(list.querySelectorAll<HTMLButtonElement>('.outline-link'))) b.disabled = !active;
    },
    clear() {
      list.replaceChildren();
      el.hidden = true;
    },
  };
}
