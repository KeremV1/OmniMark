# MarkItDown test dosyaları

Bu klasördeki dosyalar Microsoft [MarkItDown](https://github.com/microsoft/markitdown) projesinden
(sürüm **0.1.8**, 2026-10-15'te alınan kopya) kopyalandı ve OmniMark dönüştürücülerinin golden
testlerinde kullanılır. MarkItDown MIT lisanslıdır; lisans metni `LICENSE-MARKITDOWN` dosyasında.

| Klasör | İçerik |
| --- | --- |
| `files/` | `packages/markitdown/tests/test_files/` içindeki tüm örnek dosyalar (üretici `.py` betikleri hariç) |
| `ocr/` | `packages/markitdown-ocr/tests/ocr_test_data/` içinden 100 KB altı dosyalar + `pdf_scanned_minimal.pdf` |
| `reference/tests/` | MarkItDown testleri (`_test_vectors.py` dahil): beklenen `must_include` / `must_not_include` dizeleri |
| `reference/ocr-tests/` | OCR eklentisinin testleri |

`reference/` altındaki Python dosyaları **çalıştırılmaz**, yalnızca beklenen çıktıların kaynağıdır.
Hangi dosyanın kapsamda olduğu ve beklentileri [docs/research/markitdown-davranis-spec.md](../../../docs/research/markitdown-davranis-spec.md)
belgesinde. O belgedeki `markitdown-main/...` satır referansları MarkItDown 0.1.8 kaynağına aittir.
