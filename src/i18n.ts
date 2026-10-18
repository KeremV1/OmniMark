import type { SettingsLanguage } from './api';

export type Lang = 'tr' | 'en';

const tr = {
  'app.title': 'OmniMark',
  'action.addFiles': 'Dosya ekle',
  'action.addFolder': 'Klasör ekle',
  'action.remove': 'Kaldır',
  'action.convertCount': 'Markdown Oluştur ({n})',
  'action.cancel': 'İptal',
  'action.change': 'Değiştir',
  'action.preview': 'Önizle',
  'action.details': 'Ayrıntılar',
  'action.reveal': 'Klasörde göster',
  'action.retry': 'Yeniden dene',
  'action.openFolder': 'Çıktı klasörünü aç',
  'action.copy': 'Kopyala',
  'action.settings': 'Ayarlar',
  'action.close': 'Kapat',
  'drop.hint': 'Dosyaları buraya bırakın',
  'drop.active': 'Bırakın, kuyruğa ekleyelim',
  'output.label': 'Çıktı klasörü',
  'filter.all': 'Tümü',
  'filter.done': 'Tamamlanan',
  'filter.failed': 'Başarısız',
  'filter.pending': 'Bekleyen',
  'status.pending': 'Bekliyor',
  'status.converting': 'Dönüştürülüyor',
  'status.done': 'Tamamlandı',
  'status.failed': 'Başarısız',
  'status.cancelled': 'İptal edildi',
  'status.unsupported': 'Desteklenmiyor',
  'status.unsupportedLater': 'Henüz desteklenmiyor, sonraki sürümde gelecek',
  'preview.view': 'Görünüm',
  'preview.source': 'Kaynak',
  'preview.truncated': 'Önizleme kısaltıldı; dosyanın tamamı çıktı klasöründe.',
  'settings.outputDir': 'Çıktı klasörü',
  'settings.extractImages': 'Görselleri ayıkla',
  'settings.workers': 'Eşzamanlı dönüşüm sayısı',
  'settings.language': 'Dil',
  'settings.languageAuto': 'Sistem',
  'settings.openLogs': 'Günlük klasörünü aç',
  'notice.truncated': 'Çok fazla dosya seçildi; yalnızca ilk {n} dosya eklendi.',
  'error.unsupported': 'Bu dosya türü desteklenmiyor.',
  'error.corrupt': 'Dosya bozuk veya okunamıyor.',
  'error.encrypted': 'Dosya parola korumalı.',
  'error.tooLarge': 'Dosya çok büyük.',
  'error.cancelled': 'İşlem iptal edildi.',
  'error.io': 'Dosya okunamadı veya yazılamadı.',
  'error.missingComponent': 'Gerekli bileşen eksik.',
  'error.busy': 'Başka bir dönüşüm sürüyor.',
  'error.invalidPath': 'Geçersiz dosya yolu.',
  'kind.pdf': 'PDF',
  'kind.word': 'Word',
  'kind.excel': 'Excel',
  'kind.powerPoint': 'PowerPoint',
  'kind.html': 'HTML',
  'kind.csv': 'CSV',
  'kind.text': 'Metin',
  'kind.json': 'JSON',
  'kind.image': 'Görsel',
  'kind.audio': 'Ses',
  'kind.unknown': 'Bilinmiyor',
} as const;

export type MessageKey = keyof typeof tr;

const en: Record<MessageKey, string> = {
  'app.title': 'OmniMark',
  'action.addFiles': 'Add files',
  'action.addFolder': 'Add folder',
  'action.remove': 'Remove',
  'action.convertCount': 'Create Markdown ({n})',
  'action.cancel': 'Cancel',
  'action.change': 'Change',
  'action.preview': 'Preview',
  'action.details': 'Details',
  'action.reveal': 'Show in folder',
  'action.retry': 'Retry',
  'action.openFolder': 'Open output folder',
  'action.copy': 'Copy',
  'action.settings': 'Settings',
  'action.close': 'Close',
  'drop.hint': 'Drop files here',
  'drop.active': 'Release to add to the queue',
  'output.label': 'Output folder',
  'filter.all': 'All',
  'filter.done': 'Done',
  'filter.failed': 'Failed',
  'filter.pending': 'Pending',
  'status.pending': 'Pending',
  'status.converting': 'Converting',
  'status.done': 'Done',
  'status.failed': 'Failed',
  'status.cancelled': 'Cancelled',
  'status.unsupported': 'Unsupported',
  'status.unsupportedLater': 'Not supported yet, coming in a later version',
  'preview.view': 'View',
  'preview.source': 'Source',
  'preview.truncated': 'Preview truncated; the full file is in the output folder.',
  'settings.outputDir': 'Output folder',
  'settings.extractImages': 'Extract images',
  'settings.workers': 'Parallel conversions',
  'settings.language': 'Language',
  'settings.languageAuto': 'System',
  'settings.openLogs': 'Open log folder',
  'notice.truncated': 'Too many files selected; only the first {n} were added.',
  'error.unsupported': 'This file type is not supported.',
  'error.corrupt': 'The file is corrupt or unreadable.',
  'error.encrypted': 'The file is password protected.',
  'error.tooLarge': 'The file is too large.',
  'error.cancelled': 'The operation was cancelled.',
  'error.io': 'The file could not be read or written.',
  'error.missingComponent': 'A required component is missing.',
  'error.busy': 'Another conversion is already running.',
  'error.invalidPath': 'Invalid file path.',
  'kind.pdf': 'PDF',
  'kind.word': 'Word',
  'kind.excel': 'Excel',
  'kind.powerPoint': 'PowerPoint',
  'kind.html': 'HTML',
  'kind.csv': 'CSV',
  'kind.text': 'Text',
  'kind.json': 'JSON',
  'kind.image': 'Image',
  'kind.audio': 'Audio',
  'kind.unknown': 'Unknown',
};

export const messages: Record<Lang, Record<MessageKey, string>> = { tr, en };

let current: Lang = 'tr';

export function resolveLang(setting: SettingsLanguage, navigatorLanguage: string): Lang {
  if (setting === 'tr' || setting === 'en') return setting;
  return navigatorLanguage.toLowerCase().startsWith('tr') ? 'tr' : 'en';
}

export function setLang(lang: Lang): void {
  current = lang;
}

export function getLang(): Lang {
  return current;
}

export function t(key: MessageKey, vars?: Record<string, string | number>): string {
  const text = messages[current][key];
  if (!vars) return text;
  return text.replace(/\{(\w+)\}/g, (m, name: string) => (name in vars ? String(vars[name]) : m));
}
