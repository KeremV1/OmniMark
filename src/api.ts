import { Channel, invoke } from '@tauri-apps/api/core';
import { open } from '@tauri-apps/plugin-dialog';

/** Mirrors `omnimark_core::detect::FileKind` (serialized camelCase). */
export type FileKind =
  | 'pdf'
  | 'word'
  | 'excel'
  | 'powerPoint'
  | 'html'
  | 'csv'
  | 'text'
  | 'json'
  | 'image'
  | 'zip'
  | 'audio'
  | 'unknown';

/** Error kinds from the core (`ErrorKind`) plus the app-level ones (`AppError`). */
export type ErrorKind =
  | 'unsupported'
  | 'corrupt'
  | 'encrypted'
  | 'tooLarge'
  | 'cancelled'
  | 'io'
  | 'missingComponent'
  | 'busy'
  | 'invalidPath';

export interface AppError {
  kind: ErrorKind;
  message: string;
}

export interface FileEntry {
  path: string;
  name: string;
  kind: FileKind;
  size: number;
  supported: boolean;
}

export type SettingsLanguage = 'auto' | 'en' | 'tr' | 'ru' | 'de' | 'es' | 'fr' | 'pt-BR';

export interface OcrSettings {
  enabled: boolean;
}

/** What a converted `.zip` becomes: a folder tree, one combined file or a zip of the folder tree. */
export type ZipOutput = 'folder' | 'single' | 'zip';

export interface Settings {
  outputDir: string;
  extractImages: boolean;
  workers: number;
  language: SettingsLanguage;
  ocr: OcrSettings;
  zipOutput: ZipOutput;
}

/** Which OCR engine will run and whether it is ready (`ocr_status`). */
export interface OcrStatus {
  /** `null` = this build has no OCR engine. */
  engine: 'windows' | 'ocrs' | null;
  ready: boolean;
  /** ocrs is the engine and its models still have to be downloaded. */
  needsDownload: boolean;
  ocrsDownloadBytes: number;
  downloading: boolean;
}

export type DownloadEvent =
  | { event: 'progress'; data: { downloaded: number; total: number } }
  | { event: 'finished' }
  | { event: 'failed'; data: { message: string } }
  | { event: 'cancelled' };

export interface OutlineHeading {
  level: number;
  text: string;
  /** The heading line lies inside the returned (capped) text. */
  inPreview: boolean;
}

export interface OutputText {
  text: string;
  truncated: boolean;
  /** Size of the whole file in bytes. */
  totalBytes: number;
  /** Exact byte length of `text`. */
  shownBytes: number;
  /** Headings of the whole document, not only of the returned text. */
  headings: OutlineHeading[];
  headingsTruncated: boolean;
}

/** Set on a finished `.zip`: how it was written and how many files were converted or skipped. */
export interface ZipInfo {
  mode: ZipOutput;
  converted: number;
  skipped: number;
}

export type ConvertEvent =
  | { event: 'started'; data: { total: number } }
  | { event: 'fileStarted'; data: { path: string } }
  | { event: 'progress'; data: { path: string; fraction: number } }
  | { event: 'fileFinished'; data: { path: string; output: string; warnings: string[]; zip?: ZipInfo | null } }
  | { event: 'fileFailed'; data: { path: string; kind: ErrorKind; message: string } }
  | { event: 'fileCancelled'; data: { path: string } }
  | { event: 'finished'; data: { ok: number; failed: number; cancelled: number } };

/** Extensions the M1 converters accept. */
export const SUPPORTED_EXTENSIONS = [
  'pdf', 'docx', 'xlsx', 'xlsm', 'xls', 'ods', 'pptx', 'html', 'htm', 'csv', 'txt', 'md', 'markdown', 'json', 'jsonl',
  'jpg', 'jpeg', 'png', 'webp', 'gif', 'bmp', 'tif', 'tiff', 'zip',
];

export function inspectFiles(paths: string[]): Promise<{ entries: FileEntry[]; truncated: boolean }> {
  return invoke('inspect_files', { paths });
}

/** Resolves when the batch ends; rejects with an `AppError` (for example `busy`). */
export function convert(files: string[], onEvent: (e: ConvertEvent) => void): Promise<void> {
  const channel = new Channel<ConvertEvent>();
  channel.onmessage = onEvent;
  return invoke('convert', { files, onEvent: channel });
}

export function cancel(): Promise<void> {
  return invoke('cancel');
}

export function readOutput(path: string): Promise<OutputText> {
  return invoke('read_output', { path });
}

/** Opens a produced `.md` in the default application. */
export function openOutput(path: string): Promise<void> {
  return invoke('open_output', { path });
}

/** Asks the backend to re-apply the WebView2 memory level (no-op off Windows). */
export function trimMemory(): Promise<void> {
  return invoke('trim_memory');
}

export function openOutputDir(): Promise<void> {
  return invoke('open_output_dir');
}

export function reveal(path: string): Promise<void> {
  return invoke('reveal', { path });
}

export function openLogDir(): Promise<void> {
  return invoke('open_log_dir');
}

export function getSettings(): Promise<Settings> {
  return invoke('get_settings');
}

/** Returns the sanitized settings the backend actually stored. */
export function setSettings(settings: Settings): Promise<Settings> {
  return invoke('set_settings', { settings });
}

/** `allFilesLabel` is the localized name of the catch-all filter, for example `t('dialog.allFiles')`. */
export async function pickFiles(allFilesLabel: string): Promise<string[]> {
  const picked = await open({
    multiple: true,
    directory: false,
    filters: [
      { name: 'OmniMark', extensions: SUPPORTED_EXTENSIONS },
      { name: allFilesLabel, extensions: ['*'] },
    ],
  });
  return picked ?? [];
}

export async function pickFolder(): Promise<string | null> {
  const picked = await open({ multiple: false, directory: true });
  return picked ?? null;
}

export function ocrStatus(): Promise<OcrStatus> {
  return invoke('ocr_status');
}

/** Resolves when the download ends (see the last event for the outcome); rejects with an `AppError` (for example `busy`). */
export function downloadOcrModels(onEvent: (e: DownloadEvent) => void): Promise<void> {
  const channel = new Channel<DownloadEvent>();
  channel.onmessage = onEvent;
  return invoke('download_ocr_models', { onEvent: channel });
}

export function cancelDownload(): Promise<void> {
  return invoke('cancel_download');
}
