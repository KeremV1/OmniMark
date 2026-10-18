import { isMac } from '../platform';

export interface ShortcutHandlers {
  addFiles(): void;
  convert(): void;
  /** Called for Delete/Backspace; return false to let the key through. */
  removeSelected(): boolean;
  /** Called for Escape when no dialog is open; return true if something closed. */
  escape(): boolean;
}

/** Window-wide shortcuts: Mod+O, Mod+Enter, Delete/Backspace (list focus), Escape. */
export function installShortcuts(handlers: ShortcutHandlers): () => void {
  const mac = isMac();
  const onKey = (ev: KeyboardEvent) => {
    const mod = mac ? ev.metaKey : ev.ctrlKey;
    const inDialog = document.querySelector('dialog[open]') !== null;
    const target = ev.target as HTMLElement | null;
    const typing = target !== null && (target.isContentEditable || /^(INPUT|SELECT|TEXTAREA)$/.test(target.tagName)) && !(target instanceof HTMLInputElement && target.type === 'checkbox');

    if (mod && !ev.shiftKey && !ev.altKey && ev.key.toLowerCase() === 'o') {
      if (inDialog) return;
      ev.preventDefault();
      handlers.addFiles();
    } else if (mod && ev.key === 'Enter') {
      if (inDialog) return;
      ev.preventDefault();
      handlers.convert();
    } else if ((ev.key === 'Delete' || ev.key === 'Backspace') && !mod && !inDialog && !typing) {
      if (handlers.removeSelected()) ev.preventDefault();
    } else if (ev.key === 'Escape' && !inDialog) {
      // Open <dialog>s close themselves on Escape.
      if (handlers.escape()) ev.preventDefault();
    }
  };
  window.addEventListener('keydown', onKey);
  return () => window.removeEventListener('keydown', onKey);
}
