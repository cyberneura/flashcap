//! 共有に回す画像だけ Retina の 2 倍を等倍に縮める判定と縮小 (CYBERNEURA-DEV-1211)。
//!
//! Tauri に依存しない純粋な部分。縮小するかの判定 (`should_scale_down`) と 1/2 への縮小
//! (`maybe_scale_down`) だけを持ち、設定の読み込み・モニターの取得・ファイルの書き出しは
//! `share_image.rs`。このホスト (Linux) では Tauri 全体を cargo check できないので、
//! ここだけを抜き出して単体テストできる形にしてある。

use image::DynamicImage;

/// 縮小の判定に要る、撮影したモニターの論理サイズと倍率
#[derive(Clone, Copy, Debug, PartialEq)]
pub(crate) struct ScreenInfo {
    pub logical_w: u32,
    pub logical_h: u32,
    pub scale_factor: f64,
}

/// 画像を 1/2 に縮めるか。「Retina (倍率 2 以上) のモニターで撮った、論理サイズより大きい
/// 画像」だけが対象。等倍のモニターで撮った画像や、論理サイズに収まる (= 既に等倍相当の)
/// 画像は縮めない。貼り付け / ファイルから開いた画像は撮影倍率が分からないので、呼び出し側が
/// ここへ来る前に除く (`captured`)。
pub(crate) fn should_scale_down(
    width: u32,
    height: u32,
    logical_w: u32,
    logical_h: u32,
    scale_factor: f64,
) -> bool {
    scale_factor >= 2.0 && (width > logical_w || height > logical_h)
}

/// 1/2 に縮める (端数は切り捨て、最小 1px)。共有用なので速さ優先の Triangle (bilinear)
pub(crate) fn scale_down_half(img: &DynamicImage) -> DynamicImage {
    let w = (img.width() / 2).max(1);
    let h = (img.height() / 2).max(1);
    img.resize_exact(w, h, image::imageops::FilterType::Triangle)
}

/// 条件を満たす時だけ 1/2 に縮めた画像を返す (満たさなければそのまま返す)
pub(crate) fn maybe_scale_down(img: DynamicImage, screen: Option<ScreenInfo>) -> DynamicImage {
    match screen {
        Some(s)
            if should_scale_down(
                img.width(),
                img.height(),
                s.logical_w,
                s.logical_h,
                s.scale_factor,
            ) =>
        {
            scale_down_half(&img)
        }
        _ => img,
    }
}

/// PNG の pHYs チャンクから、標準解像度 (72 dpi) の何倍の密度で撮られたかを読む
/// (CYBERNEURA-DEV-1237)。macOS の screencapture は Retina なら 144 dpi、等倍なら 72 dpi を書く。
/// PNG でない・pHYs が無い・単位がメートルでない・値が 0 なら None。pHYs は IDAT より前に
/// 置かれる決まりなので、IDAT に達したらそこで打ち切る
pub(crate) fn png_density_scale(png: &[u8]) -> Option<f64> {
    /// 72 dpi を 1 メートルあたりのピクセル数にした値
    const PIXELS_PER_METER_AT_72_DPI: f64 = 72.0 / 0.0254;
    const SIGNATURE: &[u8] = b"\x89PNG\r\n\x1a\n";

    let mut rest = png.strip_prefix(SIGNATURE)?;
    while rest.len() >= 8 {
        let len = u32::from_be_bytes(rest[..4].try_into().ok()?) as usize;
        let kind = &rest[4..8];
        if kind == b"IDAT" {
            return None;
        }
        let body = rest.get(8..8usize.checked_add(len)?)?;
        if kind == b"pHYs" {
            if len != 9 || body[8] != 1 {
                return None;
            }
            let per_meter = u32::from_be_bytes(body[..4].try_into().ok()?);
            return (per_meter > 0).then(|| per_meter as f64 / PIXELS_PER_METER_AT_72_DPI);
        }
        // 長さ + 種類 + 本体 + CRC
        rest = rest.get(8usize.checked_add(len)?.checked_add(4)?..)?;
    }
    None
}

