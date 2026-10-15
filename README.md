# OmniMark

**OmniMark**, farklı dosya formatlarını Markdown'a (`.md`) dönüştüren, hafif, hızlı ve çok platformlu bir masaüstü uygulamasıdır.

> **Durum:** M1 kodlaması sürüyor — çekirdek kütüphanenin ilk 3 görevi tamamlandı. Henüz çalışan bir uygulama yok.

## Kaldığımız yer

- **Son durum (2026-10-15):** Görev 1-3 / 19 tamamlandı. Rust workspace, hata tipleri, tür tespiti ve çıktı adlandırma hazır; 23 test geçiyor.
- **Kod:** [`m1` dalı](https://github.com/KeremV1/OmniMark/tree/m1). `main`'e M1 bitince birleştirilecek.
- **Sıradaki:** Görev 4-5 (kayıt defteri, CLI, golden testler), ardından dönüştürücüler.
- **Ayrıntı:** [HANDOFF.md](HANDOFF.md), [plan](docs/superpowers/plans/2026-10-15-omnimark-m1/00-genel.md), [tasarım](docs/superpowers/specs/2026-10-15-omnimark-m1-design.md).

- **Platformlar:** Windows, macOS, Linux
- **Teknoloji:** Rust + [Tauri 2](https://v2.tauri.app/), arayüz HTML/CSS/TypeScript
- **Hedefler:** Düşük RAM kullanımı, hızlı açılış, küçük kurulum boyutu
- **Referans:** Microsoft'un açık kaynaklı Python kütüphanesi [MarkItDown](https://github.com/microsoft/markitdown) (MIT)

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

1. Proje iskeleti ve temel arayüz
2. Sürükle-bırak ve dosya kuyruğu
3. İlk çalışan Markdown dönüştürme akışı
4. PDF, DOCX, XLSX, PPTX ve HTML desteği
5. Görsel metadata ve OCR
6. Ses transkripsiyonu
7. Hata yönetimi, testler ve performans iyileştirmeleri
8. Windows, macOS ve Linux paketleri

**Temel ilke:** Önce küçük, çalışan ve güvenilir bir uygulama; ardından formatlar ve gelişmiş özellikler aşamalı olarak eklenir. Tüm platformlarda aynı temel deneyim korunur.

## Geliştirme

Henüz kod yok. Gereksinimler:

- Rust 1.90+ (Windows'ta MSVC toolchain)
- Windows için Visual Studio 2022 Build Tools (C++ iş yükü) ve WebView2
- Node.js 20+
- PDF için pdfium: `scripts/fetch-pdfium` betiğiyle sabit bir sürümden indirilecek ([bblanchon/pdfium-binaries](https://github.com/bblanchon/pdfium-binaries))

Araştırma notları:

- [Rust kütüphaneleri](docs/research/rust-kutuphaneleri.md): format başına kütüphane karşılaştırması ve öneriler
- [Tauri 2 rehberi](docs/research/tauri2-rehberi.md): kurulum, sürükle-bırak, ilerleme, paketleme
- [MarkItDown davranış spesifikasyonu](docs/research/markitdown-davranis-spec.md): format başına çıktı kuralları ve golden test listesi

Golden test dosyaları [tests/fixtures/markitdown/](tests/fixtures/markitdown/) altında (MarkItDown 0.1.8, MIT).

## Teşekkür

Dönüştürme davranışı, Microsoft'un [MarkItDown](https://github.com/microsoft/markitdown) projesi (MIT lisansı) incelenerek tasarlanıyor. OmniMark, MarkItDown kodunu içermez; Rust ile sıfırdan yazılıyor.
