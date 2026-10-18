# MarkItDown'dan bilinçli sapmalar

Golden vektörler MarkItDown 0.1.8 testlerinden (`tests/fixtures/markitdown/reference/`) alınır. OmniMark'ın
bilerek farklı davrandığı yerler burada gerekçesiyle listelenir. Her dönüştürücü görevi kendi bölümünü ekler.

| Fixture | MarkItDown beklentisi | OmniMark beklentisi | Gerekçe |
| --- | --- | --- | --- |
| PDF fixture'ları | pdfminer/pdfplumber çıktısıyla birebir eşitlik | İçerik ve yapı doğrulaması; başlıklar `#` ile, madde imleri `- ` ile | pdfium farklı metin motoru; MarkItDown başlık üretmiyor |
| PDF form sayfaları | Tablo satırı ile düz metin satırı bitişik (satır sonu ile) | Tablo öncesi ve sonrası boş satır | Bitişik düz satır, Markdown'da tablo satırı sayılmasın |
