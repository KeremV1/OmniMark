# MarkItDown'dan bilinçli sapmalar

Golden vektörler MarkItDown 0.1.8 testlerinden (`tests/fixtures/markitdown/reference/`) alınır. OmniMark'ın
bilerek farklı davrandığı yerler burada gerekçesiyle listelenir. Her dönüştürücü görevi kendi bölümünü ekler.

| Fixture | MarkItDown beklentisi | OmniMark beklentisi | Gerekçe |
| --- | --- | --- | --- |
| test.xlsx, test.xls | Boş hücre `NaN`, `Unnamed: N`, `100.0` | Boş hücre boş, tam sayılar `.0`'sız | pandas artefaktları |
