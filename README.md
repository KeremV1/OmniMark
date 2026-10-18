# OmniMark

**OmniMark**, farklı dosya formatlarını Markdown'a (`.md`) dönüştüren, hafif, hızlı ve çok platformlu bir masaüstü uygulamasıdır.

> **Durum:** M1 kodu tamamlandı ve son incelemeden geçti (19/19 görev, `m1` dalında); `main`'e birleştirme onayı bekleniyor.

## Kaldığımız yer

**Geldiğimiz nokta (2026-10-18):**
- **Çekirdek altyapı (Görev 1-5):** tür tespiti, karakter kodlaması, görsel çıkarma, çakışmasız çıktı adlandırma, dönüştürücü kayıt defteri, çökme yalıtımı, güvenli zip/XML okuma, `omnimark` komut satırı aracı, golden testler, pdfium indirme betikleri.
- **Dönüştürücüler (Görev 6-10, 12-13):** TXT/MD/JSON, CSV (`;`/sekme tespiti), HTML, Excel (XLSX/XLS/ODS), Word (başlık, liste, tablo, görsel, dipnot), PowerPoint (slayt, tablo, grafik, not), PDF (pdfium; tablo ve form tespiti).
- **Word formülleri (Görev 11):** Office Math → LaTeX (`$…$`, `$$…$$`).
- **Masaüstü (Görev 14, 15, 17):** Tauri 2 kabuğu, paralel dönüştürme kuyruğu, ilerleme olayları, ayarlar; arayüz çekirdeği (kuyruk, Türkçe/İngilizce metinler); logodan üretilen uygulama simgeleri. Arayüz yalnızca çıktı klasöründeki `.md` dosyalarını okuyabilir.
- **Arayüz (Görev 16):** dosya kuyruğu, sürükle-bırak, ilerleme çubukları, filtreler, Markdown önizleme (görünüm/kaynak), ayarlar, klavye kısayolları; açık/koyu tema; Türkçe/İngilizce.
- **CI (Görev 18):** her push'ta Windows, macOS ve Linux'ta otomatik test ([Actions](https://github.com/KeremV1/OmniMark/actions)).
- **Ölçümler (Görev 19, Windows):** kurulum dosyası **5,4 MB** (hedef < 15 MB ✅) · pencere açılışı **59 ms** (hedef < 1 s ✅) · boşta RAM **170 MB** private / 347 MB WorkingSet (hedef < 100 MB ❌; uygulama süreci 5,5 MB, geri kalanı WebView2) · 144 Rust + 17 arayüz testi geçiyor. Ayrıntı: `docs/m1-olcumler.md` (`m1` dalında).
- **Güvenlik:** kötü niyetli dosyalara karşı sınırlar eklendi (çok derin HTML, dev tablo/grafik/sayfa verisi, zip öğe boyutu, XML düğüm sayısı; harici bağlantılar asla açılmaz). Son incelemede bulunan bellek tüketme açıkları kapatıldı. 121 test geçiyor.
- **Dal:** kod [`m1` dalında](https://github.com/KeremV1/OmniMark/tree/m1); M1 bitince `main`'e birleşecek.

**Sıradaki iş:**
1. `m1` → `main` birleştirmesi (son inceleme ve düzeltmeleri bitti); M1 kapanışı.
2. Gerçek kullanımla deneme: sürükle-bırak, önizleme, kopyalama.
3. RAM hedefinin değerlendirilmesi (WebView2 kaynaklı) ve M2 (görsel metadata + OCR) planı.

Ayrıntı: [HANDOFF.md](HANDOFF.md) · [plan](docs/superpowers/plans/2026-10-15-omnimark-m1/00-genel.md) · [tasarım](docs/superpowers/specs/2026-10-15-omnimark-m1-design.md)

## Desteklenecek formatlar

| Format | Uzantılar | Çıktı |
| --- | --- | --- |
| PDF | `.pdf` | Metin, mümkünse tablolar |
| Word | `.docx` | Başlıklar, listeler, tablolar, bağlantılar, formüller (LaTeX) |
| Excel | `.xlsx`, `.xls` | Her sayfa için ayrı başlık ve Markdown tablosu |
| PowerPoint | `.pptx` | Slayt slayt başlıklar, metinler, tablolar, grafik verileri, konuşmacı notları |
| HTML | `.html`, `.htm` | Gövde içeriği; script ve stil blokları ayıklanır |
| Görsel | `.png`, `.jpg`, `.jpeg`, `.webp` … | EXIF metadata, isteğe bağlı OCR |
| Ses | `.wav`, `.mp3`, `.m4a` … | Metadata, isteğe bağlı konuşmadan metne çeviri |
| Metin | `.txt`, `.md`, `.json`, `.csv` | Düz metin; CSV için Markdown tablosu |

Her format için uygun Rust kütüphaneleri seçilecek. MarkItDown'ın Python bağımlılıklarının Rust'ta birebir karşılığı olduğu varsayılmıyor; her biri ayrıca araştırılıyor ([docs/research/](docs/research/)).

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
| **M1** | Uygulama iskeleti, sürükle-bırak kuyruğu, dönüştürme akışı, önizleme; PDF, Word, Excel, PowerPoint, HTML, CSV, TXT/JSON | 1 · 2 · 3 · 4 | 🔄 Son inceleme | 19/19 görev |
| **M2** | Görsel metadata (EXIF) ve OCR (isteğe bağlı modül) | 5 | ⏳ Planlandı | — |
| **M3** | Ses transkripsiyonu (yerel Whisper, isteğe bağlı modül) | 6 | ⏳ Planlandı | — |
| **M4** | Sağlamlaştırma: hata yönetimi, ek testler, performans iyileştirmeleri | 7 | ⏳ Planlandı | — |
| **M5** | Windows, macOS ve Linux kurulum paketleri, imzalama | 8 | ⏳ Planlandı | — |

Yol haritası adımları: **1** proje iskeleti ve temel arayüz · **2** sürükle-bırak ve dosya kuyruğu · **3** ilk çalışan Markdown dönüştürme akışı · **4** PDF, DOCX, XLSX, PPTX ve HTML desteği · **5** görsel metadata ve OCR · **6** ses transkripsiyonu · **7** hata yönetimi, testler ve performans · **8** Windows, macOS ve Linux paketleri.

Durum işaretleri: ✅ tamamlandı · 🔄 sürüyor · ⏳ planlandı.

**Temel ilke:** Önce küçük, çalışan ve güvenilir bir uygulama; ardından formatlar ve gelişmiş özellikler aşamalı olarak eklenir. Tüm platformlarda aynı temel deneyim korunur.

## Platform desteği

| Platform | Geliştirme ve derleme | Otomatik testler (CI) | Kurulum paketi | Durum |
| --- | --- | --- | --- | --- |
| **Windows** 10/11 (x64) | ✅ Geliştirme ortamı kurulu, tüm testler geçiyor | ✅ GitHub Actions | ⏳ M5 · NSIS kurulum (`.exe`) | 🔄 Geliştiriliyor |
| **macOS** (Apple Silicon ve Intel) | ✅ CI üzerinde derleniyor | ✅ GitHub Actions | ⏳ M5 · `.dmg` (imzalama için Apple Developer hesabı gerekir) | 🔄 CI’da test ediliyor |
| **Linux** (x64; Ubuntu 22.04+ ve benzerleri) | ✅ CI üzerinde derleniyor | ✅ GitHub Actions | ⏳ M5 · AppImage ve `.deb` | 🔄 CI’da test ediliyor |

## Geliştirme

Gereksinimler:

- Rust 1.90+ (Windows'ta MSVC toolchain)
- Windows için Visual Studio 2022 Build Tools (C++ iş yükü) ve WebView2
- Node.js 20+
- PDF için pdfium: **derlemeden önce** `scripts/fetch-pdfium.ps1` (Windows) ya da `scripts/fetch-pdfium.sh` (macOS/Linux) çalıştırılmalı. Sabit sürüm `chromium/8066`, SHA-256 doğrulamalı ([bblanchon/pdfium-binaries](https://github.com/bblanchon/pdfium-binaries)). `vendor/pdfium` olmadan masaüstü uygulaması derlenmez.

Komutlar: `cargo test --workspace` (testler), `cargo run -p omnimark-cli -- <dosya>` (komut satırı), `npm ci && npm run tauri dev` (masaüstü uygulaması).

Araştırma notları:

- [Rust kütüphaneleri](docs/research/rust-kutuphaneleri.md): format başına kütüphane karşılaştırması ve öneriler
- [Tauri 2 rehberi](docs/research/tauri2-rehberi.md): kurulum, sürükle-bırak, ilerleme, paketleme
- [MarkItDown davranış spesifikasyonu](docs/research/markitdown-davranis-spec.md): format başına çıktı kuralları ve golden test listesi

Golden test dosyaları [tests/fixtures/markitdown/](tests/fixtures/markitdown/) altında (MarkItDown 0.1.8, MIT).

## Teşekkür

Dönüştürme davranışı, Microsoft'un [MarkItDown](https://github.com/microsoft/markitdown) projesi (MIT lisansı) incelenerek tasarlanıyor. OmniMark, MarkItDown kodunu içermez; Rust ile sıfırdan yazılıyor.
