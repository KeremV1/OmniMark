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
  <img alt="Tests" src="https://img.shields.io/badge/tests-277%20passing-15803D?style=flat-square">
  <img alt="Platforms" src="https://img.shields.io/badge/Windows%20%C2%B7%20macOS%20%C2%B7%20Linux-5A5F6E?style=flat-square">
  <a href="LICENSE"><img alt="MIT" src="https://img.shields.io/badge/lisans-MIT-4F46E5?style=flat-square"></a>
</p>

---

# OmniMark

**OmniMark**, farklı dosya formatlarını Markdown'a (`.md`) dönüştüren, hafif, hızlı ve çok platformlu bir masaüstü uygulamasıdır.

> **Durum:** ✅ M1 tamamlandı. ✅ M2 (görsel metadata + OCR) tamamlandı: **9/9 görev** ve son inceleme `main`'de. Sıradaki aşama M3 (ses transkripsiyonu); başlamadan önce son bir gözden geçirme yapılacak.

## Kaldığımız yer

**Geldiğimiz nokta (2026-10-20):** M1 ve M2 bitti. OmniMark görselleri dönüştürüyor ve isteğe bağlı OCR yapıyor: görsel dosyalarda, PDF'lerde ve Office belgelerinde. Masaüstü uygulamasında OCR ayarları var. Elle testler tamamlandı.

### Yapılanlar (M2)

| Görev | Ne yapıldı |
| --- | --- |
| M2-0 | OCR çekirdeği: ortak motor arayüzü, görsel çözme sınırları, önbellek, `*[Image OCR]` blokları |
| M2-1 | Görsel dönüştürücü: EXIF ve XMP metadata (MarkItDown alan sırası) |
| M2-2 | ocrs motoru: tüm platformlarda çalışır, ~12 MB model isteğe bağlı iner (SHA-256 doğrulamalı, zaman aşımlı, iptal edilebilir). Komut satırına `--ocr` ve `models fetch` eklendi |
| M2-3 | Windows OCR motoru ve üç motorun kalite ölçümü ([sonuçlar](docs/m2-ocr-kalite.md)). Türkçede hata oranı: Windows %0,3, Tesseract %1,2, ocrs %15,6 (ocrs Türkçe harfleri tanımıyor) |
| M2-4 | PDF'te OCR: sayfadaki görseller ve taranmış sayfalar. OCR pdfium kilidinin dışında çalışıyor, bellek sayfa başına sınırlı |
| M2-5 | Word, PowerPoint ve Excel'deki görsellerde OCR: aynı görsel iki kez okunmuyor |
| M2-6 | Masaüstü uygulaması: Ayarlar → OCR (aç/kapat, motor, tanıma dili, Tesseract programı). Motor durum satırları, ocrs modelini ilerleme çubuğuyla indirme ve durdurma |
| M2-7 | Ölçümler ([sonuçlar](docs/m2-olcumler.md)), CI'da üç platformda OCR testleri (Tesseract kurulumu, ocrs model önbelleği), belgeler |
| M2-8 | Tesseract motoru: sistemdeki program kabuksuz, sabit argümanlarla ve zaman aşımıyla çalışıyor |
| Elle test düzeltmeleri | Ayarlarda yanıltıcı "motor yok" satırı düzeltildi, Windows'ta Tesseract isteğe bağlı gösteriliyor; dönüşen dosyanın satırına İptal düğmesi; PDF'te düzyazının sahte tablolara dönmesi büyük ölçüde engellendi |
| Son inceleme düzeltmeleri | Büyük görselde bellek patlaması, Tesseract'ta Otomatik dil, Tesseract iptali, belge başına 1000 OCR sınırı, `--stdout` uyarıları |

- **Varsayılan:** OCR kapalı. Açıldığında motor otomatik seçilir: Windows → Tesseract → ocrs. Tanıma dili varsayılan olarak arayüz diline, arayüz dili de sistem diline uyar. Uygulama İngilizce öncelikli.
- **Ölçümler (hedeflerin hepsi tuttu):** kurulum dosyası 7,3 MB (hedef < 15 MB). Boşta bellek OCR kapalıyken 71,1 MB, açıkken 71,2 MB, Ayarlar bir kez açıldıktan sonra 79,5 MB (hedef < 100 MB). 3 sayfalık taranmış PDF: Windows OCR 0,7 sn, ocrs 8,6 sn, Tesseract 10 sn.
- **Nasıl çalışıldı:** görevleri Sonnet ajanları paralel yazdı, hepsini Opus inceledi; sonda tüm M2 Opus ile baştan incelendi ve bulgular tek turda kapatıldı.
- **Testler:** 246 Rust testi ve 31 arayüz testi geçiyor; clippy ve tsc temiz. CI Windows, macOS ve Linux'ta OCR testlerini de çalıştırıyor.
- **M1'den gelenler:** PDF, Word, Excel, PowerPoint, HTML, CSV, TXT/MD/JSON dönüştürücüleri. Tauri 2 uygulaması: kuyruk, sürükle-bırak, önizleme (ilk 64 KB ve başlık listesi), açık/koyu tema, Türkçe/İngilizce. Güvenlik sınırları ([M1 ölçümleri](docs/m1-olcumler.md)).

### Senin kontrol etmen gerekenler

- [ ] **M3'e geçmeden son gözden geçirme:** değiştirmek istediğin bir şey var mı? Aday küçük iyileştirmeler [HANDOFF.md](HANDOFF.md) → "Onay bekleyen kararlar" bölümünde.
- [ ] **Senin adına verilen kararlar:** aynı bölümde.

### Sıradaki iş

