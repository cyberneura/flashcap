// Freehand pen strokes (CYBERNEURA-DEV-1401)
//
// A stroke is the dragged path as a list of points in image natural-resolution px. Thinning,
// the SVG path, hit testing and moving are pure functions here so that the overlay
// (src/lib/StrokeOverlay.svelte), the export (renderComposite in +page.svelte) and
// tests/stroke.test.mts all use the same geometry.

import type { Stroke, StrokePoint } from "./types";

/**
 * Minimum distance (screen px) between two kept points while drawing. Movements smaller than this
 * are dropped so that a slow drag does not record hundreds of jittery points.
 */
export const STROKE_MIN_STEP = 2;

/**
 * Appends `pt` to the points being drawn unless it is closer than `minDistance` to the last kept
 * point. Returns the same array when the point is dropped (so callers can skip a re-render).
 */
export function appendStrokePoint(points: StrokePoint[], pt: StrokePoint, minDistance: number): StrokePoint[] {
  const last = points[points.length - 1];
  if (last && Math.hypot(pt.x - last.x, pt.y - last.y) < minDistance) return points;
  return [...points, { x: pt.x, y: pt.y }];
}

/**
 * Makes sure the release position is part of the stroke, even if it was within `minDistance` of
 * the last kept point (otherwise the line would end short of where the pointer was released).
 */
export function finishStrokePoints(points: StrokePoint[], pt: StrokePoint): StrokePoint[] {
  const last = points[points.length - 1];
  if (!last || (last.x === pt.x && last.y === pt.y)) return points;
  return [...points, { x: pt.x, y: pt.y }];
}

/** Total length of the polyline. Used to discard a click that did not really draw anything */
export function strokeLength(points: readonly StrokePoint[]): number {
  let len = 0;
  for (let i = 1; i < points.length; i++) {
    len += Math.hypot(points[i].x - points[i - 1].x, points[i].y - points[i - 1].y);
  }
  return len;
}

function fmt(n: number): string {
  // Two decimals are far below a pixel and keep the path string short
  return String(Math.round(n * 100) / 100);
}

/**
 * SVG path data for the stroke, smoothed with quadratic Béziers: each kept point is a control point
 * and the curve passes through the midpoints between consecutive points (the usual midpoint
 * smoothing). The first and last points are reached with straight segments so the line starts and
 * ends exactly where the pointer did. The same string is drawn on screen (`<path d>`) and in the
 * export (`new Path2D(d)`), so the two cannot drift apart.
 *
 * A single point becomes a zero-length segment, which round caps draw as a dot.
 */
export function strokePath(points: readonly StrokePoint[]): string {
  if (points.length === 0) return "";
  const p0 = points[0];
  if (points.length === 1) return `M${fmt(p0.x)} ${fmt(p0.y)}L${fmt(p0.x)} ${fmt(p0.y)}`;
  if (points.length === 2) {
    const p1 = points[1];
    return `M${fmt(p0.x)} ${fmt(p0.y)}L${fmt(p1.x)} ${fmt(p1.y)}`;
  }
  let d = `M${fmt(p0.x)} ${fmt(p0.y)}`;
  const m1 = { x: (p0.x + points[1].x) / 2, y: (p0.y + points[1].y) / 2 };
  d += `L${fmt(m1.x)} ${fmt(m1.y)}`;
  for (let i = 1; i < points.length - 1; i++) {
    const c = points[i];
    const n = points[i + 1];
    d += `Q${fmt(c.x)} ${fmt(c.y)} ${fmt((c.x + n.x) / 2)} ${fmt((c.y + n.y) / 2)}`;
  }
  const last = points[points.length - 1];
  d += `L${fmt(last.x)} ${fmt(last.y)}`;
  return d;
}

function distToSegment(px: number, py: number, a: StrokePoint, b: StrokePoint): number {
  const dx = b.x - a.x;
  const dy = b.y - a.y;
  const lenSq = dx * dx + dy * dy;
  if (lenSq === 0) return Math.hypot(px - a.x, py - a.y);
  const t = Math.max(0, Math.min(1, ((px - a.x) * dx + (py - a.y) * dy) / lenSq));
  return Math.hypot(px - (a.x + t * dx), py - (a.y + t * dy));
}

