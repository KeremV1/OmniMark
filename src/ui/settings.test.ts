import { describe, expect, it } from 'vitest';
import type { OcrStatus } from '../api';
import { setLang } from '../i18n';
import { formatMegabytes, ocrStatusLines, setDialogError } from './settings';

describe('settings dialog error line', () => {
  it('shows the message inside the dialog and hides it again on success', () => {
    const line: Pick<HTMLElement, 'textContent' | 'hidden'> = { textContent: '', hidden: true };
    setDialogError(line, 'Dosya okunamadı veya yazılamadı.');
    expect(line).toEqual({ textContent: 'Dosya okunamadı veya yazılamadı.', hidden: false });
    setDialogError(line, null);
    expect(line).toEqual({ textContent: '', hidden: true });
  });
});

const base: OcrStatus = { windowsBuilt: true, windowsLanguages: ['tr'], tesseractPath: null, tesseractLanguages: [], ocrsBuilt: true, ocrsModels: false, ocrsDownloadBytes: 12_212_656, selected: 'windows', downloading: false };

describe('OCR settings', () => {
  it('formats download sizes per language', () => {
    expect(formatMegabytes(12_212_656, 'tr')).toBe('12,2 MB');
    expect(formatMegabytes(12_212_656, 'en')).toBe('12.2 MB');
  });
  it('describes engine status and warns about ocrs for Turkish', () => {
    setLang('tr');
    expect(ocrStatusLines(base, 'tr')).toEqual(['Windows OCR: tr hazır', 'Tesseract: kurulu değil (isteğe bağlı; Windows OCR yeterli)', 'ocrs: model indirilmedi']);
    const withTesseract: OcrStatus = { ...base, tesseractPath: '/usr/bin/tesseract', tesseractLanguages: ['eng', 'tur'], selected: 'tesseract' };
    expect(ocrStatusLines(withTesseract, 'tr')[1]).toBe('Tesseract: /usr/bin/tesseract (eng, tur)');
    const ocrsOnly: OcrStatus = { ...base, windowsLanguages: [], ocrsModels: true, selected: 'ocrs' };
    expect(ocrStatusLines(ocrsOnly, 'tr')).toEqual([
      'Windows OCR: yüklü dil yok', 'Tesseract: kurulu değil (isteğe bağlı)', 'ocrs: model indirildi', 'ocrs Türkçe harfleri (ğ ş ı İ ç ö ü) tanımaz; Türkçe için önerilmez.',
    ]);
    setLang('en');
    expect(ocrStatusLines({ ...base, windowsBuilt: false, selected: null }, 'en')).toEqual([
      'Tesseract: not installed (optional)', 'ocrs: model not downloaded', 'No OCR engine is available; files convert without OCR.',
    ]);
  });
  it('says the selected engine is unavailable only when another engine is ready', () => {
    setLang('en');
    const selectedMissing: OcrStatus = { ...base, selected: null };
    expect(ocrStatusLines(selectedMissing, 'en')).toContain("The selected engine is not available; choose 'Automatic' or install it.");
    expect(ocrStatusLines(selectedMissing, 'en').join()).not.toContain('No OCR engine');
    const nothing: OcrStatus = { ...base, windowsLanguages: [], selected: null };
    expect(ocrStatusLines(nothing, 'en')).toContain('No OCR engine is available; files convert without OCR.');
    setLang('tr');
    expect(ocrStatusLines(selectedMissing, 'tr')).toContain("Seçili motor kullanılamıyor; 'Otomatik'i seçin ya da motoru kurun.");
  });
  it('words the missing Tesseract line per platform', () => {
    setLang('en');
    expect(ocrStatusLines(base, 'en')[1]).toBe('Tesseract: not installed (optional; Windows OCR covers it)');
    expect(ocrStatusLines({ ...base, windowsBuilt: false }, 'en')[0]).toBe('Tesseract: not installed (optional)');
    // Windows OCR built but without a language pack cannot "cover it".
    expect(ocrStatusLines({ ...base, windowsLanguages: [] }, 'en')[1]).toBe('Tesseract: not installed (optional)');
  });
});
