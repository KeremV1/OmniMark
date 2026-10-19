<p align="center">
  <img src="docs/media/hero-dark.png" alt="OmniMark: dosya kuyruğu ve açık Markdown önizlemesi (koyu tema)" width="100%">
</p>

<p align="center">
  <b>Her dosyayı Markdown'a çevirin.</b><br>
  PDF, Word, Excel, PowerPoint, HTML ve daha fazlası; hafif, hızlı, çok platformlu masaüstü uygulaması.
</p>

<p align="center">
  <img alt="Rust" src="https://img.shields.io/badge/Rust-1.90%2B-B7410E?style=flat-square">
  <img alt="Tauri 2" src="https://img.shields.io/badge/Tauri-2-24C8DB?style=flat-square">
  <img alt="Tests" src="https://img.shields.io/badge/tests-186%20passing-15803D?style=flat-square">
  <img alt="Platforms" src="https://img.shields.io/badge/Windows%20%C2%B7%20macOS%20%C2%B7%20Linux-5A5F6E?style=flat-square">
  <a href="LICENSE"><img alt="MIT" src="https://img.shields.io/badge/lisans-MIT-4F46E5?style=flat-square"></a>
</p>

---

# OmniMark

**OmniMark**, farklı dosya formatlarını Markdown'a (`.md`) dönüştüren, hafif, hızlı ve çok platformlu bir masaüstü uygulamasıdır.

> **Durum:** ✅ M1 tamamlandı (19/19 görev) ve `main`'e birleşti. Windows'ta çalışan bir masaüstü uygulaması ve komut satırı aracı var; sıradaki aşama M2.

## Kaldığımız yer

![OmniMark: dosya kuyruğu ve önizleme](docs/media/preview-dark.png)

**Geldiğimiz nokta (2026-10-19):** M1 bitti; ardından büyük belge önizlemesi hafifletildi.
- **Dönüştürücüler:** PDF (pdfium; tablo ve form tespiti), Word (başlık, liste, tablo, görsel, dipnot, formül → LaTeX), Excel (XLSX/XLSM/XLS/ODS), PowerPoint (slayt, tablo, grafik, not), HTML, CSV (`;`/sekme tespiti), TXT/MD/JSON.
- **Masaüstü uygulaması (Tauri 2):** dosya kuyruğu, sürükle-bırak, ilerleme, filtreler, Markdown önizleme (Görünüm/Kaynak), ayarlar, kısayollar, açık/koyu tema, Türkçe/İngilizce.
- **Komut satırı:** `omnimark convert dosya.pdf -o çıktı`.
- **Güvenlik:** kötü niyetli dosyalara karşı bellek/boyut sınırları, harici bağlantılar asla açılmaz, çökmeler dosya bazında yalıtılır, arayüz yalnızca çıktı klasöründeki `.md` dosyalarını okuyabilir.
- **Önizleme:** belgenin ilk ~64 KB'ı çizilir; üstte belgenin **tamamının başlık listesi** görünür; kesildiyse "ilk 64 KB gösteriliyor" şeridi ve **Varsayılan uygulamada aç** düğmesi çıkar. Büyük belgede önizleme belleği +294 MB'tan **+24 MB**'a indi (hedef +50 MB ✅); kapatınca ~21 MB WebView2'de kalıyor (önizleme boyutundan bağımsız).
- **Testler:** 160 Rust + 26 arayüz testi geçiyor; CI her push'ta Windows, macOS ve Linux'ta yeşil.
- **Ölçümler (Windows, kurulu sürüm):** kurulum dosyası **5,4 MB** · pencere açılışı **59 ms** · boşta bellek **71 MB** (Görev Yöneticisi "Bellek" toplamı, WebView2 dahil; hedef < 100 MB ✅). Ayrıntı: [docs/m1-olcumler.md](docs/m1-olcumler.md).

