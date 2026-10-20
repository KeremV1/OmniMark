import { describe, expect, it } from 'vitest';
import { showFirstRun } from './firstrun';

describe('first-run card', () => {
  it('shows until onboarded', () => {
    expect(showFirstRun({ onboarded: false })).toBe(true);
    expect(showFirstRun({ onboarded: true })).toBe(false);
  });
});
