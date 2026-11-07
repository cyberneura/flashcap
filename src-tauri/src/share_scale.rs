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

#[cfg(test)]
mod tests {
    use super::*;

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
