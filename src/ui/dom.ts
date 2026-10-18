/** Tiny DOM helpers shared by the UI modules. No framework, no innerHTML for user data. */

type Child = Node | string | null | undefined | false;
type Attrs = Record<string, string | number | boolean | null | undefined | ((ev: Event) => void)>;

/**
 * `h('button', { class: 'btn', onclick: fn, disabled: true }, 'Label')`.
 * `on*` keys become listeners, booleans toggle the attribute, `null`/`undefined`/`false` skip it.
 */
export function h<K extends keyof HTMLElementTagNameMap>(tag: K, attrs: Attrs = {}, ...children: Child[]): HTMLElementTagNameMap[K] {
  const el = document.createElement(tag);
  for (const [key, value] of Object.entries(attrs)) {
    if (value === null || value === undefined || value === false) continue;
    if (typeof value === 'function') {
      el.addEventListener(key.slice(2), value);
    } else if (value === true) {
      el.setAttribute(key, '');
    } else {
      el.setAttribute(key, String(value));
    }
  }
  append(el, children);
  return el;
}

export function append(el: Element, children: Child[]): void {
  for (const c of children) {
    if (c === null || c === undefined || c === false) continue;
    el.append(typeof c === 'string' ? document.createTextNode(c) : c);
  }
}

/** Static, trusted SVG markup (icons, the bundled logo) to an element. Never pass user data. */
export function svg(markup: string, className?: string): Element {
  const tpl = document.createElement('template');
  tpl.innerHTML = markup.trim();
  const node = tpl.content.firstElementChild as Element;
  node.setAttribute('aria-hidden', 'true');
  node.setAttribute('focusable', 'false');
  if (className) node.setAttribute('class', className);
  return node;
}

export function setText(el: Element, text: string): void {
  if (el.textContent !== text) el.textContent = text;
}

export function toggleAttr(el: Element, name: string, on: boolean): void {
  if (on) {
    if (!el.hasAttribute(name)) el.setAttribute(name, '');
  } else if (el.hasAttribute(name)) {
    el.removeAttribute(name);
  }
}

const stroke = 'fill="none" stroke="currentColor" stroke-width="1.6" stroke-linecap="round" stroke-linejoin="round"';

export const icons = {
  plus: `<svg viewBox="0 0 16 16" width="16" height="16" ${stroke}><path d="M8 3.5v9M3.5 8h9"/></svg>`,
  // Cog outline from Feather Icons (MIT).
  gear: `<svg viewBox="0 0 24 24" width="18" height="18" ${stroke} stroke-width="1.8"><circle cx="12" cy="12" r="3"/><path d="M19.4 15a1.65 1.65 0 0 0 .33 1.82l.06.06a2 2 0 0 1 0 2.83 2 2 0 0 1-2.83 0l-.06-.06a1.65 1.65 0 0 0-1.82-.33 1.65 1.65 0 0 0-1 1.51V21a2 2 0 0 1-2 2 2 2 0 0 1-2-2v-.09A1.65 1.65 0 0 0 9 19.4a1.65 1.65 0 0 0-1.82.33l-.06.06a2 2 0 0 1-2.83 0 2 2 0 0 1 0-2.83l.06-.06a1.65 1.65 0 0 0 .33-1.82 1.65 1.65 0 0 0-1.51-1H3a2 2 0 0 1-2-2 2 2 0 0 1 2-2h.09A1.65 1.65 0 0 0 4.6 9a1.65 1.65 0 0 0-.33-1.82l-.06-.06a2 2 0 0 1 0-2.83 2 2 0 0 1 2.83 0l.06.06a1.65 1.65 0 0 0 1.82.33H9a1.65 1.65 0 0 0 1-1.51V3a2 2 0 0 1 2-2 2 2 0 0 1 2 2v.09a1.65 1.65 0 0 0 1 1.51 1.65 1.65 0 0 0 1.82-.33l.06-.06a2 2 0 0 1 2.83 0 2 2 0 0 1 0 2.83l-.06.06a1.65 1.65 0 0 0-.33 1.82V9a1.65 1.65 0 0 0 1.51 1H21a2 2 0 0 1 2 2 2 2 0 0 1-2 2h-.09a1.65 1.65 0 0 0-1.51 1z"/></svg>`,
  more: `<svg viewBox="0 0 16 16" width="16" height="16" fill="currentColor"><circle cx="3.5" cy="8" r="1.3"/><circle cx="8" cy="8" r="1.3"/><circle cx="12.5" cy="8" r="1.3"/></svg>`,
  close: `<svg viewBox="0 0 16 16" width="16" height="16" ${stroke}><path d="M4 4l8 8M12 4l-8 8"/></svg>`,
  check: `<svg viewBox="0 0 16 16" width="14" height="14" ${stroke} stroke-width="2"><path d="M3.5 8.5l3 3 6-7"/></svg>`,
  cross: `<svg viewBox="0 0 16 16" width="14" height="14" ${stroke} stroke-width="2"><path d="M4.5 4.5l7 7M11.5 4.5l-7 7"/></svg>`,
  info: `<svg viewBox="0 0 16 16" width="16" height="16" ${stroke}><circle cx="8" cy="8" r="6.2"/><path d="M8 7.2v4M8 4.9v.1"/></svg>`,
  folder: `<svg viewBox="0 0 16 16" width="16" height="16" ${stroke}><path d="M2 4.5c0-.6.4-1 1-1h3l1.5 1.5H13c.6 0 1 .4 1 1V12c0 .6-.4 1-1 1H3c-.6 0-1-.4-1-1z"/></svg>`,
};