/// 注釈の線の太さ・文字の大きさに掛ける倍率。Retina (2 倍前後) の撮影なら 2、それ以外は 1。
/// 記録された密度があればそれだけで決め、無ければモニターの倍率で決める。144 dpi は
/// pHYs の整数丸めで 2.0 をわずかに割るので閾値は 1.9 にしている。
/// 1.5x (Windows の 150%) は `should_scale_down` と同じく対象外
pub(crate) fn annotation_scale(png_density: Option<f64>, monitor_scale: Option<f64>) -> u32 {
    match png_density.or(monitor_scale) {
        Some(scale) if scale >= 1.9 => 2,
        _ => 1,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// 指定のチャンクを並べた PNG のバイト列 (CRC は読まないので 0 埋め)
    fn png_with_chunks(chunks: &[(&[u8; 4], Vec<u8>)]) -> Vec<u8> {
        let mut png = b"\x89PNG\r\n\x1a\n".to_vec();
        for (kind, body) in chunks {
            png.extend_from_slice(&(body.len() as u32).to_be_bytes());
            png.extend_from_slice(*kind);
            png.extend_from_slice(body);
            png.extend_from_slice(&[0; 4]);
        }
        png
    }

    fn phys(per_meter: u32, unit: u8) -> Vec<u8> {
        let mut body = per_meter.to_be_bytes().to_vec();
        body.extend_from_slice(&per_meter.to_be_bytes());
        body.push(unit);
        body
    }

    #[test]
    fn density_is_read_from_phys_before_idat() {
        let ihdr: (&[u8; 4], Vec<u8>) = (b"IHDR", vec![0; 13]);
        // 144 dpi / 72 dpi を整数に丸めた値 (macOS の screencapture が書く値)
        let retina = png_with_chunks(&[
            ihdr.clone(),
            (b"pHYs", phys(5669, 1)),
            (b"IDAT", vec![0; 4]),
        ]);
        assert_eq!(annotation_scale(png_density_scale(&retina), None), 2);
        let standard = png_with_chunks(&[
            ihdr.clone(),
            (b"pHYs", phys(2835, 1)),
            (b"IDAT", vec![0; 4]),
        ]);
        assert_eq!(annotation_scale(png_density_scale(&standard), None), 1);
        // IDAT より後の pHYs、単位なし、0、壊れた長さは読まない
        let late = png_with_chunks(&[
            ihdr.clone(),
            (b"IDAT", vec![0; 4]),
            (b"pHYs", phys(5669, 1)),
        ]);
        assert_eq!(png_density_scale(&late), None);
        let no_unit = png_with_chunks(&[ihdr.clone(), (b"pHYs", phys(5669, 0))]);
        assert_eq!(png_density_scale(&no_unit), None);
        let zero = png_with_chunks(&[ihdr.clone(), (b"pHYs", phys(0, 1))]);
        assert_eq!(png_density_scale(&zero), None);
        let mut truncated = png_with_chunks(&[ihdr, (b"pHYs", phys(5669, 1))]);
        truncated.truncate(truncated.len() - 8);
        assert_eq!(png_density_scale(&truncated), None);
        assert_eq!(png_density_scale(b"not a png"), None);
    }

    #[test]
    fn annotation_scale_prefers_the_recorded_density() {
        // 等倍の外部モニターで撮った画像は、ウインドウが Retina にあっても 1
        assert_eq!(annotation_scale(Some(1.0), Some(2.0)), 1);
        assert_eq!(annotation_scale(Some(2.0), Some(1.0)), 2);
        // 記録が無ければモニターで決める (Windows)
        assert_eq!(annotation_scale(None, Some(2.0)), 2);
        assert_eq!(annotation_scale(None, Some(1.5)), 1);
        assert_eq!(annotation_scale(None, Some(1.0)), 1);
        assert_eq!(annotation_scale(None, None), 1);
    }

    #[test]
    fn retina_full_screen_capture_is_scaled_down() {
        // 2x のモニター (論理 1440x900 = 物理 2880x1800) の全画面
        assert!(should_scale_down(2880, 1800, 1440, 900, 2.0));
        // 一部分でも論理サイズを超えていれば対象
        assert!(should_scale_down(1500, 100, 1440, 900, 2.0));
        assert!(should_scale_down(100, 901, 1440, 900, 2.0));
    }

    #[test]
    fn small_or_non_retina_captures_are_left_alone() {
        // 論理サイズに収まる = 等倍相当
        assert!(!should_scale_down(1440, 900, 1440, 900, 2.0));
        assert!(!should_scale_down(800, 600, 1440, 900, 2.0));
        // 等倍のモニターでは大きくても縮めない
        assert!(!should_scale_down(2880, 1800, 2880, 1800, 1.0));
        assert!(!should_scale_down(3000, 100, 2880, 1800, 1.0));
        // 1.5x (Windows の 150%) も対象外
        assert!(!should_scale_down(2880, 1800, 1920, 1200, 1.5));
    }

    #[test]
    fn maybe_scale_down_halves_only_when_needed() {
        let img = DynamicImage::new_rgba8(2880, 1800);
        let screen = Some(ScreenInfo {
            logical_w: 1440,
            logical_h: 900,
            scale_factor: 2.0,
        });
        let small = maybe_scale_down(img.clone(), screen);
        assert_eq!((small.width(), small.height()), (1440, 900));
        // モニターが分からなければ縮めない
        let same = maybe_scale_down(img.clone(), None);
        assert_eq!((same.width(), same.height()), (2880, 1800));
        // 奇数は切り捨て、1px を割らない
        let odd = maybe_scale_down(DynamicImage::new_rgba8(2001, 1), screen);
        assert_eq!((odd.width(), odd.height()), (1000, 1));
    }
}
