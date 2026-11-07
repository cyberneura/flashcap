//! `ocr.swift --json` の出力 (認識した行と画像ピクセルの箱) の形 (CYBERNEURA-DEV-1210)。
//! Tauri に依存しない純粋な部分で、形を固定するテストがここにある (Swift はこのホストでは
//! 実行できないので、出力形式は両側のテストで合わせる)。

use serde::{Deserialize, Serialize};

/// OCR で認識した 1 行。`box` は画像の自然解像度ピクセル、左上原点の [x, y, w, h]
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct OcrLine {
    pub text: String,
    #[serde(rename = "box")]
    pub bbox: [f64; 4],
}

/// `ocr.swift --json` の stdout を読む。空 (結果なし) は空の配列
pub(crate) fn parse_ocr_lines(stdout: &str) -> Result<Vec<OcrLine>, String> {
    let trimmed = stdout.trim();
    if trimmed.is_empty() {
        return Ok(Vec::new());
    }
    serde_json::from_str(trimmed).map_err(|e| format!("Failed to parse the OCR output: {}", e))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_the_swift_json_output() {
        // ocr.swift が --json で出す形そのもの (1 行の JSON 配列)
        let out = r#"[{"text":"user@example.com","box":[10,20.5,120,14]},{"text":"hello","box":[0,0,1,1]}]"#;
        let lines = parse_ocr_lines(out).unwrap();
        assert_eq!(lines.len(), 2);
        assert_eq!(lines[0].text, "user@example.com");
        assert_eq!(lines[0].bbox, [10.0, 20.5, 120.0, 14.0]);
        assert_eq!(lines[1].text, "hello");
    }

    #[test]
    fn empty_output_means_no_lines() {
        assert_eq!(parse_ocr_lines("").unwrap(), Vec::<OcrLine>::new());
        assert_eq!(parse_ocr_lines("[]\n").unwrap(), Vec::<OcrLine>::new());
    }

    #[test]
    fn rejects_the_plain_text_output() {
        // --json を付け忘れた (改行区切りの文字列) 時はエラーにする (黙って 0 件にしない)
        assert!(parse_ocr_lines("hello\nworld").is_err());
    }

    #[test]
    fn round_trips_through_serde() {
        let line = OcrLine {
            text: "sk-abc".into(),
            bbox: [1.0, 2.0, 3.0, 4.0],
        };
        let json = serde_json::to_string(&line).unwrap();
        assert_eq!(json, r#"{"text":"sk-abc","box":[1.0,2.0,3.0,4.0]}"#);
        assert_eq!(serde_json::from_str::<OcrLine>(&json).unwrap(), line);
    }
}
