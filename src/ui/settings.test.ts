import { describe, expect, it } from 'vitest';
import type { OcrStatus } from '../api';
import { loadLang, t, zipSummaryLabel } from '../i18n';
import { formatMegabytes, ocrStatusLines, setDialogError, ZIP_OUTPUTS } from './settings';

describe('settings dialog error line', () => {
  it('shows the message inside the dialog and hides it again on success', () => {
    const line: Pick<HTMLElement, 'textContent' | 'hidden'> = { textContent: '', hidden: true };
    setDialogError(line, 'Dosya okunamadı veya yazılamadı.');
    expect(line).toEqual({ textContent: 'Dosya okunamadı veya yazılamadı.', hidden: false });
    setDialogError(line, null);
    expect(line).toEqual({ textContent: '', hidden: true });
  });
});

const windows: OcrStatus = { engine: 'windows', ready: true, needsDownload: false, ocrsDownloadBytes: 12_212_656, downloading: false };

describe('OCR settings', () => {
  it('formats download sizes per language', () => {
    expect(formatMegabytes(12_212_656, 'tr')).toBe('12,2 MB');
    expect(formatMegabytes(12_212_656, 'en')).toBe('12.2 MB');
  });

  it('describes the engine that will run', async () => {
    await loadLang('en');
    expect(ocrStatusLines(windows)).toEqual(['Windows OCR will be used (ready).']);
    const ready: OcrStatus = { ...windows, engine: 'ocrs' };
    expect(ocrStatusLines(ready)).toEqual(['The built-in ocrs engine will be used (ready).', 'ocrs reads Latin-script text (English-focused).']);
    const missing: OcrStatus = { ...ready, ready: false, needsDownload: true };
    expect(ocrStatusLines(missing)[0]).toBe('The built-in ocrs engine will be used; its model must be downloaded first.');
    expect(ocrStatusLines({ ...windows, engine: null, ready: false })).toEqual(['No OCR engine is available; files convert without OCR.']);
  });

  it('is translated', async () => {
    await loadLang('tr');
    expect(ocrStatusLines(windows)).toEqual(['Windows OCR kullanılacak (hazır).']);
    expect(ocrStatusLines({ ...windows, engine: 'ocrs' })).toHaveLength(2);
  });
});

describe('ZIP settings', () => {
  it('offers the three modes in display order, each with a label', async () => {
    expect(ZIP_OUTPUTS.map(([mode]) => mode)).toEqual(['folder', 'single', 'zip']);
    await loadLang('en');
    expect(ZIP_OUTPUTS.map(([, key]) => t(key))).toEqual([
      'A folder with the same structure',
      'One combined Markdown file',
      'A ZIP of the Markdown files',
    ]);
  });

  it('summarizes a finished zip per language', async () => {
    await loadLang('en');
    expect(zipSummaryLabel(12, 2)).toBe('12 files converted, 2 skipped');
    expect(zipSummaryLabel(5, 0)).toBe('5 files converted');
    await loadLang('tr');
    expect(zipSummaryLabel(12, 2)).toBe('12 dosya dönüştürüldü, 2 atlandı');
    await loadLang('ru');
    expect(zipSummaryLabel(12, 2)).toBe('Преобразовано файлов: 12, пропущено: 2');
  });
});
