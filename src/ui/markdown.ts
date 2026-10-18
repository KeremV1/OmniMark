import MarkdownIt from 'markdown-it';
import { dirName, separatorOf } from './context';

// Raw HTML in the Markdown is shown as text, never rendered.
const md = new MarkdownIt({ html: false, linkify: true });

/** Unsanitized markdown-it output; the preview always passes it through DOMPurify. */
export function markdownToHtml(text: string): string {
  return md.render(text);
}

const SCHEME = /^[a-z][a-z0-9+.-]*:/i;

/** True for `img.png` or `report_assets/img.png`; false for URLs, data URIs, anchors and absolute paths. */
export function isRelativeSrc(src: string): boolean {
  if (src.length === 0 || SCHEME.test(src) || src.startsWith('/') || src.startsWith('\\') || src.startsWith('#')) return false;
  // Defence in depth (the asset scope already confines reads): never climb out of the output folder.
  return !decoded(src).split(/[\\/]/).includes('..');
}

function decoded(src: string): string {
  try {
    return decodeURI(src);
  } catch {
    return src;
  }
}

/** Resolves a relative image `src` against the folder of the Markdown file, in that file's path style. */
export function assetPath(markdownPath: string, src: string): string {
  const sep = separatorOf(markdownPath);
  const rel = decoded(src);
  return `${dirName(markdownPath)}${sep}${sep === '\\' ? rel.replace(/\//g, '\\') : rel}`;
}
