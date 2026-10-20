<p align="center">
  <img src="docs/media/hero-dark.png" alt="OmniMark: dosya kuyruğu ve açık Markdown önizlemesi (koyu tema)" width="100%">
</p>

<p align="center">
  <b>Her dosyayı Markdown'a çevirin.</b><br>
  PDF, Word, Excel, PowerPoint, HTML, görseller ve daha fazlası; hafif, hızlı, çok platformlu masaüstü uygulaması.
</p>

<p align="center">
  <img alt="Rust" src="https://img.shields.io/badge/Rust-1.90%2B-B7410E?style=flat-square">
  <img alt="Tauri 2" src="https://img.shields.io/badge/Tauri-2-24C8DB?style=flat-square">
  <img alt="Tests" src="https://img.shields.io/badge/tests-279%20passing-15803D?style=flat-square">
  <img alt="Platforms" src="https://img.shields.io/badge/Windows%20%C2%B7%20macOS%20%C2%B7%20Linux-5A5F6E?style=flat-square">
  <img alt="Languages" src="https://img.shields.io/badge/aray%C3%BCz-7%20dil-0E7490?style=flat-square">
  <a href="LICENSE"><img alt="MIT" src="https://img.shields.io/badge/lisans-MIT-4F46E5?style=flat-square"></a>
</p>

---

# OmniMark

**OmniMark**, belgeleri, tabloları, sunumları, web sayfalarını ve görselleri temiz Markdown'a (`.md`) dönüştüren bir masaüstü uygulamasıdır. Her şey yerelde çalışır, hiçbir dosya buluta gönderilmez. Rust ve Tauri 2 ile yazıldığı için hızlı açılır, küçük kalır (≈7 MB kurulum) ve boşta yaklaşık 71 MB bellek kullanır.

> **Durum:** ✅ M1 (dönüştürücüler + masaüstü uygulaması) ve ✅ M2 (görsel metadata + OCR) tamamlandı. M2 kapanış turu: platform başına tek, otomatik OCR motoru, 7 dilli arayüz, ilk açılışta çıktı klasörü kartı ✅ ve **ZIP desteği** 🔄 (geliştiriliyor). Bunlar bitince **M3: ses transkripsiyonu** başlar. README yayına kadar Türkçe; yayında İngilizceye geçecek.

## Kaldığımız yer

