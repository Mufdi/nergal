import type { ImageRun } from "../../lib/types";

export const KITTY_UNDER_BG_Z = -1073741824;

export interface ImageLayers {
  underBackground: ImageRun[];
  underText: ImageRun[];
  overText: ImageRun[];
}

export function imageLayers(runs: ImageRun[]): ImageLayers {
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
