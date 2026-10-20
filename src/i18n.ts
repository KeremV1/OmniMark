import type { SettingsLanguage } from './api';
import { en, type MessageKey, type Messages } from './locales/en';

export type { MessageKey, Messages };

/** Interface languages; every value is a valid BCP-47 tag. English is the default and the fallback. */
export const LANGS = ['en', 'tr', 'ru', 'de', 'es', 'fr', 'pt-BR'] as const;
export type Lang = (typeof LANGS)[number];

/** Each language shown by its own name (settings language select). */
export const LANGUAGE_NAMES: Record<Lang, string> = {
  en: 'English',
  tr: 'Türkçe',
  ru: 'Русский',
  de: 'Deutsch',
  es: 'Español',
  fr: 'Français',
  'pt-BR': 'Português (Brasil)',
};

/** English is bundled; every other locale is a separate chunk fetched on demand. */
const loaders: Record<Exclude<Lang, 'en'>, () => Promise<{ default: Messages }>> = {
  tr: () => import('./locales/tr'),
  ru: () => import('./locales/ru'),
  de: () => import('./locales/de'),
  es: () => import('./locales/es'),
  fr: () => import('./locales/fr'),
  'pt-BR': () => import('./locales/pt-BR'),
};

const loaded: Partial<Record<Lang, Messages>> = { en };

let current: Lang = 'en';

export function isLang(value: string): value is Lang {
  return (LANGS as readonly string[]).includes(value);
}

/** Maps a BCP-47 tag (`de-AT`, `pt-PT`, `ru`) to a supported language, or undefined. */
export function matchLang(tag: string): Lang | undefined {
  const primary = tag.toLowerCase().split(/[-_]/)[0];
  switch (primary) {
    case 'tr':
    case 'ru':
    case 'de':
    case 'es':
    case 'fr':
      return primary;
    case 'pt':
      return 'pt-BR';
    case 'en':
      return 'en';
    default:
      return undefined;
  }
}

/** The explicit setting wins; `auto` takes the first supported system language, else English. */
export function resolveLang(setting: SettingsLanguage, systemLanguages: string | readonly string[]): Lang {
  if (setting !== 'auto' && isLang(setting)) return setting;
  const tags = typeof systemLanguages === 'string' ? [systemLanguages] : systemLanguages;
  for (const tag of tags) {
    const lang = matchLang(tag);
    if (lang) return lang;
  }
  return 'en';
}

/** Fetches the locale if needed and makes it the active one. A failed fetch falls back to English. */
export async function loadLang(lang: Lang): Promise<Lang> {
  if (!loaded[lang] && lang !== 'en') {
    try {
      loaded[lang] = (await loaders[lang]()).default;
    } catch (err) {
      console.error(`could not load locale ${lang}`, err);
      current = 'en';
      return current;
    }
  }
  current = lang;
  return current;
}

/** Switches to an already loaded language (English is always loaded); otherwise to English. */
export function setLang(lang: Lang): void {
  current = loaded[lang] ? lang : 'en';
}

export function getLang(): Lang {
  return current;
}

export function hasMessage(key: string): key is MessageKey {
  return key in en;
}

export function t(key: MessageKey, vars?: Record<string, string | number>): string {
  const text = loaded[current]?.[key] ?? en[key];
  if (!vars) return text;
  return text.replace(/\{(\w+)\}/g, (m, name: string) => (name in vars ? String(vars[name]) : m));
}

/** "1 warning" for one, "{n} warnings" otherwise. */
export function warningsLabel(n: number): string {
  return n === 1 ? t('status.warning1') : t('status.warnings', { n });
}