/** Segments per quadratic when flattening the drawn curve for hit testing */
const CURVE_SAMPLES = 8;

/**
 * The drawn curve (`strokePath`) flattened to a polyline: the same straight ends and the quadratics
 * sampled at `CURVE_SAMPLES` steps. Hit testing measures against this rather than the kept points —
 * at a sharp corner the smoothed curve cuts well inside the corner (half a segment), so the polyline
 * through the kept points can be farther than the tolerance from the line the user sees.
 */
export function strokeCurvePoints(points: readonly StrokePoint[]): StrokePoint[] {
  if (points.length <= 2) return points.map((p) => ({ x: p.x, y: p.y }));
  const out: StrokePoint[] = [{ x: points[0].x, y: points[0].y }];
  let from = { x: (points[0].x + points[1].x) / 2, y: (points[0].y + points[1].y) / 2 };
  out.push(from);
  for (let i = 1; i < points.length - 1; i++) {
    const c = points[i];
    const n = points[i + 1];
    const to = { x: (c.x + n.x) / 2, y: (c.y + n.y) / 2 };
    for (let s = 1; s <= CURVE_SAMPLES; s++) {
      const t = s / CURVE_SAMPLES;
      const u = 1 - t;
      out.push({
        x: u * u * from.x + 2 * u * t * c.x + t * t * to.x,
        y: u * u * from.y + 2 * u * t * c.y + t * t * to.y,
      });
    }
    from = to;
  }
  const last = points[points.length - 1];
  out.push({ x: last.x, y: last.y });
  return out;
}

/** Distance from (px, py) to the drawn (smoothed) stroke */
export function distToStroke(points: readonly StrokePoint[], px: number, py: number): number {
  if (points.length === 0) return Infinity;
  if (points.length === 1) return Math.hypot(px - points[0].x, py - points[0].y);
  const curve = strokeCurvePoints(points);
  let best = Infinity;
  for (let i = 1; i < curve.length; i++) {
    best = Math.min(best, distToSegment(px, py, curve[i - 1], curve[i]));
  }
  return best;
}

/**
 * Whether a press at (px, py) hits the stroke. The tolerance is the same as the arrow's: the drawn
 * thickness (x annotationScale) plus 6 screen px.
 */
export function hitsStroke(stroke: Stroke, px: number, py: number, scale: number, annotationScale = 1): boolean {
  return distToStroke(stroke.points, px, py) < stroke.thickness * annotationScale + 6 / scale;
}

/** The topmost (last drawn) stroke under the point, or undefined */
export function strokeAt(
  strokes: readonly Stroke[],
  px: number,
  py: number,
  scale: number,
  annotationScale = 1,
): Stroke | undefined {
  for (let i = strokes.length - 1; i >= 0; i--) {
    if (hitsStroke(strokes[i], px, py, scale, annotationScale)) return strokes[i];
  }
  return undefined;
}

/** A copy of the stroke moved by (dx, dy) with a new id (duplicate, move, crop) */
export function offsetStrokeCopy(stroke: Stroke, dx: number, dy: number, id: string): Stroke {
  return { ...stroke, id, points: stroke.points.map((p) => ({ x: p.x + dx, y: p.y + dy })) };
}

/** Bounding box of the points (for the selection frame) */
export function strokeBounds(points: readonly StrokePoint[]): { x: number; y: number; width: number; height: number } | null {
  if (points.length === 0) return null;
  let minX = Infinity;
  let minY = Infinity;
  let maxX = -Infinity;
  let maxY = -Infinity;
  for (const p of points) {
    minX = Math.min(minX, p.x);
    minY = Math.min(minY, p.y);
    maxX = Math.max(maxX, p.x);
    maxY = Math.max(maxY, p.y);
  }
  return { x: minX, y: minY, width: maxX - minX, height: maxY - minY };
}
