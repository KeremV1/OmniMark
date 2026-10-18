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