**Sıradaki iş:**
1. **Elle yapılacak testler** (aşağıda) ve sonuçlarına göre düzeltmeler.
2. **M2 (sürüyor):** görsel dosyaları (EXIF/XMP metadata) ve isteğe bağlı OCR — [M2 tasarımı](docs/superpowers/specs/2026-10-19-omnimark-m2-design.md).
3. **M3:** ses transkripsiyonu (yerel Whisper, isteğe bağlı modül).

**Elle yapılacak testler** (aşağıda) ve sonuçlarına göre düzeltmeler.
3. **M3:** ses transkripsiyonu (yerel Whisper, isteğe bağlı modül).

**Elle yapılacak testler** (otomatikleştirilemeyenler):
- [ ] Explorer'dan dosya ve klasör sürükle-bırak (klasörde desteklenmeyen dosyalar bilerek atlanır).
- [ ] **Dosya ekle** ve **Değiştir** (klasör seçme) pencereleri.
- [ ] Dönüştürme sırasında **İptal**.
- [ ] Önizlemede **Kopyala**: WebView2 pano izni soruyor mu?
- [ ] Ayarlarda dil değişimi, paralel işçi sayısı, görselleri çıkar seçeneği.
- [ ] Büyük bir PDF/Word ile dönüştürme ve önizleme: başlık listesi, "ilk 64 KB" şeridi, **Varsayılan uygulamada aç**; Görev Yöneticisi'nde bellek.

Kontrol edilecek diğer konular: oturum boyunca senin adına verilen kararlar [HANDOFF.md](HANDOFF.md) dosyasında listeli.

Ayrıntı: [HANDOFF.md](HANDOFF.md) · [plan](docs/superpowers/plans/2026-10-15-omnimark-m1/00-genel.md) · [tasarım](docs/superpowers/specs/2026-10-15-omnimark-m1-design.md) · [ölçümler](docs/m1-olcumler.md)

## Ekranlar

<table>
  <tr>
    <td align="center"><img src="docs/media/queue-light.png" width="480" alt="Kuyruk"><br><sub><b>Kuyruk</b><br>Eklenen dosyalar, tür ve boyut; desteklenmeyenler soluk görünür (açık tema)</sub></td>
    <td align="center"><img src="docs/media/converting-dark.png" width="480" alt="Dönüştürme sürüyor"><br><sub><b>Dönüştürme sürüyor</b><br>Dosya başına ilerleme, uyarılar, hata ve İptal (koyu tema)</sub></td>
  </tr>
  <tr>
    <td align="center"><img src="docs/media/preview-dark.png" width="480" alt="Önizleme ve başlık listesi"><br><sub><b>Önizleme + başlık listesi</b><br>Büyük belgede ilk 64 KB çizilir; belgenin tamamının başlıkları üstte listelenir (koyu tema)</sub></td>
    <td align="center"><img src="docs/media/drag-dark.png" width="480" alt="Sürükle-bırak"><br><sub><b>Sürükle-bırak</b><br>Dosyaları ya da klasörü pencereye bırakın (koyu tema); <a href="docs/media/empty-light.png">boş ekran</a></sub></td>
  </tr>
  <tr>
    <td align="center"><img src="docs/media/settings-light.png" width="480" alt="Ayarlar"><br><sub><b>Ayarlar</b><br>Çıktı klasörü, görseller, paralel işçi sayısı, dil ve Hakkında</sub></td>
    <td align="center"><img src="docs/media/converting-en-light.png" width="480" alt="English interface"><br><sub><b>İngilizce arayüz</b><br>Türkçe ve İngilizce, sistem diline göre ya da ayarlardan (açık tema)</sub></td>
  </tr>
</table>

## Desteklenen formatlar

