import { describe, expect, it } from 'vitest';
import { fileName, formatBytes, isMac } from './platform';

describe('platform', () => {
  it('detects mac and formats sizes per language', () => {
    expect(isMac('Mozilla/5.0 (Macintosh; Intel Mac OS X 14_0)')).toBe(true);
    expect(isMac('Mozilla/5.0 (Windows NT 10.0)')).toBe(false);
    expect(formatBytes(2_516_582, 'tr')).toBe('2,4 MB');
    expect(formatBytes(2_516_582, 'en')).toBe('2.4 MB');
    expect(formatBytes(512, 'tr')).toBe('512 B');
    expect(fileName('C:\\a\\b\\rapor.pdf')).toBe('rapor.pdf');
    expect(fileName('/home/u/rapor.pdf')).toBe('rapor.pdf');
  });
});
