const MB = 1024 * 1024;
export const IMAGE_BUDGET_BYTES = 256 * MB;
export const IMAGE_CEILING_BYTES = 512 * MB;

const HEADER_BYTES = 9;
const MAX_FETCH_RETRIES = 2;
const MAX_IN_FLIGHT_FETCHES = 2;
const UNSUPPORTED_FORMAT = "unsupported image format";

export interface ImageHeader {
  kind: 0 | 1;
  width: number;
  height: number;
  body: Uint8Array<ArrayBuffer>;
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
  setVisible(hashes: ReadonlySet<string>): string[];
  releaseAll(): void;
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

export function headerPixels(header: ImageHeader): Uint8ClampedArray<ArrayBuffer> {
  const { buffer, byteOffset, byteLength } = header.body;
  return new Uint8ClampedArray(buffer, byteOffset, byteLength);
}

export function headerBlob(header: ImageHeader): Blob {
  return new Blob([header.body]);
}

type Entry<B> =
  | { state: "queued" }
  | { state: "loading" }
  | { state: "decoding" }
  | { state: "failed"; fetchFailures: number }
  | { state: "blocked"; size: number }
  | { state: "dead" }
  | { state: "ready"; bitmap: B; size: number; lastDrawn: number };

function isRetryable<B>(e: Entry<B> | undefined): boolean {
  return e?.state === "blocked" || (e?.state === "failed" && e.fetchFailures <= MAX_FETCH_RETRIES);
}

export function createImageCache<B extends CachedBitmap>(opts: ImageCacheOptions<B>): ImageCache<B> {
  const entries = new Map<string, Entry<B>>();
  const failures = new Map<string, number>();
  let visible: ReadonlySet<string> = new Set();
  let visibilityKnown = false;
  let tick = 0;
  let total = 0;
  let retryable = 0;
  let decoding = 0;
  let inFlight = 0;
  let queue: string[] = [];
  let epoch = 0;
  let disposed = false;
  let warnedNonBuffer = false;

  function put(hash: string, e: Entry<B>): void {
    if (isRetryable(entries.get(hash))) retryable -= 1;
    if (isRetryable(e)) retryable += 1;
    entries.set(hash, e);
  }

  function drop(hash: string): void {
    if (isRetryable(entries.get(hash))) retryable -= 1;
    entries.delete(hash);
  }

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
      drop(hash);
    }
  }

  // A lone decode may cross the ceiling; stacking another one on top of
  // in-flight decodes may not, or a burst of fetches would all pass the check.
  function overCeiling(size: number, held: number = total, restarted: number = 0): boolean {
    if (held >= IMAGE_CEILING_BYTES) return true;
    return decoding + restarted > 0 && held + size > IMAGE_CEILING_BYTES;
  }

  function unreferencedBytes(): number {
    let sum = 0;
    for (const [hash, e] of entries) {
      if (e.state === "ready" && !visible.has(hash)) sum += e.size;
    }
    return sum;
  }

  function retryBlocked(): void {
    if (retryable === 0) return;
    requestRetries(visible, false);
  }

  function requestRetries(hashes: ReadonlySet<string>, includeFailed: boolean): string[] {
    let held = total - unreferencedBytes();
    const requested: string[] = [];
    let restarted = 0;
    for (const [hash, e] of [...entries]) {
      if (!hashes.has(hash)) continue;
      if (includeFailed && e.state === "failed" && isRetryable(e)) {
        startFetch(hash);
        requested.push(hash);
      } else if (e.state === "blocked" && !overCeiling(e.size, held, restarted)) {
        held += e.size;
        restarted += 1;
        startFetch(hash);
        requested.push(hash);
      }
    }
    return requested;
  }

  function startFetch(hash: string): void {
    if (inFlight >= MAX_IN_FLIGHT_FETCHES) {
      put(hash, { state: "queued" });
      queue.push(hash);
      return;
    }
    beginFetch(hash);
  }

  function drainQueue(): void {
    while (inFlight < MAX_IN_FLIGHT_FETCHES && queue.length > 0) {
      const next = queue.shift();
      if (next === undefined || entries.get(next)?.state !== "queued") continue;
      if (!visibilityKnown || visible.has(next)) beginFetch(next);
      else drop(next);
    }
  }

  function isUnsupported(reason: unknown): boolean {
    const message = reason instanceof Error ? reason.message : typeof reason === "string" ? reason : "";
    return message.includes(UNSUPPORTED_FORMAT);
  }

  function beginFetch(hash: string): void {
    put(hash, { state: "loading" });
    inFlight += 1;
    const startedIn = epoch;
    opts
      .fetch(opts.key, hash)
      .then(
        (buf) => {
          if (disposed) return;
          inFlight -= 1;
          if (startedIn === epoch || entries.get(hash)?.state === "queued") onFetched(hash, buf);
          drainQueue();
        },
        (reason) => {
          if (disposed) return;
          inFlight -= 1;
          if (startedIn === epoch) {
            if (isUnsupported(reason)) {
              put(hash, { state: "dead" });
            } else {
              const count = (failures.get(hash) ?? 0) + 1;
              failures.set(hash, count);
              put(hash, { state: "failed", fetchFailures: count });
            }
          }
          drainQueue();
        },
      );
  }

  function onFetched(hash: string, buf: ArrayBuffer): void {
    if (!(buf instanceof ArrayBuffer)) {
      if (!warnedNonBuffer) {
        warnedNonBuffer = true;
        console.warn("terminal image fetch returned a non-ArrayBuffer; the IPC transport must deliver raw bytes");
      }
      put(hash, { state: "dead" });
      return;
    }
    let header: ImageHeader;
    try {
      header = parseImageHeader(buf);
    } catch {
      put(hash, { state: "dead" });
      return;
    }
    const size = header.width * header.height * 4;
    evictFor(size);
    if (overCeiling(size)) {
      put(hash, { state: "blocked", size });
      return;
    }
    total += size;
    decoding += 1;
    put(hash, { state: "decoding" });
    const startedIn = epoch;

    const fail = () => {
      if (disposed || startedIn !== epoch) return;
      decoding -= 1;
      total -= size;
      put(hash, { state: "dead" });
      retryBlocked();
    };

    let decoded: Promise<B>;
    try {
      decoded = opts.decode(header);
    } catch {
      fail();
      return;
    }
    decoded.then((bitmap) => {
      if (disposed || startedIn !== epoch) {
        bitmap.close();
        return;
      }
      decoding -= 1;
      put(hash, { state: "ready", bitmap, size, lastDrawn: tick });
      opts.onReady(hash);
      retryBlocked();
    }, fail);
  }

  return {
    get(hash) {
      if (disposed) return undefined;
      const e = entries.get(hash);
      if (!e) {
        startFetch(hash);
        return undefined;
      }
      return e.state === "ready" ? e.bitmap : undefined;
    },
    touch(hash) {
      const e = entries.get(hash);
      if (e?.state !== "ready") return;
      tick += 1;
      e.lastDrawn = tick;
    },
    setVisible(hashes) {
      visible = hashes;
      visibilityKnown = true;
      if (disposed || retryable === 0) return [];
      return requestRetries(hashes, true);
    },
    releaseAll() {
      epoch += 1;
      for (const e of entries.values()) {
        if (e.state === "ready") e.bitmap.close();
      }
      entries.clear();
      failures.clear();
      total = 0;
      retryable = 0;
      decoding = 0;
      queue = [];
    },
    dispose() {
      disposed = true;
      for (const e of entries.values()) {
        if (e.state === "ready") e.bitmap.close();
      }
      entries.clear();
      total = 0;
      retryable = 0;
      queue = [];
    },
    bytes: () => total,
  };
}
