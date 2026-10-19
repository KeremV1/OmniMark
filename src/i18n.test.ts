import { describe, expect, it } from 'vitest';
import { messages, resolveLang, setLang, t, warningsLabel } from './i18n';

describe('i18n', () => {
  it('resolves auto from the system language', () => {
    expect(resolveLang('auto', 'tr-TR')).toBe('tr');
    expect(resolveLang('auto', 'de-DE')).toBe('en');
    expect(resolveLang('en', 'tr-TR')).toBe('en');
  });
  it('interpolates and has identical keys in both languages', () => {
    setLang('tr');
    expect(t('action.convertCount', { n: 3 })).toBe('Markdown Oluştur (3)');
    expect(Object.keys(messages.tr).sort()).toEqual(Object.keys(messages.en).sort());
  });
  it('uses the singular for exactly one warning', () => {
    setLang('en');
    expect(warningsLabel(1)).toBe('1 warning');
    expect(warningsLabel(2)).toBe('2 warnings');
    setLang('tr');
    expect(warningsLabel(1)).toBe('1 uyarı');
    expect(warningsLabel(3)).toBe('3 uyarı');
  });
});
