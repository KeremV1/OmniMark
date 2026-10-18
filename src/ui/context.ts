import type { Settings } from '../api';
import type { Filter, QueueState } from '../queue';

export interface Notice {
  tone: 'info' | 'error';
  text: string;
}

/** Everything the window shows; owned by `main.ts`, read by every UI module. */
export interface View {
  queue: QueueState;
  settings: Settings;
  /** Path of the queue item whose output is open in the preview panel. */
  preview: string | null;
  notice: Notice | null;
  dragging: boolean;
}

/** What the UI modules may ask for; implemented in `main.ts`. */
export interface Actions {
  addFiles(): void;
  addPaths(paths: string[]): void;
  removeSelected(): void;
  removePath(path: string): void;
  setSelected(path: string, value: boolean): void;
  setAllSelected(value: boolean): void;
  setFilter(filter: Filter): void;
  convertSelected(): void;
  convertPaths(paths: string[]): void;
  cancel(): void;
  openPreview(path: string): void;
  closePreview(): void;
  showDetails(path: string): void;
  reveal(path: string): void;
  changeOutputDir(): void;
  openOutputDir(): void;
  openSettings(): void;
  dismissNotice(): void;
  setDragging(value: boolean): void;
  /** Shows a command failure in the notice strip. */
  fail(error: unknown): void;
}

/** Native-style separator for display and joining (Windows paths keep backslashes). */
export function separatorOf(path: string): string {
  return path.includes('\\') && !path.includes('/') ? '\\' : '/';
}

export function dirName(path: string): string {
  const i = Math.max(path.lastIndexOf('/'), path.lastIndexOf('\\'));
  return i > 0 ? path.slice(0, i) : path;
}

/** `C:\Users\Kerem\Documents\OmniMark` → `…\Documents\OmniMark`. */
export function shortPath(path: string, keep = 2): string {
  const sep = separatorOf(path);
  const parts = path.split(/[\\/]/).filter((p) => p.length > 0);
  if (parts.length <= keep + 1) return path;
  return `…${sep}${parts.slice(-keep).join(sep)}`;
}
