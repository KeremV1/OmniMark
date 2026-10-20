/**
 * Fake backend for `npm run dev` in a plain browser (no Tauri). Loaded only when
 * `import.meta.env.DEV && !('__TAURI_INTERNALS__' in window)`, so it never ships.
 *
 * `?scenario=` picks a screen for screenshots: `queue` (default), `empty`, `drag`,
 * `idle` (files added, nothing converted), `preview`, `settings`, `details`, `menu`.
 * `&lang=en|tr` forces the UI language.
 */
import type { Channel } from '@tauri-apps/api/core';
import { mockConvertFileSrc, mockIPC, mockWindows } from '@tauri-apps/api/mocks';
import type { ConvertEvent, DownloadEvent, FileEntry, OcrStatus, Settings } from './api';
import type { Actions } from './ui/context';

const DIR = 'C:\\Users\\Kerem\\Documents\\Raporlar';
const OUT = 'C:\\Users\\Kerem\\Documents\\OmniMark';

const SAMPLES: FileEntry[] = [
  { path: `${DIR}\\Yıllık rapor 2026.pdf`, name: 'Yıllık rapor 2026.pdf', kind: 'pdf', size: 2_516_582, supported: true },
  { path: `${DIR}\\Ürün yol haritası sunumu.pptx`, name: 'Ürün yol haritası sunumu.pptx', kind: 'powerPoint', size: 8_493_465, supported: true },
  { path: `${DIR}\\Bütçe 2027.xlsx`, name: 'Bütçe 2027.xlsx', kind: 'excel', size: 184_320, supported: true },
  { path: `${DIR}\\ofis-fotoğrafı.jpg`, name: 'ofis-fotoğrafı.jpg', kind: 'image', size: 1_258_291, supported: true },
  { path: `${DIR}\\Sözleşme taslağı.docx`, name: 'Sözleşme taslağı.docx', kind: 'word', size: 56_320, supported: true },
];

const SAMPLE_MARKDOWN = `# Yıllık rapor 2026

Bu rapor, şirketin 2026 yılı faaliyetlerini ve **temel göstergelerini** özetler.

## Öne çıkanlar

- Gelir bir önceki yıla göre %18 arttı.
- Yeni iki bölge ofisi açıldı.
- Müşteri memnuniyeti 4,6 / 5 oldu.

## Çeyrek sonuçları

| Çeyrek | Gelir (milyon ₺) | Değişim |
|---|---:|---:|
| Q1 | 42,1 | +12% |
| Q2 | 47,8 | +15% |
| Q3 | 51,3 | +19% |
| Q4 | 58,0 | +24% |

> Rakamlar denetlenmemiş ön sonuçlardır.

Ayrıntılar için https://example.com/rapor adresine bakın.

<script>alert('bu metin olarak görünmeli')</script>
`;

const forcedLang = new URLSearchParams(location.search).get('lang');
let settings: Settings = {
  outputDir: OUT,
  extractImages: true,
  workers: 2,
  language: forcedLang === 'en' || forcedLang === 'tr' ? forcedLang : 'auto',
  ocr: { enabled: false, engine: 'auto', language: 'auto', tesseractPath: null },
};
let cancelDownload = false;
let cancelRun: (() => void) | null = null;

function outputOf(path: string): string {
  const name = path.split('\\').pop() ?? path;
  return `${OUT}\\${name.replace(/\.[^.]+$/, '')}.md`;
}

const wait = (ms: number) => new Promise((r) => setTimeout(r, ms));

async function fakeConvert(files: string[], channel: Channel<ConvertEvent>): Promise<void> {
  const send = (e: ConvertEvent) => channel.onmessage(e);
  let cancelled = false;
  let release: () => void = () => {};
  const hang = new Promise<void>((r) => (release = r));
  cancelRun = () => {
    cancelled = true;
    release();
  };
  send({ event: 'started', data: { total: files.length } });
  let ok = 0;
  let failed = 0;
  let nCancelled = 0;
  await Promise.all(
    files.map(async (path, i) => {
      await wait(20 * i);
      send({ event: 'fileStarted', data: { path } });
      if (path.endsWith('.docx')) {
        await wait(60);
        failed++;
        send({ event: 'fileFailed', data: { path, kind: 'corrupt', message: 'word/document.xml: unexpected end of zip archive' } });
        return;
      }
      if (path.endsWith('.pptx')) {
        // Stays mid-conversion so screenshots show a progress bar and the Cancel button.
        for (const f of [0.2, 0.41, 0.62]) {
          await wait(40);
          send({ event: 'progress', data: { path, fraction: f } });
        }
        await hang;
        nCancelled++;
        send({ event: 'fileCancelled', data: { path } });
        return;
      }
      for (const f of [0.3, 0.7]) {
        await wait(30);
        if (cancelled) break;
        send({ event: 'progress', data: { path, fraction: f } });
      }
      ok++;
      send({ event: 'fileFinished', data: { path, output: outputOf(path), warnings: path.endsWith('.pdf') ? ['Page 4: an image could not be extracted.'] : [] } });
    }),
  );
  cancelRun = null;
  send({ event: 'finished', data: { ok, failed, cancelled: nCancelled } });
}

