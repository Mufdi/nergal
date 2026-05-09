/// Lazy glyph cache rendered into an offscreen canvas.
///
/// The hot path (draw a cell) is `drawGlyph` — it either blits a previously
/// rasterized glyph via `drawImage`, or rasterizes on demand and caches the
/// resulting tile's coordinates. Bg colors are drawn separately as solid
/// rects by the renderer; the atlas only stores foreground glyph pixels
/// keyed by (char, fg, bold, italic).
///
/// Why not pre-rasterize ASCII: the extra complexity (variants per style,
/// theme-change invalidation, atlas growth) buys little in Phase 4 because
/// the lazy path already caches on first use. Revisit if measurement says so.

export interface FontMetrics {
  /// Cell width in device pixels (DPR-scaled).
  cellWidth: number;
  /// Cell height in device pixels (DPR-scaled).
  cellHeight: number;
  /// Baseline offset (from top of cell, device pixels).
  baseline: number;
  /// CSS pixel sizes for layout.
  cssWidth: number;
  cssHeight: number;
}

interface GlyphSlot {
  x: number;
  y: number;
}

/// Color emojis are rendered by the browser as full-square glyphs centered
/// on the alphabetic baseline. With a baseline at 78% of cell height that
/// pushes the emoji's top above the slot rect, and `drawImage` later clips
/// it back to the slot — the user sees only the bottom sliver. Detecting
/// pictographics and centering them inside the cell keeps the whole glyph
/// visible.
const EMOJI_RE = /\p{Extended_Pictographic}/u;

export function measureFont(
  family: string,
  cssSize: number,
  lineHeight: number,
  dpr: number,
): FontMetrics {
  const probe = document.createElement("canvas");
  const ctx = probe.getContext("2d");
  if (!ctx) throw new Error("2d context unavailable");

  // `measureText` uses the DPR-scaled font when the probe context is scaled;
  // keep the math symmetric with the renderer so cell sizes line up.
  const pxSize = Math.round(cssSize * dpr);
  ctx.font = `${pxSize}px ${family}`;
  const sample = ctx.measureText("M");
  const cellWidth = Math.max(1, Math.round(sample.width));
  const cellHeight = Math.max(1, Math.round(pxSize * lineHeight));
  // Place the baseline roughly 2/3 down; most monospace fonts read well here.
  const baseline = Math.round(cellHeight * 0.78);

  return {
    cellWidth,
    cellHeight,
    baseline,
    cssWidth: cellWidth / dpr,
    cssHeight: cellHeight / dpr,
  };
}

export class FontAtlas {
  private canvas: HTMLCanvasElement;
  private ctx: CanvasRenderingContext2D;
  private slots = new Map<string, GlyphSlot>();
  private nextX = 0;
  private nextY = 0;
  private readonly family: string;
  private readonly pxSize: number;
  private readonly columns: number;
  private readonly rows: number;

  constructor(
    private readonly metrics: FontMetrics,
    family: string,
    cssSize: number,
    dpr: number,
  ) {
    this.family = family;
    this.pxSize = Math.round(cssSize * dpr);
    // 128 cells per row is plenty wide; 128 rows gives room for ~16k glyph
    // variants which is well past realistic need. Growth strategy is a TODO
    // if this ever gets close to full.
    this.columns = 128;
    this.rows = 128;
    this.canvas = document.createElement("canvas");
    this.canvas.width = metrics.cellWidth * this.columns;
    this.canvas.height = metrics.cellHeight * this.rows;
    const ctx = this.canvas.getContext("2d", { alpha: true });
    if (!ctx) throw new Error("atlas 2d context unavailable");
    this.ctx = ctx;
  }

  /// Draw a single glyph into the target context at (dx, dy) — rasterizing
  /// it into the atlas first if this is the first time we see this variant.
  drawGlyph(
    target: CanvasRenderingContext2D,
    dx: number,
    dy: number,
    ch: string,
    fg: string,
    bold: boolean,
    italic: boolean,
  ): void {
    if (ch === "" || ch === " ") return;

    const key = `${ch}\u0000${fg}\u0000${bold ? "1" : "0"}\u0000${italic ? "1" : "0"}`;
    let slot = this.slots.get(key);
    if (!slot) {
      slot = this.rasterize(key, ch, fg, bold, italic);
    }
    target.drawImage(
      this.canvas,
      slot.x,
      slot.y,
      this.metrics.cellWidth,
      this.metrics.cellHeight,
      dx,
      dy,
      this.metrics.cellWidth,
      this.metrics.cellHeight,
    );
  }

  private rasterize(
    key: string,
    ch: string,
    fg: string,
    bold: boolean,
    italic: boolean,
  ): GlyphSlot {
    const slot: GlyphSlot = {
      x: this.nextX * this.metrics.cellWidth,
      y: this.nextY * this.metrics.cellHeight,
    };
    this.nextX += 1;
    if (this.nextX >= this.columns) {
      this.nextX = 0;
      this.nextY += 1;
      if (this.nextY >= this.rows) {
        // Full: overwrite the oldest line. Simple round-robin eviction.
        this.nextY = 0;
        this.slots.clear();
      }
    }

    this.ctx.clearRect(slot.x, slot.y, this.metrics.cellWidth, this.metrics.cellHeight);
    this.ctx.fillStyle = fg;
    if (EMOJI_RE.test(ch)) {
      // Emojis at pxSize render roughly square (~pxSize × pxSize), but a
      // monospace cell is narrower (M-width is ~0.6 × pxSize). Painting at
      // pxSize would clip the emoji's left/right ~30% to the slot edges.
      // Scale the emoji font size down to the cell width so the whole glyph
      // fits, then center it. Wide-cell support (rendering across two cells)
      // is the proper long-term fix; tracked separately.
      const emojiSize = Math.min(this.pxSize, this.metrics.cellWidth);
      this.ctx.font = `${emojiSize}px ${this.family}`;
      this.ctx.textBaseline = "middle";
      this.ctx.textAlign = "center";
      this.ctx.fillText(
        ch,
        slot.x + this.metrics.cellWidth / 2,
        slot.y + this.metrics.cellHeight / 2,
      );
    } else {
      const style = `${italic ? "italic " : ""}${bold ? "bold " : ""}${this.pxSize}px ${this.family}`;
      this.ctx.font = style;
      this.ctx.textBaseline = "alphabetic";
      this.ctx.textAlign = "start";
      this.ctx.fillText(ch, slot.x, slot.y + this.metrics.baseline);
    }

    this.slots.set(key, slot);
    return slot;
  }
}
