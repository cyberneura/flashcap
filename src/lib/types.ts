export interface Arrow {
  id: string;
  startX: number;
  startY: number;
  endX: number;
  endY: number;
  color: string;
  thickness: number;
  whiteStroke: boolean;
  dropShadow: boolean;
}

export interface ArrowSettings {
  color: string;
  thickness: number;
  whiteStroke: boolean;
  dropShadow: boolean;
}

export type MaskMode = "mosaic" | "blur" | "fill";

export interface MaskRect {
  id: string;
  x: number;
  y: number;
  width: number;
  height: number;
  mode: MaskMode;
  color: string;
}

export interface MaskSettings {
  mode: MaskMode;
  color: string;
  blurRadius: number;
  mosaicBlockSize: number;
}

export type ShapeType = "rect" | "ellipse";

export interface Shape {
  id: string;
  type: ShapeType;
  x: number;
  y: number;
  width: number;
  height: number;
  color: string;
  thickness: number;
  whiteStroke: boolean;
  dropShadow: boolean;
}

export interface ShapeSettings {
  type: ShapeType;
  color: string;
  thickness: number;
  whiteStroke: boolean;
  dropShadow: boolean;
}

export interface TextAnnotation {
  id: string;
  x: number;
  y: number;
  text: string;
  fontSize: number;
  color: string;
  bold: boolean;
  italic: boolean;
  whiteStroke: boolean;
  dropShadow: boolean;
}

export interface TextSettings {
  fontSize: number;
  color: string;
  bold: boolean;
  italic: boolean;
  whiteStroke: boolean;
  dropShadow: boolean;
}

/** トリミング範囲。座標・寸法は画像の自然解像度ピクセル */
export interface CropRect {
  x: number;
  y: number;
  width: number;
  height: number;
}

// --- Video capture / export ---

export type VideoExportFormat = "mp4" | "gif";

// 等倍 / 1/2 / HD(1920x1080) / 720p(1280x720) / 512x512
export type VideoSizePreset = "original" | "half" | "hd" | "720p" | "square512";

// fill: はみ出しを切り落とす (crop) / fit: 収まるようにリサイズ (pad)
export type VideoResizeMode = "fill" | "fit";

export interface VideoExportSettings {
  format: VideoExportFormat;
  sizePreset: VideoSizePreset;
  resizeMode: VideoResizeMode;
  pad: boolean;
  padColor: string;
  fps: number;
}
