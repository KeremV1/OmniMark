# MarkItDown'dan bilinçli sapmalar

Golden vektörler MarkItDown 0.1.8 testlerinden (`tests/fixtures/markitdown/reference/`) alınır. OmniMark'ın
bilerek farklı davrandığı yerler burada gerekçesiyle listelenir. Her dönüştürücü görevi kendi bölümünü ekler.

| Fixture | MarkItDown beklentisi | OmniMark beklentisi | Gerekçe |
| --- | --- | --- | --- |
| test.pptx | `![…](Picture4.jpg)` (var olmayan dosya, hep .jpg) | `test_assets/<gerçek ad>` dosyası | Görseller erişilebilir |
| (satır içi testler) | Grafik sonrası satır sonu yok, `10.0` | Satır sonu var, önbellek değeri (`10`) | Yapışan metin hatası; pandas/py float biçimi |