| Format | Uzantılar | Çıktı |
| --- | --- | --- |
| PDF | `.pdf` | Metin, mümkünse tablolar |
| Word | `.docx` | Başlıklar, listeler, tablolar, bağlantılar, formüller (LaTeX) |
| Excel | `.xlsx`, `.xlsm`, `.xls`, `.ods` | Her sayfa için ayrı başlık ve Markdown tablosu |
| PowerPoint | `.pptx` | Slayt slayt başlıklar, metinler, tablolar, grafik verileri, konuşmacı notları |
| HTML | `.html`, `.htm` | Gövde içeriği; script ve stil blokları ayıklanır |
| Görsel (M2) | `.png`, `.jpg`, `.jpeg`, `.webp` … | EXIF metadata, isteğe bağlı OCR |
| Ses (M3) | `.wav`, `.mp3`, `.m4a` … | Metadata, isteğe bağlı konuşmadan metne çeviri |
| Metin | `.txt`, `.md`, `.markdown`, `.json`, `.jsonl`, `.csv` | Düz metin; JSON kod bloğu; CSV için Markdown tablosu |

Görsel ve ses M2/M3'te gelecek. Kütüphane seçimleri: [docs/research/](docs/research/).

OCR ve ses transkripsiyonu **isteğe bağlı modüller** olacak. Gerekli modeller ilk kullanımda indirilecek, böylece temel kurulum küçük kalacak.

## Kullanım akışı

1. OmniMark'ı aç.
2. Dosyaları pencereye sürükle-bırak ya da **Dosya Ekle** ile seç.
3. Dosyalar kuyrukta listelenir: ad, tür, boyut, durum.
4. **Markdown Oluştur**'a bas; ilerleme ve hatalar dosya bazında gösterilir.
5. Markdown dosyaları seçtiğin klasöre kaydedilir.
6. Çıktı klasörünü aç ya da sonucu uygulama içinde önizle.

## Mimari

```
OmniMark
├── Rust çekirdeği      dosya okuma, format tespiti, dönüştürücüler
│   └── her format için ayrı dönüştürücü modülü (pdf, docx, xlsx, pptx, html, image, audio …)
├── Tauri 2 kabuğu      pencere, sürükle-bırak, dosya/klasör diyalogları, işletim sistemi entegrasyonu
└── Arayüz              HTML/CSS/TypeScript; kuyruk, ilerleme, önizleme
```

Dönüştürme akışı MarkItDown'daki modelden uyarlanıyor: her dönüştürücü önce dosyayı kabul edip etmediğini söyler (uzantı, MIME türü, içerik imzası), ardından dosyayı Markdown'a çevirir. Son adımda satır sonu boşlukları temizlenir ve art arda gelen boş satırlar sadeleştirilir.

## Yol haritası

| Aşama | Kapsam | Yol haritası adımları | Durum | İlerleme |
| --- | --- | --- | --- | --- |
| **M1** | Uygulama iskeleti, sürükle-bırak kuyruğu, dönüştürme akışı, önizleme; PDF, Word, Excel, PowerPoint, HTML, CSV, TXT/JSON | 1 · 2 · 3 · 4 | ✅ Tamamlandı | 19/19 görev |
| **M2** | Görsel metadata (EXIF) ve OCR (isteğe bağlı modül) | 5 | 🔄 Sürüyor | tasarım hazır |
| **M3** | Ses transkripsiyonu (yerel Whisper, isteğe bağlı modül) | 6 | ⏳ Planlandı | — |
| **M4** | Sağlamlaştırma: hata yönetimi, ek testler, performans iyileştirmeleri | 7 | ⏳ Planlandı | — |
| **M5** | Windows, macOS ve Linux kurulum paketleri, imzalama | 8 | ⏳ Planlandı | — |

Yol haritası adımları: **1** proje iskeleti ve temel arayüz · **2** sürükle-bırak ve dosya kuyruğu · **3** ilk çalışan Markdown dönüştürme akışı · **4** PDF, DOCX, XLSX, PPTX ve HTML desteği · **5** görsel metadata ve OCR · **6** ses transkripsiyonu · **7** hata yönetimi, testler ve performans · **8** Windows, macOS ve Linux paketleri.

