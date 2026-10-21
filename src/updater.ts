import { relaunch } from '@tauri-apps/plugin-process';
import { check, type Update } from '@tauri-apps/plugin-updater';
import type { Settings } from './api';

/** The footer's update notice: a newer version was found and what is happening with it. */
export interface UpdateState {
  version: string;
  phase: 'available' | 'installing' | 'failed';
}

/** Old settings objects may lack the field; the check is on unless it is explicitly off. */
export function shouldCheckForUpdates(settings: Partial<Pick<Settings, 'checkUpdates'>>): boolean {
  return settings.checkUpdates !== false;
}

/** The pending update, kept only while the notice is shown so nothing stays resident otherwise. */
let pending: Update | null = null;

/**
 * Silent background check: resolves to the new version or `null`. A failure (offline, no release
 * yet, bad signature) is only logged, never shown.
 */
export async function checkQuietly(
  checkFn: () => Promise<{ version: string } | null>,
  log: (message: string, error: unknown) => void = (m, e) => console.warn(m, e),
): Promise<string | null> {
  try {
    return (await checkFn())?.version ?? null;
  } catch (error) {
    log('update check failed', error);
    return null;
  }
}

/** Checks the release endpoint; returns the version of a newer release, if any. */
export async function findUpdate(): Promise<string | null> {
  return checkQuietly(async () => {
    pending = await check();
    return pending;
  });
}

/** Downloads, installs and relaunches. Rejects if any step fails. */
export async function installUpdate(): Promise<void> {
  if (!pending) throw new Error('no update is pending');
  await pending.downloadAndInstall();
  await relaunch();
}
