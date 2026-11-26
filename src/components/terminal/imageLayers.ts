import type { ImageRun } from "../../lib/types";

export const KITTY_UNDER_BG_Z = -1073741824;

export interface ImageLayers {
  underBackground: ImageRun[];
  underText: ImageRun[];
  overText: ImageRun[];
}

const EMPTY_LAYERS: Readonly<ImageLayers> = Object.freeze({
  underBackground: Object.freeze([]) as unknown as ImageRun[],
  underText: Object.freeze([]) as unknown as ImageRun[],
  overText: Object.freeze([]) as unknown as ImageRun[],
});

export function imageLayers(runs: ImageRun[]): ImageLayers {
  if (runs.length === 0) return EMPTY_LAYERS;
  const layers: ImageLayers = { underBackground: [], underText: [], overText: [] };
  for (const run of runs) {
    if (run.z < KITTY_UNDER_BG_Z) layers.underBackground.push(run);
    else if (run.z < 0) layers.underText.push(run);
    else layers.overText.push(run);
  }
  // Array.prototype.sort is stable, so equal z keeps row order.
  layers.underBackground.sort((a, b) => a.z - b.z);
  layers.underText.sort((a, b) => a.z - b.z);
  layers.overText.sort((a, b) => a.z - b.z);
  return layers;
}
