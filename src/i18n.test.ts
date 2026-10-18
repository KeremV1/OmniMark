import { describe, expect, it } from 'vitest';
import { messages, resolveLang, setLang, t } from './i18n';

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
});
