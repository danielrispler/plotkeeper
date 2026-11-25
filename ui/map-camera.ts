// The Map view's camera (spec #380, ticket #383; spec #548 Group 4), as pure logic. Split from `map-model.ts` by concern.

import type { Point, Rect } from "./map-model.js";
import type { Camera } from "./types.js";

// ---- The map's camera (spec #380, ticket #383): pan and zoom applied to the world, in place of a scrolling box. A point `p` of the world
// lands at `p * scale + (x, y)` in the map area, as with `briefCamera`.

const zoomLimits = { min: 0.4, max: 2 };
/** Zooms `camera` by `factor` around the area's point (px, py), which keeps the world point under it; the scale is held to `zoomLimits`. */
export function zoomAt(camera: Camera, factor: number, px: number, py: number): Camera {
  const scale = Math.min(zoomLimits.max, Math.max(zoomLimits.min, camera.scale * factor));
  const r = scale / camera.scale;
  return { scale, x: px - (px - camera.x) * r, y: py - (py - camera.y) * r };
}
/** The camera a drag of the empty background gives: `start` moved by the pointer's travel from `from` to `at`, at the same scale. */
export const panTo = (start: Camera, from: Point, at: Point): Camera => ({ scale: start.scale, x: start.x + at.x - from.x, y: start.y + at.y - from.y });
/**
 * The camera that fits `box` (the whole map) into an area `area` large with `gutter` kept around it, centred; never above 125 % (a small map is
 * not blown up) nor outside `zoomLimits`.
 */
export function fitCamera(box: Rect, area: { w: number; h: number }, gutter: number): Camera {
  const room = { w: Math.max(1, area.w - 2 * gutter), h: Math.max(1, area.h - 2 * gutter) };
  const wanted = Math.min(room.w / Math.max(1, box.x1 - box.x0), room.h / Math.max(1, box.y1 - box.y0), 1.25);
  const scale = Math.min(zoomLimits.max, Math.max(zoomLimits.min, wanted));
  return { scale, x: area.w / 2 - ((box.x0 + box.x1) / 2) * scale, y: area.h / 2 - ((box.y0 + box.y1) / 2) * scale };
}
/**
 * Reveals plates spanning `left` to `right` of the world by panning just enough to bring them, with `wanted` gutter, inside the visible band of
 * the area (from the panel's right edge to the sheet's left edge, in area coordinates). The gutter shrinks to the room the band has; a span
 * wider than the band is left where it is. Only the camera moves: no position is written.
 */
export function revealPan(camera: Camera, left: number, right: number, band: { left: number; right: number }, wanted: number): Camera {
  const [lo, hi] = [left * camera.scale + camera.x, right * camera.scale + camera.x];
  const gutter = Math.min(wanted, (band.right - band.left - (hi - lo)) / 2);
  if (gutter < 0) return camera;
  if (hi + gutter > band.right) return { ...camera, x: camera.x + band.right - (hi + gutter) };
  if (lo - gutter < band.left) return { ...camera, x: camera.x + band.left - (lo - gutter) };
  return camera;
}
/**
 * The map's one camera, shared by Daniel's view and a PR brief's: a brief borrows it with `lend` while it plays (each step may lend again),
 * and `giveBack` shows Daniel's view as the brief found it. While lent, Daniel's moves are refused (`move` says false), so his view is never
 * the brief's.
 */
export type CameraRig = { shown: () => Camera; user: () => Camera; lent: () => boolean; move: (to: Camera) => boolean; lend: (to: Camera) => void; giveBack: () => Camera };
export function cameraRig(start: Camera): CameraRig {
  let user = start;
  let borrowed: Camera | null = null;
  return {
    shown: () => borrowed ?? user,
    user: () => user,
    lent: () => borrowed !== null,
    move: (to) => {
      if (borrowed) return false;
      user = to;
      return true;
    },
    lend: (to) => {
      borrowed = to;
    },
    giveBack: () => {
      borrowed = null;
      return user;
    },
  };
}
