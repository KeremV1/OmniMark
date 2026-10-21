import { describe, expect, it, vi } from 'vitest';
import { loadLang, t } from '../i18n';
import { checkQuietly, shouldCheckForUpdates } from '../updater';
import { updateNotice } from './update';

describe('update check', () => {
  it('runs unless the setting is explicitly off (old settings count as on)', () => {
    expect(shouldCheckForUpdates({ checkUpdates: true })).toBe(true);
    expect(shouldCheckForUpdates({})).toBe(true);
    expect(shouldCheckForUpdates({ checkUpdates: false })).toBe(false);
  });

  it('returns the new version, or null when up to date', async () => {
    expect(await checkQuietly(async () => ({ version: '0.3.0' }))).toBe('0.3.0');
    expect(await checkQuietly(async () => null)).toBeNull();
  });

  it('stays silent on failure and only logs', async () => {
    const log = vi.fn();
    const boom = new Error('offline');
    expect(await checkQuietly(() => Promise.reject(boom), log)).toBeNull();
    expect(log).toHaveBeenCalledWith('update check failed', boom);
  });
});

describe('footer update notice', () => {
  it('is hidden without an update', () => {
    expect(updateNotice(null)).toBeNull();
  });

  it('offers the install, then shows progress, then allows a retry', () => {
    expect(updateNotice({ version: '0.3.0', phase: 'available' })).toEqual({
      textKey: 'update.available',
      version: '0.3.0',
      buttonKey: 'update.install',
      busy: false,
    });
    expect(updateNotice({ version: '0.3.0', phase: 'installing' })).toMatchObject({ buttonKey: 'update.installing', busy: true });
    expect(updateNotice({ version: '0.3.0', phase: 'failed' })).toMatchObject({ textKey: 'update.failed', busy: false });
  });

  it('renders the version into the text', async () => {
    await loadLang('en');
    expect(t('update.available', { version: '0.3.0' })).toBe('Update available (v0.3.0)');
  });
});