Durum işaretleri: ✅ tamamlandı · 🔄 sürüyor · ⏳ planlandı.

**Temel ilke:** Önce küçük, çalışan ve güvenilir bir uygulama; ardından formatlar ve gelişmiş özellikler aşamalı olarak eklenir. Tüm platformlarda aynı temel deneyim korunur.

## Platform desteği

| Platform | Geliştirme ve derleme | Otomatik testler (CI) | Kurulum paketi | Durum |
| --- | --- | --- | --- | --- |
| **Windows** 10/11 (x64) | ✅ Derleniyor, tüm testler geçiyor | ✅ GitHub Actions | 🔄 NSIS (`.exe`, 5,4 MB) yerelde üretiliyor; imzalı dağıtım M5 | ✅ M1 çalışıyor |
| **macOS** (Apple Silicon ve Intel) | ✅ CI üzerinde derleniyor | ✅ GitHub Actions | ⏳ M5 · `.dmg` (imzalama için Apple Developer hesabı gerekir) | 🔄 CI'da test ediliyor, elle denenmedi |
| **Linux** (x64; Ubuntu 22.04+ ve benzerleri) | ✅ CI üzerinde derleniyor | ✅ GitHub Actions | ⏳ M5 · AppImage ve `.deb` | 🔄 CI'da test ediliyor, elle denenmedi |

## Geliştirme

Gereksinimler:

- Rust 1.90+ (Windows'ta MSVC toolchain)
- Windows için Visual Studio 2022 Build Tools (C++ iş yükü) ve WebView2
- Node.js 20+
- PDF için pdfium: **derlemeden önce** `scripts/fetch-pdfium.ps1` (Windows) ya da `scripts/fetch-pdfium.sh` (macOS/Linux) çalıştırılmalı. Sabit sürüm `chromium/8066`, SHA-256 doğrulamalı ([bblanchon/pdfium-binaries](https://github.com/bblanchon/pdfium-binaries)). `vendor/pdfium` olmadan masaüstü uygulaması derlenmez.

Komutlar:

```powershell
scripts/fetch-pdfium.ps1                                  # pdfium indir (macOS/Linux: bash scripts/fetch-pdfium.sh)
npm ci                                                    # arayüz bağımlılıkları
npm run tauri dev                                         # masaüstü uygulaması (geliştirme modu)
npm run tauri build -- --bundles nsis                     # Windows kurulum dosyası
cargo test --workspace                                    # Rust testleri (golden dahil)
npm test                                                  # arayüz testleri
cargo run -p omnimark-cli -- convert dosya.pdf -o çıktı   # komut satırı
```

Not: proje yolunda `&` varsa npm/tauri komutlarını `subst O: "<proje yolu>"` ile `O:\` üzerinden çalıştır.

Araştırma notları:

- [Rust kütüphaneleri](docs/research/rust-kutuphaneleri.md): format başına kütüphane karşılaştırması ve öneriler
- [Tauri 2 rehberi](docs/research/tauri2-rehberi.md): kurulum, sürükle-bırak, ilerleme, paketleme
- [MarkItDown davranış spesifikasyonu](docs/research/markitdown-davranis-spec.md): format başına çıktı kuralları ve golden test listesi

Golden test dosyaları [tests/fixtures/markitdown/](tests/fixtures/markitdown/) altında (MarkItDown 0.1.8, MIT).

## Teşekkür

Dönüştürme davranışı, Microsoft'un [MarkItDown](https://github.com/microsoft/markitdown) projesi (MIT lisansı) incelenerek tasarlanıyor. OmniMark, MarkItDown kodunu içermez; Rust ile sıfırdan yazılıyor. Test dosyaları ve beklenen çıktılar `tests/fixtures/markitdown/` altında, kendi MIT lisans notuyla (Microsoft Corporation) durur.

## Lisans

[MIT](LICENSE) © 2026 MoNoR (KeremV1).
