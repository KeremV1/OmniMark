import { describe, expect, it, vi } from 'vitest';
import { bindMenuDismissal, deferredCompact } from './list';

/** Counts live listeners per event type, honouring `{ signal }` removal. */
class CountingTarget extends EventTarget {
  live = new Map<string, number>();
  override addEventListener(type: string, cb: EventListenerOrEventListenerObject | null, opts?: AddEventListenerOptions | boolean): void {
    super.addEventListener(type, cb, opts);
    this.live.set(type, (this.live.get(type) ?? 0) + 1);
    const signal = typeof opts === 'object' ? opts.signal : undefined;
    signal?.addEventListener('abort', () => this.live.set(type, (this.live.get(type) ?? 1) - 1));
  }
}

const node = {} as Node;

describe('row menu global listeners', () => {
  it('a re-mount leaves only the current mount listening', () => {
    const doc = new CountingTarget();
    const win = new CountingTarget();
    const first = { pointerdown: vi.fn(), resize: vi.fn(), scroll: vi.fn() };
    const second = { pointerdown: vi.fn(), resize: vi.fn(), scroll: vi.fn() };

    const mount1 = new AbortController();
    bindMenuDismissal(doc, win, mount1.signal, first);
    // What mount() does on a language change: destroy the old list, build a new one.
    mount1.abort();
    const mount2 = new AbortController();
    bindMenuDismissal(doc, win, mount2.signal, second);

    expect(Object.fromEntries(doc.live)).toEqual({ pointerdown: 1, scroll: 1 });
    expect(Object.fromEntries(win.live)).toEqual({ resize: 1 });

    const pointer = new Event('pointerdown');
    Object.defineProperty(pointer, 'target', { value: node });
    doc.dispatchEvent(pointer);
    doc.dispatchEvent(new Event('scroll'));
    win.dispatchEvent(new Event('resize'));
    expect(first.pointerdown).not.toHaveBeenCalled();
    expect(first.scroll).not.toHaveBeenCalled();
    expect(first.resize).not.toHaveBeenCalled();
    expect(second.pointerdown).toHaveBeenCalledTimes(1);
    expect(second.scroll).toHaveBeenCalledTimes(1);
    expect(second.resize).toHaveBeenCalledTimes(1);

    mount2.abort();
    expect([...doc.live.values(), ...win.live.values()].every((n) => n === 0)).toBe(true);
  });
});

describe('compact toggle', () => {
  function fake(initial: boolean) {
    const set = new Set<string>(initial ? ['compact'] : []);
    const toggle = vi.fn((name: string, force?: boolean) => (force ? set.add(name) : set.delete(name)));
    const el = { classList: { contains: (n: string) => set.has(n), toggle } } as unknown as Pick<HTMLElement, 'classList'>;
    const frames: Array<() => void> = [];
    const raf = (cb: () => void) => frames.push(cb);
    return { el, toggle, frames, raf };
  }

  it('does not touch the class inside the observer callback, only in the next frame', () => {
    const f = fake(false);
    const c = deferredCompact(f.el, () => 400, f.raf, vi.fn());
    c.schedule();
    c.schedule(); // coalesced
    expect(f.toggle).not.toHaveBeenCalled();
    expect(f.frames).toHaveLength(1);
    f.frames[0]!();
    expect(f.toggle).toHaveBeenCalledWith('compact', true);
  });

  it('skips the toggle when the value does not change', () => {
    const f = fake(true);
    const c = deferredCompact(f.el, () => 400, f.raf, vi.fn());
    c.schedule();
    f.frames[0]!();
    expect(f.toggle).not.toHaveBeenCalled();
  });
});
