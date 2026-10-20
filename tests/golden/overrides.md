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

## M2 (görsel ve OCR)

### Görsel (M2-1)

| Fixture | MarkItDown beklentisi | OmniMark beklentisi | Gerekçe |
| --- | --- | --- | --- |
| (satır içi testler) | `Keywords: ['a', 'b']` (Python liste biçimi) | `Keywords: a, b` | Okunabilirlik (spec §8 karar 7) |
| test.jpg ve tüm görseller | OCR yok (OCR yalnız Office/PDF içinde) | OCR açıkken metadata + boş satır + `*[Image OCR]` bloğu | spec §3.3, karar 6 |
| (satır içi testler) | exiftool ayrı kurulum ister, yoksa çıktı boş | Yerleşik EXIF/XMP okuyucu; HEIC, IPTC ve TIFF içi XMP okunmaz | Dış araç bağımlılığı yok |

### PDF OCR (M2-4)

| Fixture | markitdown-ocr beklentisi | OmniMark beklentisi | Gerekçe |
| --- | --- | --- | --- |
| pdf_image_*, pdf_multipage | Blok çevresinde 3-4 boş satır (ham çıktı) | Tek boş satır | Son normalizasyon |
| pdf_complex_layout | `ItemQuantity` (kelimeler yapışık) | `Item Quantity` | pdfium kelime boşluklarını korur |
| (OCR açık, tüm PDF'ler) | Yerleşik tablo/form ve başlık mantığı kaybolur | Korunur; bloklar satırlar arasına yerleşir | spec §3.1; markitdown §13 madde 2 |
| pdf_scanned_*, MEDRPT… | Gömülü raster görsel OCR'lanır; görselsiz "tarama" okunmaz | Metin katmanı olmayan sayfa 300 DPI (uzun kenar ≤ 4096) çizilip OCR'lanır | Her taranmış sayfa okunur |
| (OCR kapalı) | — | Uyarı: `Page N has no text layer (scanned?); enable OCR to read it.` | OCR artık var |

### Office OCR (M2-5)

| Fixture | markitdown-ocr beklentisi | OmniMark beklentisi | Gerekçe |
| --- | --- | --- | --- |
| docx_*, pptx_*, xlsx_* | `*[Image OCR]  \n…  \n[End OCR]*` (iki boşluklu satır sonu) | `*[Image OCR]\` + satır sonu (ters bölü sert satır sonu); `\ * _ [ ]` kaçışlı | Son normalizasyon satır sonu boşluklarını siler (00-genel karar 1) |
| pptx_* | Blok görsel bağlantısının **yerine** | `![alt](yol)` korunur, blok altında | spec §3.2, karar 5 |
| xlsx_* | `### Images in this sheet:` başlığı + bloklar, `NaN` hücreler | Görsel satırı + altında blok; başlık yok, boş hücre boş | spec §3.2; pandas artefaktı yok |
| docx (tablo hücresi) | Hücre içinde HTML blok | Tek satır `*[Image OCR] … [End OCR]*` | GFM hücresinde satır sonu olmaz |

### PDF tables (M2 fix)

| Fixture | MarkItDown beklentisi | OmniMark beklentisi | Gerekçe |
| --- | --- | --- | --- |
| (gerçek PDF'ler, ör. ders notları) | Hizalı satır dizisi, sütun sayısı kaç olursa olsun tablo olur; iki yana yaslı paragraf ve dağınık formül harfleri çoğunlukla boş hücreli tabloya dönüşür | Bir satır dizisi ancak şu dört kuralı geçerse tablo olur, aksi halde düz metin kalır: `min_run_fill` 0,5 (dolu hücre / (satır × kullanılan sütun)), `min_run_rows` 2, `max_prose_gap_ratio` 3,0 (satırın en geniş kelime boşluğu ≤ 3 × yazı boyutu ise "düzyazı benzeri"), `min_prose_row_share` 0,5 (düzyazı benzeri satır payı ≥ 0,5 ise ret) | Sahte tablo gürültüsünü kesmek (spec M2 son inceleme) |
| (gerçek PDF'ler) | Sayfada hiç tablo kalmasa da sayfa "form" olarak düz satırlarla yazılır | Hiçbir dizi kabul edilmezse sayfa form sayılmaz; normal yerleşim yolu (başlık, paragraf, OCR bloğu yerleşimi) kullanılır | Başlık/paragraf algısı ve OCR yerleşimi korunur |
| (gerçek PDF'ler) | — | Seyrek (düşük dolulukta) uzun satır dizileri için ayrı bir kural uygulanmadı: doluluk eşiği her uzunluktaki diziye uygulanır | Denendi: yalnız ≤ 3 satırlık dizileri elemek fizik notlarında 363 yerine 1790 tablo satırı üretti (üst sınır 450) |
