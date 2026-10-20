import { describe, expect, it } from 'vitest';
import {
  addEntries,
  applyEvent,
  beginBatch,
  convertiblePaths,
  counts,
  emptyQueue,
  endBatch,
  failBatch,
  removablePaths,
  removePaths,
  selectionState,
  setAllSelected,
  setFilter,
  previewable,
  setSelected,
  visibleItems,
} from './queue';
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

  it('ignores late progress and fileStarted events for finished items', () => {
    let s = beginBatch(addEntries(emptyQueue(), [entry('/a.pdf'), entry('/b.pdf'), entry('/c.pdf')]), ['/a.pdf', '/b.pdf', '/c.pdf']);
    s = applyEvent(s, { event: 'fileFinished', data: { path: '/a.pdf', output: '/out/a.md', warnings: [] } });
    s = applyEvent(s, { event: 'fileFailed', data: { path: '/b.pdf', kind: 'corrupt', message: 'bad' } });
    s = applyEvent(s, { event: 'fileCancelled', data: { path: '/c.pdf' } });
    for (const path of ['/a.pdf', '/b.pdf', '/c.pdf']) {
      s = applyEvent(s, { event: 'progress', data: { path, fraction: 0.4 } });
      s = applyEvent(s, { event: 'fileStarted', data: { path } });
    }
    expect(s.items.map((i) => [i.status, i.progress])).toEqual([
      ['done', 1],
      ['failed', 0],
      ['cancelled', 0],
    ]);
  });

  it('a retried item converts again after beginBatch', () => {
    let s = beginBatch(addEntries(emptyQueue(), [entry('/a.pdf')]), ['/a.pdf']);
    s = applyEvent(s, { event: 'fileFailed', data: { path: '/a.pdf', kind: 'io', message: 'x' } });
    s = beginBatch(s, ['/a.pdf']);
    s = applyEvent(s, { event: 'progress', data: { path: '/a.pdf', fraction: 0.3 } });
    expect(s.items[0]).toMatchObject({ status: 'converting', progress: 0.3, error: undefined });
  });

  it('endBatch stops the run without touching items', () => {
    let s = beginBatch(addEntries(emptyQueue(), [entry('/a.pdf')]), ['/a.pdf']);
    s = endBatch(s);
    expect(s.running).toBe(false);
    expect(s.items[0].status).toBe('pending');
  });

  it('failBatch stops the run and fails items that were mid-conversion', () => {
    let s = beginBatch(addEntries(emptyQueue(), [entry('/a.pdf'), entry('/b.pdf')]), ['/a.pdf', '/b.pdf']);
    s = applyEvent(s, { event: 'fileStarted', data: { path: '/a.pdf' } });
    s = failBatch(s, { kind: 'busy', message: 'a conversion is already running' });
    expect(s.running).toBe(false);
    expect(s.items[0]).toMatchObject({ status: 'failed', error: { kind: 'busy' } });
    expect(s.items[1].status).toBe('pending');
    expect(convertiblePaths(s)).toEqual(['/a.pdf', '/b.pdf']);
  });

  it('reports selection state and removable paths', () => {
    let s = addEntries(emptyQueue(), [entry('/a.pdf'), entry('/b.pdf'), entry('/c.jpg', false)]);
    expect(selectionState(s)).toBe('all');
    s = setSelected(s, '/b.pdf', false);
    expect(selectionState(s)).toBe('some');
    s = setAllSelected(s, false);
    expect(selectionState(s)).toBe('none');
    expect(selectionState(emptyQueue())).toBe('none');
    s = setSelected(beginBatch(setAllSelected(s, true), ['/a.pdf']), '/a.pdf', true);
    s = applyEvent(s, { event: 'fileStarted', data: { path: '/a.pdf' } });
    expect(removablePaths(s)).toEqual(['/b.pdf']);
  });
});

describe('zip rows', () => {
  const zipEntry: FileEntry = { path: '/p.zip', name: 'p.zip', kind: 'zip', size: 10, supported: true };

  it('keeps the zip result and previews only the combined file', () => {
    for (const [mode, preview] of [['single', true], ['folder', false], ['zip', false]] as const) {
      let s = beginBatch(addEntries(emptyQueue(), [zipEntry]), ['/p.zip']);
      s = applyEvent(s, {
        event: 'fileFinished',
        data: { path: '/p.zip', output: '/out/p', warnings: [], zip: { mode, converted: 3, skipped: 1 } },
      });
      expect(s.items[0]).toMatchObject({ status: 'done', zip: { mode, converted: 3, skipped: 1 } });
      expect(previewable(s.items[0]!)).toBe(preview);
      // Converting again forgets the old result.
      expect(beginBatch(s, ['/p.zip']).items[0]!.zip).toBeUndefined();
    }
  });

  it('previews ordinary files and never unfinished ones', () => {
    let s = beginBatch(addEntries(emptyQueue(), [entry('/a.pdf')]), ['/a.pdf']);
    expect(previewable(s.items[0]!)).toBe(false);
    s = applyEvent(s, { event: 'fileFinished', data: { path: '/a.pdf', output: '/out/a.md', warnings: [], zip: null } });
    expect(previewable(s.items[0]!)).toBe(true);
    expect(s.items[0]!.zip).toBeUndefined();
  });
});
