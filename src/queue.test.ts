import { describe, expect, it } from 'vitest';
import { addEntries, applyEvent, beginBatch, convertiblePaths, counts, emptyQueue, removePaths, setFilter, visibleItems } from './queue';
import type { FileEntry } from './api';

const entry = (path: string, supported = true): FileEntry => ({ path, name: path.split('/').pop()!, kind: supported ? 'pdf' : 'image', size: 10, supported });

describe('queue', () => {
  it('adds entries once, unsupported ones unselected', () => {
    let s = addEntries(emptyQueue(), [entry('/a.pdf'), entry('/b.jpg', false)]);
    s = addEntries(s, [entry('/a.pdf')]);
    expect(s.items.map((i) => [i.path, i.status, i.selected])).toEqual([
      ['/a.pdf', 'pending', true],
      ['/b.jpg', 'unsupported', false],
    ]);
    expect(convertiblePaths(s)).toEqual(['/a.pdf']);
  });

  it('follows conversion events', () => {
    let s = addEntries(emptyQueue(), [entry('/a.pdf'), entry('/c.pdf')]);
    s = beginBatch(s, ['/a.pdf', '/c.pdf']);
    expect(s.running).toBe(true);
    s = applyEvent(s, { event: 'fileStarted', data: { path: '/a.pdf' } });
    s = applyEvent(s, { event: 'progress', data: { path: '/a.pdf', fraction: 0.5 } });
    expect(s.items[0]).toMatchObject({ status: 'converting', progress: 0.5 });
    s = applyEvent(s, { event: 'fileFinished', data: { path: '/a.pdf', output: '/out/a.md', warnings: ['w'] } });
    s = applyEvent(s, { event: 'fileFailed', data: { path: '/c.pdf', kind: 'corrupt', message: 'bad' } });
    s = applyEvent(s, { event: 'finished', data: { ok: 1, failed: 1, cancelled: 0 } });
    expect(s.running).toBe(false);
    expect(s.items[0]).toMatchObject({ status: 'done', output: '/out/a.md', warnings: ['w'], progress: 1 });
    expect(s.items[1]).toMatchObject({ status: 'failed', error: { kind: 'corrupt', message: 'bad' } });
    expect(counts(s)).toEqual({ all: 2, done: 1, failed: 1, pending: 0 });
    expect(visibleItems(setFilter(s, 'failed')).map((i) => i.path)).toEqual(['/c.pdf']);
  });

  it('cannot remove items while they convert', () => {
    let s = beginBatch(addEntries(emptyQueue(), [entry('/a.pdf')]), ['/a.pdf']);
    s = applyEvent(s, { event: 'fileStarted', data: { path: '/a.pdf' } });
    expect(removePaths(s, ['/a.pdf']).items).toHaveLength(1);
  });
});
