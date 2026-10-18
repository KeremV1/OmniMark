# MarkItDown'dan bilinçli sapmalar

Golden vektörler MarkItDown 0.1.8 testlerinden (`tests/fixtures/markitdown/reference/`) alınır. OmniMark'ın
bilerek farklı davrandığı yerler burada gerekçesiyle listelenir. Her dönüştürücü görevi kendi bölümünü ekler.

| Fixture | MarkItDown beklentisi | OmniMark beklentisi | Gerekçe |
| --- | --- | --- | --- |
| test.json, json_late_non_ascii.json | JSON metni olduğu gibi | ```` ```json ```` kod bloğu içinde | Markdown görüntüleyicilerde okunabilirlik |
| (satır içi testler) | `;` ayrılmış CSV tek sütun | `,` `;` sekme otomatik tespit | Türkçe Excel CSV'yi `;` ile kaydeder |
