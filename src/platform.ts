import type { Lang } from './i18n';

export function isMac(userAgent: string = navigator.userAgent): boolean {
  return /Macintosh|Mac OS X|iPhone|iPad/i.test(userAgent);
}

export function modKeyLabel(): string {
  return isMac() ? '⌘' : 'Ctrl';
}

/** `512 B`, `2,4 MB` (tr) / `2.4 MB` (en). Binary units (1024). */
export function formatBytes(n: number, lang: Lang): string {
  if (n < 1024) return `${n} B`;
  const units = ['KB', 'MB', 'GB', 'TB'];
  let value = n / 1024;
  let unit = 0;
  while (value >= 1024 && unit < units.length - 1) {
    value /= 1024;
    unit++;
  }
  const text = value.toFixed(1);
  return `${lang === 'tr' ? text.replace('.', ',') : text} ${units[unit]}`;
}

export function fileName(path: string): string {
  const parts = path.split(/[\\/]/);
  return parts[parts.length - 1] || path;
}