**Geldiğimiz nokta (2026-10-20):** M2'nin 9 görevi bitti; M3'ten önce bir kapanış turu yapılıyor. Türkçe öncelikli M2 sonu hâli [`v0.1.0-tr`](https://github.com/KeremV1/OmniMark/releases/tag/v0.1.0-tr) etiketinde saklanıyor.

### M2 kapanış turu (M3 öncesi)

| İş | Ne yapıldı | Durum |
| --- | --- | --- |
| OCR sadeleştirme | Tesseract kaldırıldı; motor otomatik (Windows OCR / ocrs); ayarlarda tek anahtar | ✅ |
| Çok dilli arayüz | English (varsayılan), Türkçe, Русский, Deutsch, Español, Français, Português (Brasil); yalnız seçili dil yüklenir | ✅ |
| İlk açılış kartı | "Markdown dosyaları şuraya kaydedilecek: `<klasör>`" · **Değiştir** · **Tamam** (tek seferlik) | ✅ |
| ZIP desteği | `.zip` içindeki dosyalar dönüştürülür; çıktı klasör / tek `.md` / `.zip`; zip bombası ve yol kaçışı sınırları | 🔄 Geliştiriliyor |

### Sıradaki iş

1. **ZIP desteğini bitir**, `main`'e birleştir, M2 kapanış turunu kapat.
2. **M3:** ses transkripsiyonu (yerel Whisper, isteğe bağlı modül); önce tasarım, onayla.

Ayrıntı: [HANDOFF.md](HANDOFF.md) · [Kapanış turu tasarımı](docs/superpowers/specs/2026-10-20-m2-kapanis-sadelestirme.md) · [Çıktı klasörü ve ZIP tasarımı](docs/superpowers/specs/2026-10-20-cikti-klasoru-ve-zip.md) · [M2 ölçümleri](docs/m2-olcumler.md) · [OCR kalite ölçümü](docs/m2-ocr-kalite.md) · [M2 ertelenen bulgular](docs/m2-ertelenen-bulgular.md) · [M1 ölçümleri](docs/m1-olcumler.md)

## Öne çıkanlar

- **Birçok format, tek bırakma alanı:** dosyaları, klasörleri ya da ZIP arşivlerini sürükleyin, paralel dönüştürün, sonucu önizleyin.
- **Sadık çıktı:** başlıklar, listeler, tablolar, bağlantılar, formüller (LaTeX), konuşmacı notları, grafik verileri, sayfa sayfa tablolar. Davranış Microsoft [MarkItDown](https://github.com/microsoft/markitdown)'ı izler ve golden testlerle doğrulanır.
- **İsteğe bağlı OCR:** görsellerdeki, taranmış PDF sayfalarındaki ve Office belgelerindeki resimlerdeki metni okur. Varsayılan kapalı, tek anahtarla açılır.
- **Gizlilik:** her şey bilgisayarınızda çalışır.
- **Hafif:** ≈7 MB kurulum, boşta ≈71 MB bellek (hedef < 100 MB).
- **7 arayüz dili:** English, Türkçe, Русский, Deutsch, Español, Français, Português (Brasil). Sistem dilini izler, desteklenmiyorsa İngilizce açılır, Ayarlar'dan değiştirilebilir.

## Desteklenen formatlar

| Format | Uzantılar | Çıktı |
| --- | --- | --- |
| PDF | `.pdf` | Metin, mümkünse tablolar; gömülü görseller ve taranmış sayfalar için isteğe bağlı OCR |
| Word | `.docx` | Başlıklar, listeler, tablolar, bağlantılar, formüller (LaTeX); görsellerde isteğe bağlı OCR |
| Excel | `.xlsx`, `.xlsm`, `.xls`, `.ods` | Her sayfa için başlık ve Markdown tablosu; `.xlsx` görsellerinde isteğe bağlı OCR |
| PowerPoint | `.pptx` | Slayt slayt başlıklar, metinler, tablolar, grafik verileri, konuşmacı notları; görsellerde isteğe bağlı OCR |
| HTML | `.html`, `.htm` | Gövde içeriği; script ve stil blokları ayıklanır |
| Görsel | `.png`, `.jpg`, `.jpeg`, `.webp`, `.gif`, `.bmp`, `.tif`, `.tiff` | EXIF/XMP metadata, isteğe bağlı OCR |
| Metin | `.txt`, `.md`, `.markdown`, `.json`, `.jsonl`, `.csv` | Düz metin; JSON kod bloğu; CSV için Markdown tablosu |
| ZIP (🔄 geliştiriliyor) | `.zip` | İçindeki desteklenen dosyalar dönüştürülür. Çıktı: klasör (yapı korunur, varsayılan) · tek birleşik `.md` (MarkItDown davranışı) · `.zip`. İç içe zip açılmaz; desteklenmeyen dosyalar atlanıp bildirilir |
| Ses (M3) | `.wav`, `.mp3`, `.m4a` … | Metadata ve isteğe bağlı konuşmadan metne çeviri |

## OCR

OCR **isteğe bağlı bir modüldür** ve varsayılan olarak kapalıdır. **Ayarlar → OCR**'dan açıldığında motoru OmniMark seçer:

| Platform | Motor | İndirme |
| --- | --- | --- |
| Windows 10/11 | Windows'un yerleşik OCR'ı: hızlı (3 sayfalık taramada ≈0,7 sn), Windows'ta kurulu OCR dillerini kullanır | Yok |
| macOS, Linux (ve OCR dili olmayan Windows) | [ocrs](https://github.com/robertknight/ocrs), saf Rust; Latin alfabesi | İlk kullanımda ≈12 MB model |

Tanınan metin çıktıda `*[Image OCR]` … `[End OCR]*` blokları içinde yer alır. Komut satırında motor `--ocr=windows` ya da `--ocr=ocrs` ile zorlanabilir.

## Kullanım akışı

1. OmniMark'ı açın.
2. İsterseniz **Ayarlar → OCR**'ı açın.
3. Dosyaları, klasörleri ya da `.zip` arşivlerini pencereye bırakın veya **Dosya ekle** / **Klasör ekle**'yi kullanın.
4. İlk açılışta çıkan kart, Markdown dosyalarının kaydedileceği klasörü gösterir; **Değiştir** ile başka klasör seçebilirsiniz.
5. **Markdown Oluştur**'a basın. İlerleme ve hatalar dosya bazında gösterilir; satırdaki ✕ ya da **İptal** işlemi durdurur.
6. Markdown dosyaları seçtiğiniz çıktı klasörüne yazılır. Klasörü açın ya da sonucu uygulama içinde önizleyin.

## Mimari

```
OmniMark
├── Rust çekirdeği      dosya tespiti, dönüştürücüler, Markdown yardımcıları
│   ├── her format için ayrı dönüştürücü (pdf, docx, xlsx, pptx, html, image …)
│   └── OCR katmanı     motor arayüzü: Windows OCR · ocrs; görsel sınırları, önbellek
├── Komut satırı (CLI)  omnimark convert / models fetch
├── Tauri 2 kabuğu      pencere, sürükle-bırak, diyaloglar, işletim sistemi entegrasyonu
└── Arayüz              HTML/CSS/TypeScript; kuyruk, ilerleme, önizleme, 7 dil (talep üzerine yüklenir)
```

Her dönüştürücü önce dosyayı kabul edip etmediğini söyler (uzantı, MIME türü, içerik imzası), sonra Markdown'a çevirir. Son adımda satır sonu boşlukları temizlenir ve art arda boş satırlar sadeleştirilir.

## Yol haritası

| Aşama | Kapsam | Durum |
| --- | --- | --- |
| **M1** | Uygulama iskeleti, sürükle-bırak kuyruğu, dönüştürme akışı, önizleme; PDF, Word, Excel, PowerPoint, HTML, CSV, TXT/JSON | ✅ Tamamlandı (19/19 görev) |
| **M2** | Görsel metadata (EXIF/XMP) ve OCR · kapanış turu: sade OCR, 7 dilli arayüz, ilk açılış kartı, ZIP desteği | 🔄 9/9 görev ✅ · kapanış turu 3/4 (ZIP sürüyor) |
| **M3** | Ses transkripsiyonu (yerel Whisper, isteğe bağlı modül) | ⏭ Kapanış turundan sonra |
| **M4** | Sağlamlaştırma: hata yönetimi, ek testler, performans | ⏳ Planlandı |
| **M5** | Windows, macOS ve Linux için imzalı kurulum paketleri; README İngilizce | ⏳ Planlandı |

Durum işaretleri: ✅ tamamlandı · 🔄 sürüyor · ⏭ sıradaki · ⏳ planlandı.

## Platform desteği

| Platform | Derleme | CI | Kurulum paketi | Durum |
| --- | --- | --- | --- | --- |
| **Windows** 10/11 (x64) | ✅ | ✅ GitHub Actions | 🔄 NSIS `.exe` (≈7 MB) yerelde üretiliyor; imzalı dağıtım M5 | ✅ Elle test edildi |
| **macOS** (Apple Silicon, Intel) | ✅ CI'da | ✅ GitHub Actions | ⏳ M5 · `.dmg` | 🔄 Yalnız CI |
| **Linux** (x64, Ubuntu 22.04+) | ✅ CI'da | ✅ GitHub Actions | ⏳ M5 · AppImage, `.deb` | 🔄 Yalnız CI |

## Geliştirme

Gereksinimler:

- Rust 1.90+ (Windows'ta MSVC toolchain); Windows'ta Visual Studio 2022 Build Tools (C++) ve WebView2
- Node.js 20+
- pdfium, **derlemeden önce**: `scripts/fetch-pdfium.ps1` (Windows) ya da `scripts/fetch-pdfium.sh` (macOS/Linux). Sabit sürüm `chromium/8066`, SHA-256 doğrulamalı ([bblanchon/pdfium-binaries](https://github.com/bblanchon/pdfium-binaries)).

```powershell
scripts/fetch-pdfium.ps1                                   # pdfium (macOS/Linux: bash scripts/fetch-pdfium.sh)
npm ci                                                     # arayüz bağımlılıkları
npm run tauri dev                                          # masaüstü uygulaması (geliştirme modu)
npm run tauri build -- --bundles nsis                      # Windows kurulum dosyası
cargo test --workspace                                     # Rust testleri (golden dahil)
npm test                                                   # arayüz testleri
cargo run -p omnimark-cli -- convert dosya.pdf -o çıktı    # komut satırı
cargo run -p omnimark-cli -- convert foto.jpg --stdout --ocr
cargo run -p omnimark-cli -- models fetch ocrs             # ocrs modeli (≈12 MB)
```

Proje yolunda `&` varsa npm/Tauri bozulur: üst klasörü `subst O: "<üst klasör>"` ile bağlayın, `O:\OmniMark` içinde çalıştırın, bitince `subst O: /D`.

Yeni arayüz dili eklemek: `src/locales/en.ts`'i `src/locales/<kod>.ts` olarak kopyalayın, değerleri çevirin (eksik anahtarı TypeScript bildirir), dili `src/i18n.ts`'e ve `src-tauri/src/settings.rs`'teki `Language` ayarına ekleyin.

Araştırma notları: [Rust kütüphaneleri](docs/research/rust-kutuphaneleri.md) · [Tauri 2 rehberi](docs/research/tauri2-rehberi.md) · [MarkItDown davranış spesifikasyonu](docs/research/markitdown-davranis-spec.md). Golden test dosyaları [tests/fixtures/markitdown/](tests/fixtures/markitdown/) altında.

## Teşekkür

Dönüştürme davranışı, Microsoft'un [MarkItDown](https://github.com/microsoft/markitdown) projesi (MIT) incelenerek tasarlanıyor. OmniMark, MarkItDown kodunu içermez; Rust ile sıfırdan yazılıyor. Test dosyaları ve beklenen çıktılar `tests/fixtures/markitdown/` altında, kendi MIT lisans notuyla (Microsoft Corporation) durur.

## Lisans

[MIT](LICENSE) © 2026 MoNoR (KeremV1).