async function fakeDownload(channel: Channel<DownloadEvent>): Promise<void> {
  cancelDownload = false;
  for (let percent = 0; percent <= 100; percent += 20) {
    if (cancelDownload) {
      channel.onmessage({ event: 'cancelled' });
      return;
    }
    channel.onmessage({ event: 'progress', data: { downloaded: Math.round(percent * 122_126.56), total: 12_212_656 } });
    await wait(80);
  }
  channel.onmessage({ event: cancelDownload ? 'cancelled' : 'finished' } as DownloadEvent);
}

const OCR_STATUS: OcrStatus = {
  windowsBuilt: true,
  windowsLanguages: ['tr'],
  tesseractPath: null,
  tesseractLanguages: [],
  ocrsBuilt: true,
  ocrsModels: false,
  ocrsDownloadBytes: 12_212_656,
  selected: 'windows',
  downloading: false,
};

export function installDevMock(): void {
  mockWindows('main');
  mockConvertFileSrc('windows');
  mockIPC(
    async (cmd, payload) => {
      const args = (payload ?? {}) as Record<string, unknown>;
      switch (cmd) {
        case 'get_settings':
          return settings;
        case 'set_settings':
          settings = { ...(args.settings as Settings) };
          return settings;
        case 'inspect_files': {
          const paths = args.paths as string[];
          return { entries: SAMPLES.filter((s) => paths.includes(s.path)), truncated: false };
        }
        case 'convert':
          return fakeConvert(args.files as string[], args.onEvent as Channel<ConvertEvent>);
        case 'cancel':
          cancelRun?.();
          return null;
        case 'read_output':
          return {
            text: SAMPLE_MARKDOWN,
            truncated: true,
            totalBytes: 5_033_165,
            shownBytes: 65_300,
            headings: [
              { level: 1, text: 'Yıllık rapor 2026', inPreview: true },
              { level: 2, text: 'Öne çıkanlar', inPreview: true },
              { level: 2, text: 'Çeyrek sonuçları', inPreview: true },
              { level: 2, text: 'Bölge ofisleri', inPreview: false },
              { level: 3, text: 'İstanbul', inPreview: false },
              { level: 3, text: 'Ankara', inPreview: false },
              { level: 2, text: 'Ekler', inPreview: false },
            ],
            headingsTruncated: true,
          };
        case 'ocr_status': {
          const draft = args.draft as Settings['ocr'] | null;
          const engine = draft?.engine ?? settings.ocr.engine;
          const selected = engine === 'tesseract' ? null : engine === 'ocrs' ? (OCR_STATUS.ocrsModels ? 'ocrs' : null) : OCR_STATUS.selected;
          return { ...OCR_STATUS, selected };
        }
        case 'download_ocr_models':
          return fakeDownload(args.onEvent as Channel<DownloadEvent>);
        case 'cancel_download':
          cancelDownload = true;
          return null;
        case 'plugin:app|version':
          return '0.1.0';
        case 'plugin:dialog|open':
          return (args.options as { directory?: boolean })?.directory ? 'C:\\Users\\Kerem\\Desktop\\Markdown' : SAMPLES.map((s) => s.path);
        default:
          return null;
      }
    },
    { shouldMockEvents: true },
  );
}

export function runScenario(actions: Actions): void {
  const scenario = new URLSearchParams(location.search).get('scenario') ?? 'queue';
  if (scenario === 'empty') return;
  if (scenario === 'drag') {
    actions.setDragging(true);
    return;
  }
  actions.addPaths(SAMPLES.map((s) => s.path));
  if (scenario === 'idle') return;
  setTimeout(() => {
    actions.convertPaths([SAMPLES[0].path, SAMPLES[1].path, SAMPLES[4].path]);
  }, 50);
  setTimeout(() => {
    if (scenario === 'preview') actions.openPreview(SAMPLES[0].path);
    if (scenario === 'settings') actions.openSettings();
    if (scenario === 'details') actions.showDetails(SAMPLES[4].path);
    if (scenario === 'menu') document.querySelectorAll<HTMLButtonElement>('.row:not(.row-head) .btn-icon')[0]?.click();
  }, 600);
}
