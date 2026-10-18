import { describe, expect, it } from 'vitest';
import { setDialogError } from './settings';

describe('settings dialog error line', () => {
  it('shows the message inside the dialog and hides it again on success', () => {
    const line: Pick<HTMLElement, 'textContent' | 'hidden'> = { textContent: '', hidden: true };
    setDialogError(line, 'Dosya okunamadı veya yazılamadı.');
    expect(line).toEqual({ textContent: 'Dosya okunamadı veya yazılamadı.', hidden: false });
    setDialogError(line, null);
    expect(line).toEqual({ textContent: '', hidden: true });
  });
});
