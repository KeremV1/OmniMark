# MarkItDown'dan bilinçli sapmalar

Golden vektörler MarkItDown 0.1.8 testlerinden (`tests/fixtures/markitdown/reference/`) alınır. OmniMark'ın
bilerek farklı davrandığı yerler burada gerekçesiyle listelenir. Her dönüştürücü görevi kendi bölümünü ekler.

| Fixture | MarkItDown beklentisi | OmniMark beklentisi | Gerekçe |
| --- | --- | --- | --- |
| test.docx | Görsel `data:image/png;base64...` olarak kesilir | `test_assets/imageN.png` dosyasına çıkarılır | Görseller kaybolmasın |
| test_with_comment.docx | Yorum metinleri (style_map ile) | Yorumlar yok | M1 kapsamı dışı |
| (satır içi testler) | Tablonun ilk satırı boş başlık | İlk satır başlık | Okunabilir GFM tablosu |
