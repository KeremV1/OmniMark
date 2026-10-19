import { describe, expect, it } from 'vitest';
import { matchHeadings, normalizeHeading } from './outline';

const o = (level: number, text: string) => ({ level, text });

describe('outline', () => {
  it('normalizes inline markdown and case', () => {
    expect(normalizeHeading('**Bold**  `code` Title')).toBe('bold code title');
  });
  it('matches outline headings to rendered headings in order', () => {
    const outline = [o(1, 'Intro'), o(2, 'A'), o(2, 'A')];
    const rendered = [o(1, 'Intro'), o(2, 'A'), o(2, 'A')];
    expect(matchHeadings(outline, rendered)).toEqual([0, 1, 2]);
  });
  it('skips rendered headings the outline does not know (setext) and unmatched outline entries', () => {
    const outline = [o(1, 'Intro'), o(2, 'Missing'), o(2, 'End')];
    const rendered = [o(1, 'Intro'), o(2, 'Setext only'), o(2, 'End')];
    expect(matchHeadings(outline, rendered)).toEqual([0, null, 2]);
  });
  it('tolerates link syntax in the outline text', () => {
    expect(matchHeadings([o(2, '[Docs](https://x.y)')], [o(2, 'Docs')])).toEqual([0]);
  });
  it('requires equal levels', () => {
    expect(matchHeadings([o(2, 'A')], [o(3, 'A')])).toEqual([null]);
  });
});
