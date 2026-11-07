import Foundation
import Vision

// stdin から base64 エンコードされた PNG データを読み取り、
// Vision framework で OCR を実行して stdout にテキストを出力する。
// --region x,y,w,h と --size imgW,imgH オプションで認識領域を指定可能。
// --json を付けると、改行区切りの文字列の代わりに、認識した行ごとの
// { "text": ..., "box": [x, y, w, h] } (画像ピクセル、左上原点) の配列を 1 行の JSON で出す
// (CYBERNEURA-DEV-1210 の自動マスク用)。--json 無しの出力は従来のまま。
// --json は --region と併用しない (領域付きの boundingBox は領域基準になるため、
// Rust 側 (ocr.rs の ocr_image_boxes) は常に画像全体で呼ぶ)。

func parseRegion(_ arg: String, imgWidth: Int, imgHeight: Int) -> CGRect? {
    let parts = arg.split(separator: ",").compactMap { Double($0) }
    guard parts.count == 4 else { return nil }
    let x = parts[0]
    let y = parts[1]
    let w = parts[2]
    let h = parts[3]

    // Vision の座標系: 左下原点、0-1 正規化
    let normX = x / Double(imgWidth)
    let normY = 1.0 - (y + h) / Double(imgHeight)
    let normW = w / Double(imgWidth)
    let normH = h / Double(imgHeight)

    // regionOfInterest は [0, 1] 範囲内である必要があるためクランプ
    let minX = max(0.0, min(1.0, normX))
    let minY = max(0.0, min(1.0, normY))
    let maxX = max(0.0, min(1.0, normX + normW))
    let maxY = max(0.0, min(1.0, normY + normH))
    let clampedW = maxX - minX
    let clampedH = maxY - minY
    guard clampedW > 0.0, clampedH > 0.0 else { return nil }

    return CGRect(x: minX, y: minY, width: clampedW, height: clampedH)
}

// コマンドライン引数の解析
var regionArg: String? = nil
var imgWidth: Int = 0
var imgHeight: Int = 0
var jsonOutput = false

let args = CommandLine.arguments
var i = 1
while i < args.count {
    if args[i] == "--region" && i + 1 < args.count {
        regionArg = args[i + 1]
        i += 2
    } else if args[i] == "--size" && i + 1 < args.count {
        let sizeParts = args[i + 1].split(separator: ",").compactMap { Int($0) }
        if sizeParts.count == 2 {
            imgWidth = sizeParts[0]
            imgHeight = sizeParts[1]
        }
        i += 2
    } else if args[i] == "--json" {
        jsonOutput = true
        i += 1
    } else {
        i += 1
    }
}

// stdin から base64 データを読み取り
guard let inputData = readLine(strippingNewline: false) else {
    fputs("Error: No input data\n", stderr)
    exit(1)
}

let base64String = inputData.trimmingCharacters(in: .whitespacesAndNewlines)
guard let imageData = Data(base64Encoded: base64String) else {
    fputs("Error: Invalid base64 data\n", stderr)
    exit(1)
}

guard let cgImageSource = CGImageSourceCreateWithData(imageData as CFData, nil),
      let cgImage = CGImageSourceCreateImageAtIndex(cgImageSource, 0, nil) else {
    fputs("Error: Failed to create image\n", stderr)
    exit(1)
}

if imgWidth == 0 { imgWidth = cgImage.width }
if imgHeight == 0 { imgHeight = cgImage.height }

let request = VNRecognizeTextRequest()
request.recognitionLevel = .accurate
request.recognitionLanguages = ["ja", "en"]
request.usesLanguageCorrection = true

// 認識領域の指定
if let regionStr = regionArg,
   let regionRect = parseRegion(regionStr, imgWidth: imgWidth, imgHeight: imgHeight) {
    request.regionOfInterest = regionRect
}

let handler = VNImageRequestHandler(cgImage: cgImage, options: [:])

do {
    try handler.perform([request])
} catch {
    fputs("Error: \(error.localizedDescription)\n", stderr)
    exit(1)
}

guard let observations = request.results else {
    if jsonOutput { print("[]") }
    exit(0)
}

if jsonOutput {
    // Vision の boundingBox は左下原点の 0-1 正規化。画像ピクセルの左上原点に直す
    let w = Double(cgImage.width)
    let h = Double(cgImage.height)
    var lines: [[String: Any]] = []
    for observation in observations {
        guard let candidate = observation.topCandidates(1).first else { continue }
        let bb = observation.boundingBox
        let x = bb.minX * w
        let y = (1.0 - bb.maxY) * h
        lines.append([
            "text": candidate.string,
            "box": [x, y, bb.width * w, bb.height * h],
        ])
    }
    do {
        let data = try JSONSerialization.data(withJSONObject: lines, options: [])
        FileHandle.standardOutput.write(data)
        FileHandle.standardOutput.write("\n".data(using: .utf8)!)
    } catch {
        fputs("Error: \(error.localizedDescription)\n", stderr)
        exit(1)
    }
    exit(0)
}

let text = observations.compactMap { $0.topCandidates(1).first?.string }.joined(separator: "\n")
print(text)
