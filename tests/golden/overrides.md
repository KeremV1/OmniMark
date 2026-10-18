# MarkItDown'dan bilinçli sapmalar

Golden vektörler MarkItDown 0.1.8 testlerinden (`tests/fixtures/markitdown/reference/`) alınır. OmniMark'ın
bilerek farklı davrandığı yerler burada gerekçesiyle listelenir. Her dönüştürücü görevi kendi bölümünü ekler.

| Fixture | MarkItDown beklentisi | OmniMark beklentisi | Gerekçe |
| --- | --- | --- | --- |
| test.json, json_late_non_ascii.json | JSON metni olduğu gibi | ```` ```json ```` kod bloğu içinde | Markdown görüntüleyicilerde okunabilirlik |
| (satır içi testler) | `;` ayrılmış CSV tek sütun | `,` `;` sekme otomatik tespit | Türkçe Excel CSV'yi `;` ile kaydeder |
| test.xlsx, test.xls | Boş hücre `NaN`, `Unnamed: N`, `100.0` | Boş hücre boş, tam sayılar `.0`'sız | pandas artefaktları |
| test_wikipedia.html | Yalnız `#mw-content-text` (URL'ye bağlı WikipediaConverter) | Tüm gövde; kenar çubuğu dizeleri kontrol edilmez | Site-özel dönüştürücüler M1 kapsamı dışında |
| (satır içi testler) | `mailto:`/`tel:` bağlantıları düşer | Korunur | Kullanıcı için anlamlı bağlantılar |
| test.pptx | `![…](Picture4.jpg)` (var olmayan dosya, hep .jpg) | `test_assets/<gerçek ad>` dosyası | Görseller erişilebilir |
| (satır içi testler) | Grafik sonrası satır sonu yok, `10.0` | Satır sonu var, önbellek değeri (`10`) | Yapışan metin hatası; pandas/py float biçimi |
| PDF fixture'ları | pdfminer/pdfplumber çıktısıyla birebir eşitlik | İçerik ve yapı doğrulaması; başlıklar `#` ile, madde imleri `- ` ile | pdfium farklı metin motoru; MarkItDown başlık üretmiyor |
| PDF form sayfaları | Tablo satırı ile düz metin satırı bitişik (satır sonu ile) | Tablo öncesi ve sonrası boş satır | Bitişik düz satır, Markdown'da tablo satırı sayılmasın |
| test.docx | Görsel `data:image/png;base64...` olarak kesilir | `test_assets/imageN.png` dosyasına çıkarılır | Görseller kaybolmasın |
| test_with_comment.docx | Yorum metinleri (style_map ile) | Yorumlar yok | M1 kapsamı dışı |
| (satır içi testler) | Tablonun ilk satırı boş başlık | İlk satır başlık | Okunabilir GFM tablosu |
| equations.docx | `$m=1$`, `$$…$$` | Aynı; `m:sty=p` yalnız ASCII harf içeren run'ları `\mathrm{}` yapar, görünmez işlemciler (U+2061) atılır | Rakam/işleç run'ları zaten dik; `$m=1$` bozulmasın |
