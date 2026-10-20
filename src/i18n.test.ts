import { beforeEach, describe, expect, it } from 'vitest';
import { LANGS, type Lang, loadLang, matchLang, resolveLang, setLang, t, warningsLabel } from './i18n';
import { en, type MessageKey, type Messages } from './locales/en';

const placeholders = (text: string): string[] => (text.match(/\{\w+\}/g) ?? []).sort();

describe('language detection', () => {
  it('maps system languages to supported ones', () => {
    expect(matchLang('tr-TR')).toBe('tr');
    expect(matchLang('ru')).toBe('ru');
    expect(matchLang('de-AT')).toBe('de');
    expect(matchLang('es-419')).toBe('es');
    expect(matchLang('fr-CA')).toBe('fr');
    expect(matchLang('pt-BR')).toBe('pt-BR');
    expect(matchLang('pt-PT')).toBe('pt-BR');
    expect(matchLang('pt')).toBe('pt-BR');
    expect(matchLang('en-GB')).toBe('en');
    expect(matchLang('ja-JP')).toBeUndefined();
  });
  it('resolves auto from the system list and falls back to English', () => {
    expect(resolveLang('auto', 'tr-TR')).toBe('tr');
    expect(resolveLang('auto', ['ja-JP', 'de-DE', 'tr'])).toBe('de');
    expect(resolveLang('auto', 'ja-JP')).toBe('en');
    expect(resolveLang('auto', [])).toBe('en');
    expect(resolveLang('ru', 'tr-TR')).toBe('ru');
    expect(resolveLang('pt-BR', 'en-US')).toBe('pt-BR');
  });
  it('only yields tags Intl accepts', () => {
    for (const lang of LANGS) expect(Intl.NumberFormat.supportedLocalesOf(lang)).toEqual([lang]);
  });
});

describe('locales', () => {
  const enKeys = Object.keys(en).sort();
  for (const lang of LANGS.filter((l) => l !== 'en')) {
    it(`${lang} has exactly the English keys and placeholders`, async () => {
      const mod = (await import(`./locales/${lang}.ts`)) as { default: Messages };
      expect(Object.keys(mod.default).sort()).toEqual(enKeys);
      for (const key of enKeys as MessageKey[]) {
        expect(placeholders(mod.default[key]), `${lang}:${key}`).toEqual(placeholders(en[key]));
        expect(mod.default[key].trim(), `${lang}:${key}`).not.toBe('');
      }
    });
  }
});

describe('i18n', () => {
  beforeEach(() => setLang('en'));

  it('defaults to English', () => {
    expect(t('action.cancel')).toBe('Cancel');
  });
  it('loading a locale switches t() and interpolates', async () => {
    await loadLang('tr');
    expect(t('action.convertCount', { n: 3 })).toBe('Markdown Oluştur (3)');
    await loadLang('de');
    expect(t('action.cancel')).toBe('Abbrechen');
    await loadLang('en');
    expect(t('action.cancel')).toBe('Cancel');
  });
  it('setLang ignores a language that is not loaded yet', () => {
    setLang('fr' as Lang);
    expect(t('action.cancel')).toBe('Cancel');
  });
  it('uses the singular for exactly one warning', async () => {
    expect(warningsLabel(1)).toBe('1 warning');
    expect(warningsLabel(2)).toBe('2 warnings');
    await loadLang('tr');
    expect(warningsLabel(1)).toBe('1 uyarı');
    expect(warningsLabel(3)).toBe('3 uyarı');
  });
});
