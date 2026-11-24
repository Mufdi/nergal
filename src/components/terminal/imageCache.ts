const MB = 1024 * 1024;
export const IMAGE_BUDGET_BYTES = 256 * MB;
export const IMAGE_CEILING_BYTES = 512 * MB;

const HEADER_BYTES = 9;
const MAX_FETCH_RETRIES = 2;

export interface ImageHeader {
  kind: 0 | 1;
  width: number;
  height: number;
  body: Uint8Array;
}

export interface CachedBitmap {
  width: number;
  height: number;
  close(): void;
}

export interface ImageCacheOptions<B extends CachedBitmap> {
  key: string;
  fetch: (key: string, hash: string) => Promise<ArrayBuffer>;
  decode: (header: ImageHeader) => Promise<B>;
  onReady: (hash: string) => void;
}

export interface ImageCache<B extends CachedBitmap> {
  get(hash: string): B | undefined;
  touch(hash: string): void;
  setVisible(hashes: ReadonlySet<string>): void;
  dispose(): void;
  bytes(): number;
}

export function parseImageHeader(buf: ArrayBuffer): ImageHeader {
  const view = new DataView(buf);
  return {
    kind: view.getUint8(0) === 0 ? 0 : 1,
    width: view.getUint32(1, true),
    height: view.getUint32(5, true),
    body: new Uint8Array(buf, HEADER_BYTES),
  };
}

type Entry<B> =
  | { state: "loading" }
  | { state: "failed"; fetchFailures: number }
  | { state: "blocked" }
  | { state: "dead" }
  | { state: "ready"; bitmap: B; size: number; lastDrawn: number };

export function createImageCache<B extends CachedBitmap>(opts: ImageCacheOptions<B>): ImageCache<B> {
  const entries = new Map<string, Entry<B>>();
  const failures = new Map<string, number>();
  let visible: ReadonlySet<string> = new Set();
  let tick = 0;
  let total = 0;
  let disposed = false;

  function evictFor(incoming: number): void {
    if (total + incoming <= IMAGE_BUDGET_BYTES) return;
    const candidates: [string, Extract<Entry<B>, { state: "ready" }>][] = [];
    for (const [hash, e] of entries) {
      if (e.state === "ready" && !visible.has(hash)) candidates.push([hash, e]);
    }
    candidates.sort((a, b) => a[1].lastDrawn - b[1].lastDrawn);
    for (const [hash, e] of candidates) {
      if (total + incoming <= IMAGE_BUDGET_BYTES) break;
      e.bitmap.close();
      total -= e.size;
      entries.delete(hash);
    }
  }

  function unreferencedBytes(): number {
    let sum = 0;
    for (const [hash, e] of entries) {
      if (e.state === "ready" && !visible.has(hash)) sum += e.size;
    }
    return sum;
  }

  function startFetch(hash: string): void {
    entries.set(hash, { state: "loading" });
    opts
      .fetch(opts.key, hash)
      .then(
        (buf) => {
          if (disposed) return;
          onFetched(hash, buf);
        },
        () => {
          if (disposed) return;
          const count = (failures.get(hash) ?? 0) + 1;
          failures.set(hash, count);
          entries.set(hash, { state: "failed", fetchFailures: count });
        },
      );
  }

  function onFetched(hash: string, buf: ArrayBuffer): void {
    let header: ImageHeader;
    try {
      header = parseImageHeader(buf);
    } catch {
      entries.set(hash, { state: "dead" });
      return;
    }
    const size = header.width * header.height * 4;
    evictFor(size);
    if (total >= IMAGE_CEILING_BYTES) {
      entries.set(hash, { state: "blocked" });
      return;
    }
    opts.decode(header).then(
      (bitmap) => {
        if (disposed) {
          bitmap.close();
          return;
        }
        evictFor(size);
        entries.set(hash, { state: "ready", bitmap, size, lastDrawn: tick });
        total += size;
        opts.onReady(hash);
      },
      () => {
        if (disposed) return;
        entries.set(hash, { state: "dead" });
      },
    );
  }

  return {
    get(hash) {
      if (disposed) return undefined;
      const e = entries.get(hash);
      if (!e) {
        startFetch(hash);
        return undefined;
      }
      if (e.state === "ready") return e.bitmap;
      if (e.state === "failed" && e.fetchFailures <= MAX_FETCH_RETRIES) {
        startFetch(hash);
      }
      return undefined;
    },
    touch(hash) {
      const e = entries.get(hash);
      if (e?.state !== "ready") return;
      tick += 1;
      e.lastDrawn = tick;
    },
    setVisible(hashes) {
      visible = hashes;
      if (total < IMAGE_CEILING_BYTES || unreferencedBytes() > 0) {
        for (const [hash, e] of entries) {
          if (e.state === "blocked") entries.delete(hash);
        }
      }
    },
    dispose() {
      disposed = true;
      for (const e of entries.values()) {
        if (e.state === "ready") e.bitmap.close();
      }
      entries.clear();
      total = 0;
    },
    bytes: () => total,
  };
}
