import type { MessageKey } from '../i18n';
import type { UpdateState } from '../updater';

export interface UpdateNotice {
  textKey: MessageKey;
  version: string;
  /** Label of the install button (the retry label after a failure). */
  buttonKey: MessageKey;
  /** The install button is disabled while the update is being installed. */
  busy: boolean;
}

/** What the footer shows for `update`; `null` hides the notice. */
export function updateNotice(update: UpdateState | null): UpdateNotice | null {
  if (!update) return null;
  switch (update.phase) {
    case 'available':
      return { textKey: 'update.available', version: update.version, buttonKey: 'update.install', busy: false };
    case 'installing':
      return { textKey: 'update.available', version: update.version, buttonKey: 'update.installing', busy: true };
    case 'failed':
      return { textKey: 'update.failed', version: update.version, buttonKey: 'update.install', busy: false };
  }
}
