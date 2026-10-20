import type { AppError, ConvertEvent, FileEntry, FileKind, ZipInfo } from './api';

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
  /** A finished `.zip`: `output` is then the combined file, a folder or a zip (see `zip.mode`). */
  zip?: ZipInfo;
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

const TERMINAL: ReadonlySet<Status> = new Set<Status>(['done', 'failed', 'cancelled', 'unsupported']);

/** Like `patch`, but leaves items that already reached a terminal status alone (late events). */
function patchActive(state: QueueState, path: string, change: Partial<QueueItem>): QueueState {
  return mapItems(state, (i) => (i.path === path && !TERMINAL.has(i.status) ? { ...i, ...change } : i));
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
      ? { ...i, status: 'pending', progress: 0, output: undefined, zip: undefined, warnings: [], error: undefined }
      : i,
  );
  return { ...next, running: true };
}

/** Marks the run as over; used when `convert()` settles, whether or not `finished` arrived. */
export function endBatch(state: QueueState): QueueState {
  return { ...state, running: false };
}

/** `convert()` rejected: stop the run and fail every item that was still mid-conversion. */
export function failBatch(state: QueueState, error: AppError): QueueState {
  return endBatch(mapItems(state, (i) => (i.status === 'converting' ? { ...i, status: 'failed', error } : i)));
}

/** Selected items that may leave the list now (converting ones stay). */
export function removablePaths(state: QueueState): string[] {
  return state.items.filter((i) => i.selected && i.status !== 'converting').map((i) => i.path);
}

/** Header checkbox state over the selectable (supported) items. */
export function selectionState(state: QueueState): 'all' | 'some' | 'none' {
  const selectable = state.items.filter((i) => i.status !== 'unsupported');
  const selected = selectable.filter((i) => i.selected).length;
  if (selected === 0) return 'none';
  return selected === selectable.length ? 'all' : 'some';
}

export function applyEvent(state: QueueState, event: ConvertEvent): QueueState {
  switch (event.event) {
    case 'started':
      return state;
    case 'fileStarted':
      return patchActive(state, event.data.path, { status: 'converting', progress: 0 });
    case 'progress':
      return patchActive(state, event.data.path, { status: 'converting', progress: event.data.fraction });
    case 'fileFinished':
      return patch(state, event.data.path, {
        status: 'done',
        progress: 1,
        output: event.data.output,
        zip: event.data.zip ?? undefined,
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

/** A finished item whose output is one Markdown file (everything except a folder or zip result). */
export function previewable(item: QueueItem): boolean {
  return item.status === 'done' && !!item.output && (!item.zip || item.zip.mode === 'single');
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
