import { describe, expect, it } from 'vitest';
import { dirName, shortPath } from './context';
import { assetPath, isRelativeSrc, markdownToHtml } from './markdown';

describe('preview markdown', () => {
  it('escapes raw HTML instead of rendering it', () => {
    const html = markdownToHtml('# Hi\n\n<script>alert(1)</script>\n\n<img src=x onerror=alert(1)>');
    expect(html).toContain('<h1>Hi</h1>');
    expect(html).not.toContain('<script>');
    expect(html).not.toContain('<img src=x');
    expect(html).toContain('&lt;script&gt;');
  });

  it('drops javascript links and linkifies plain URLs', () => {
    expect(markdownToHtml('[x](javascript:alert(1))')).not.toContain('href="javascript:');
    expect(markdownToHtml('see https://example.com')).toContain('href="https://example.com"');
  });

  it('resolves only relative image sources next to the Markdown file', () => {
    expect(isRelativeSrc('rapor_assets/image1.png')).toBe(true);
    expect(isRelativeSrc('a..b/c..png')).toBe(true);
    for (const src of [
      'https://x/y.png',
      'data:image/png;base64,AA',
      '/etc/x.png',
      'C:\\x.png',
      '#a',
      '',
      '../secret.png',
      'a_assets/../../x.png',
      'a_assets\\..\\x.png',
      '%2E%2E/x.png',
      'a/%2e%2e/%2e%2e/x.png',
    ]) {
      expect(isRelativeSrc(src)).toBe(false);
    }
    expect(assetPath('C:\\Users\\k\\OmniMark\\rapor.md', 'rapor_assets/g%C3%B6rsel%201.png')).toBe(
      'C:\\Users\\k\\OmniMark\\rapor_assets\\görsel 1.png',
    );
    expect(assetPath('/home/k/OmniMark/a.md', 'a_assets/i.png')).toBe('/home/k/OmniMark/a_assets/i.png');
  });

  it('shortens long output paths to their last folders', () => {
    expect(shortPath('C:\\Users\\alex\\Documents\\OmniMark')).toBe('…\\Documents\\OmniMark');
    expect(shortPath('/home/k/OmniMark')).toBe('/home/k/OmniMark');
    expect(dirName('/home/k/a.md')).toBe('/home/k');
  });
});
