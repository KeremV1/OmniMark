import type { AppError, ConvertEvent, FileEntry, FileKind } from './api';

export type Status = 'pending' | 'converting' | 'done' | 'failed' | 'cancelled' | 'unsupported';
export type Filter = 'all' | 'done' | 'failed' | 'pending';

export interface QueueItem {
  path: string;
  name: string;
  kind: FileKind;
  size: number;
  status: Status;
  selected: boolean;
  progress: number;
  output?: string;
  warnings: string[];
  error?: AppError;
}

export interface QueueState {
  items: QueueItem[];
  running: boolean;
  filter: Filter;
}

export function emptyQueue(): QueueState {
  return { items: [], running: false, filter: 'all' };
}

export function addEntries(state: QueueState, entries: FileEntry[]): QueueState {
  const known = new Set(state.items.map((i) => i.path));
  const added: QueueItem[] = [];
  for (const e of entries) {
    if (known.has(e.path)) continue;
    known.add(e.path);
    added.push({
      path: e.path,
      name: e.name,
      kind: e.kind,
      size: e.size,
      status: e.supported ? 'pending' : 'unsupported',
      selected: e.supported,
      progress: 0,
      warnings: [],
    });
  }
  return { ...state, items: [...state.items, ...added] };
}

function mapItems(state: QueueState, fn: (item: QueueItem) => QueueItem): QueueState {
  return { ...state, items: state.items.map(fn) };
}

function patch(state: QueueState, path: string, change: Partial<QueueItem>): QueueState {
  return mapItems(state, (i) => (i.path === path ? { ...i, ...change } : i));
}

/** Converting items stay; everything else listed in `paths` is dropped. */
export function removePaths(state: QueueState, paths: string[]): QueueState {
  const drop = new Set(paths);
  return { ...state, items: state.items.filter((i) => !drop.has(i.path) || i.status === 'converting') };
}

export function setSelected(state: QueueState, path: string, value: boolean): QueueState {
  return mapItems(state, (i) => (i.path === path && i.status !== 'unsupported' ? { ...i, selected: value } : i));
}

export function setAllSelected(state: QueueState, value: boolean): QueueState {
  return mapItems(state, (i) => (i.status === 'unsupported' ? i : { ...i, selected: value }));
}

/** Selected files that can be (re)converted now, in queue order. */
export function convertiblePaths(state: QueueState): string[] {
  return state.items
    .filter((i) => i.selected && i.status !== 'unsupported' && i.status !== 'converting')
    .map((i) => i.path);
}

/** Resets the given files to pending and marks the queue as running. */
export function beginBatch(state: QueueState, paths: string[]): QueueState {
  const batch = new Set(paths);
  const next = mapItems(state, (i) =>
    batch.has(i.path) && i.status !== 'unsupported'
      ? { ...i, status: 'pending', progress: 0, output: undefined, warnings: [], error: undefined }
      : i,
  );
  return { ...next, running: true };
}

export function applyEvent(state: QueueState, event: ConvertEvent): QueueState {
  switch (event.event) {
    case 'started':
      return state;
    case 'fileStarted':
      return patch(state, event.data.path, { status: 'converting', progress: 0 });
    case 'progress':
      return patch(state, event.data.path, { status: 'converting', progress: event.data.fraction });
    case 'fileFinished':
      return patch(state, event.data.path, {
        status: 'done',
        progress: 1,
        output: event.data.output,
        warnings: event.data.warnings,
        error: undefined,
      });
    case 'fileFailed':
      return patch(state, event.data.path, {
        status: 'failed',
        error: { kind: event.data.kind, message: event.data.message },
      });
    case 'fileCancelled':
      return patch(state, event.data.path, { status: 'cancelled' });
    case 'finished':
      return { ...state, running: false };
  }
}

export function counts(state: QueueState): { all: number; done: number; failed: number; pending: number } {
  const c = { all: state.items.length, done: 0, failed: 0, pending: 0 };
  for (const i of state.items) {
    if (i.status === 'done') c.done++;
    else if (i.status === 'failed') c.failed++;
    else if (i.status === 'pending' || i.status === 'converting') c.pending++;
  }
  return c;
}

export function setFilter(state: QueueState, filter: Filter): QueueState {
  return { ...state, filter };
}

export function visibleItems(state: QueueState): QueueItem[] {
  switch (state.filter) {
    case 'all':
      return state.items;
    case 'done':
      return state.items.filter((i) => i.status === 'done');
    case 'failed':
      return state.items.filter((i) => i.status === 'failed');
    case 'pending':
      return state.items.filter((i) => i.status === 'pending' || i.status === 'converting');
  }
}