1. **Son gözden geçirme** (yukarıda). İstersen küçük iyileştirmeler M3'ten önce yapılır.
2. **M3:** ses transkripsiyonu (yerel Whisper, isteğe bağlı modül). Önce tasarım ve plan, senin onayınla.

Ayrıntı: [HANDOFF.md](HANDOFF.md) · [M2 planı](docs/superpowers/plans/2026-10-19-omnimark-m2/00-genel.md) · [M2 tasarımı](docs/superpowers/specs/2026-10-19-omnimark-m2-design.md) · [M2 ölçümleri](docs/m2-olcumler.md) · [OCR kalite ölçümü](docs/m2-ocr-kalite.md) · [M1 ölçümleri](docs/m1-olcumler.md)

## Desteklenen formatlar

| Format | Uzantılar | Çıktı |
| --- | --- | --- |
| PDF | `.pdf` | Metin, mümkünse tablolar |
| Word | `.docx` | Başlıklar, listeler, tablolar, bağlantılar, formüller (LaTeX) |
| Excel | `.xlsx`, `.xlsm`, `.xls`, `.ods` | Her sayfa için ayrı başlık ve Markdown tablosu |
| PowerPoint | `.pptx` | Slayt slayt başlıklar, metinler, tablolar, grafik verileri, konuşmacı notları |
| HTML | `.html`, `.htm` | Gövde içeriği; script ve stil blokları ayıklanır |
| Görsel | `.png`, `.jpg`, `.jpeg`, `.webp` … | EXIF/XMP metadata, isteğe bağlı OCR |
| Ses (M3) | `.wav`, `.mp3`, `.m4a` … | Metadata, isteğe bağlı konuşmadan metne çeviri |
| Metin | `.txt`, `.md`, `.markdown`, `.json`, `.jsonl`, `.csv` | Düz metin; JSON kod bloğu; CSV için Markdown tablosu |

Ses M3'te gelecek. Kütüphane seçimleri: [docs/research/](docs/research/).

OCR ve ses transkripsiyonu **isteğe bağlı modüller**: OCR varsayılan kapalı. Windows'un kendi OCR'ı indirme gerektirmez. Tesseract sistemde kuruluysa kullanılır. ocrs modeli (~12 MB) yalnız istenirse iner. Böylece temel kurulum küçük kalır.

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
| **M2** | Görsel metadata (EXIF) ve OCR (isteğe bağlı modül) | 5 | ✅ Tamamlandı | 9/9 görev |
| **M3** | Ses transkripsiyonu (yerel Whisper, isteğe bağlı modül) | 6 | ⏳ Planlandı | — |
| **M4** | Sağlamlaştırma: hata yönetimi, ek testler, performans iyileştirmeleri | 7 | ⏳ Planlandı | — |
| **M5** | Windows, macOS ve Linux kurulum paketleri, imzalama | 8 | ⏳ Planlandı | — |

Yol haritası adımları: **1** proje iskeleti ve temel arayüz · **2** sürükle-bırak ve dosya kuyruğu · **3** ilk çalışan Markdown dönüştürme akışı · **4** PDF, DOCX, XLSX, PPTX ve HTML desteği · **5** görsel metadata ve OCR · **6** ses transkripsiyonu · **7** hata yönetimi, testler ve performans · **8** Windows, macOS ve Linux paketleri.

Durum işaretleri: ✅ tamamlandı · 🔄 sürüyor · ⏳ planlandı.

**Temel ilke:** Önce küçük, çalışan ve güvenilir bir uygulama; ardından formatlar ve gelişmiş özellikler aşamalı olarak eklenir. Tüm platformlarda aynı temel deneyim korunur.

## Platform desteği

| Platform | Geliştirme ve derleme | Otomatik testler (CI) | Kurulum paketi | Durum |
| --- | --- | --- | --- | --- |
| **Windows** 10/11 (x64) | ✅ Derleniyor, tüm testler geçiyor | ✅ GitHub Actions | 🔄 NSIS (`.exe`, 7,3 MB) yerelde üretiliyor; imzalı dağıtım M5 | ✅ M2 çalışıyor (elle test edildi) |
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
cargo run -p omnimark-cli -- convert foto.jpg --stdout --ocr        # OCR (Windows → Tesseract → ocrs)
cargo run -p omnimark-cli -- models fetch ocrs            # ocrs modelini indir (~12 MB)
scripts/fetch-tesseract.ps1                               # isteğe bağlı: OCR testleri için proje içi Tesseract (Windows)
```

Not: proje yolunda `&` varsa npm/tauri komutlarını üst klasörü `subst O: "<üst klasör>"` ile bağlayıp `O:\OmniMark` üzerinden çalıştır; bitince `subst O: /D`.

Araştırma notları:

- [Rust kütüphaneleri](docs/research/rust-kutuphaneleri.md): format başına kütüphane karşılaştırması ve öneriler
- [Tauri 2 rehberi](docs/research/tauri2-rehberi.md): kurulum, sürükle-bırak, ilerleme, paketleme
- [MarkItDown davranış spesifikasyonu](docs/research/markitdown-davranis-spec.md): format başına çıktı kuralları ve golden test listesi

Golden test dosyaları [tests/fixtures/markitdown/](tests/fixtures/markitdown/) altında (MarkItDown 0.1.8, MIT).

## Teşekkür

Dönüştürme davranışı, Microsoft'un [MarkItDown](https://github.com/microsoft/markitdown) projesi (MIT lisansı) incelenerek tasarlanıyor. OmniMark, MarkItDown kodunu içermez; Rust ile sıfırdan yazılıyor. Test dosyaları ve beklenen çıktılar `tests/fixtures/markitdown/` altında, kendi MIT lisans notuyla (Microsoft Corporation) durur.

## Lisans

[MIT](LICENSE) © 2026 MoNoR (KeremV1).
