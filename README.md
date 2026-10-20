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

> **Durum:** ✅ M1 tamamlandı. 🔄 M2 (görsel metadata + OCR) sürüyor: **7/9 görev** `main`'de; kalanlar M2-3B (OCR kalite ölçümü), M2-6 (uygulamada OCR ayarları) ve M2-7 (ölçümler, CI, belgeler).

## Kaldığımız yer

**Geldiğimiz nokta (2026-10-20, akşam):** M1 bitti. M2'nin çekirdek kısmı `main`'de: görsel dosyalar, üç OCR motoru, PDF ve Office belgelerinde OCR.
- **M2'de bugün yapılanlar (7/9 görev):**
  - **M2-0** OCR çekirdeği: ortak motor arayüzü, görsel çözme sınırları, önbellek, `*[Image OCR]` blok biçimi.
  - **M2-1** Görsel dönüştürücü: EXIF ve XMP metadata (MarkItDown alan sırası).
  - **M2-3A** Windows OCR: Windows'un kendi motoru. İndirme gerektirmiyor ve Türkçe metni hatasız okudu. Çok büyük görselleri küçültüyor.
  - **M2-8** Tesseract: sistemdeki programı kabuksuz ve sabit argümanlarla, zaman aşımıyla çalıştırıyor. Türkçe metni hatasız okudu.
  - **M2-2** ocrs: tüm platformlarda çalışan motor. ~12 MB model isteğe bağlı iniyor; indirme SHA-256 ile doğrulanıyor, zaman aşımlı ve iptal edilebilir. Komut satırına `--ocr`, `models list` ve `models fetch ocrs` eklendi.
  - **M2-4** PDF'te OCR: sayfadaki görseller ve taranmış sayfalar okunuyor. OCR pdfium kilidi dışında çalışıyor ve bellek sayfa başına sınırlı.
  - **M2-5** Word, PowerPoint ve Excel'deki görsellerde OCR: aynı görsel iki kez okunmuyor.
- Her görevi Sonnet ajanları yazdı, Opus inceledi. Opus'un bulduğu önemli sorunlar birleştirmeden önce düzeltildi: indirmenin asılı kalması, sayfa başına sınırsız bellek ve Tesseract kurulum betiğinin kalıntı bırakması.
- **OCR varsayılan kapalı.** Şimdilik yalnız komut satırından açılıyor: `omnimark convert foto.jpg --ocr`. Uygulamadaki ayarı M2-6'da gelecek.
- **Dönüştürücüler:** PDF (pdfium; tablo ve form tespiti), Word (başlık, liste, tablo, görsel, dipnot, formül → LaTeX), Excel (XLSX/XLSM/XLS/ODS), PowerPoint (slayt, tablo, grafik, not), HTML, CSV (`;`/sekme tespiti), TXT/MD/JSON ve **görseller (EXIF/XMP + isteğe bağlı OCR)**.
- **Masaüstü uygulaması (Tauri 2):** dosya kuyruğu, sürükle-bırak, ilerleme, filtreler, Markdown önizleme (Görünüm/Kaynak), ayarlar, kısayollar, açık/koyu tema, Türkçe/İngilizce.
- **Güvenlik:** kötü niyetli dosyalara karşı bellek ve boyut sınırları var. Harici bağlantılar asla açılmıyor. Bir dosyanın çökmesi diğerlerini etkilemiyor. Arayüz yalnız çıktı klasöründeki `.md` dosyalarını okuyabiliyor.
- **Önizleme:** belgenin ilk ~64 KB'ı çiziliyor ve belgenin tamamının başlık listesi gösteriliyor. Önizleme belleği +24 MB.
- **Testler:** 227 Rust testi (OCR özellikleri açıkken 190 çekirdek testi) ve 27 arayüz testi geçiyor.
- **Ölçümler (Windows, kurulu sürüm):** kurulum dosyası **5,4 MB** · açılış **59 ms** · boşta bellek **71 MB** (hedef < 100 MB ✅). Ayrıntı: [docs/m1-olcumler.md](docs/m1-olcumler.md).

**Sıradaki iş (sonraki oturum):**
1. **M2-3B:** Windows, Tesseract ve ocrs motorlarının Türkçe ve İngilizce kalite karşılaştırması (`docs/m2-ocr-kalite.md`); varsayılan motor seçimini doğrulamak için.
2. **M2-6:** masaüstü uygulamasında OCR ayarları: motor ve dil seçimi, Tesseract yolu, ocrs modelini ilerleme çubuğuyla indirme. Ardından **M2-7:** bellek ölçümleri, CI'da OCR testleri, belgeler. Sonra M2 biter.
3. **Kalan elle testler** (aşağıda), ardından **M3:** ses transkripsiyonu.

**Elle testler** (2026-10-19, kullanıcı):
- [x] Explorer'dan dosya ve klasör sürükle-bırak (klasörde desteklenmeyen dosyalar bilerek atlanır).
- [x] **Dosya ekle** ve **Değiştir** (klasör seçme) pencereleri.
- [x] Ayarlarda dil değişimi, paralel işçi sayısı, görselleri çıkar seçeneği.
- [x] Büyük bir PDF/Word ile dönüştürme ve önizleme: başlık listesi, kesme şeridi, **Varsayılan uygulamada aç** (256 KB sürümüyle).
- [x] Önizlemede **Kopyala**: WebView2 pano izni sormuyor.
- [ ] **İptal**: dönüşüm çok hızlı bittiği için denenemedi → 50+ dosya ya da birkaç büyük PDF ile dönüştürmeyi başlatıp hemen İptal'e bas.
- [ ] **64 KB önizleme**: büyük bir belgede "ilk 64 KB gösteriliyor" şeridi, başlık listesi; Görev Yöneticisi'nde bellek.

Kontrol edilecek diğer konular: senin adına verilen kararlar (M2 kararları dahil) [HANDOFF.md](HANDOFF.md) dosyasında, "Onay bekleyen kararlar" bölümünde.

Ayrıntı: [HANDOFF.md](HANDOFF.md) · [M2 planı](docs/superpowers/plans/2026-10-19-omnimark-m2/00-genel.md) · [M2 tasarımı](docs/superpowers/specs/2026-10-19-omnimark-m2-design.md) · [M1 planı](docs/superpowers/plans/2026-10-15-omnimark-m1/00-genel.md) · [tasarım](docs/superpowers/specs/2026-10-15-omnimark-m1-design.md) · [ölçümler](docs/m1-olcumler.md)

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
| **M2** | Görsel metadata (EXIF) ve OCR (isteğe bağlı modül) | 5 | 🔄 Sürüyor | 7/9 görev (kalan: M2-3B, M2-6, M2-7) |
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
