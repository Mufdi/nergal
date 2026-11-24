import { describe, expect, it, vi, type Mock } from "vitest";
import {
  createImageCache,
  IMAGE_BUDGET_BYTES,
  IMAGE_CEILING_BYTES,
  parseImageHeader,
} from "./imageCache";

const MB = 1024 * 1024;

type Header = ReturnType<typeof parseImageHeader>;
type Bitmap = { width: number; height: number; close: Mock<() => void> };

function deferred<T>() {
  let resolve!: (v: T) => void;
  let reject!: (e: unknown) => void;
  const promise = new Promise<T>((res, rej) => {
    resolve = res;
    reject = rej;
  });
  return { promise, resolve, reject };
}

async function flush() {
  for (let i = 0; i < 20; i++) await Promise.resolve();
}

function buf(kind: 0 | 1, width: number, height: number, body = new Uint8Array(0)): ArrayBuffer {
  const out = new ArrayBuffer(9 + body.length);
  const view = new DataView(out);
  view.setUint8(0, kind);
  view.setUint32(1, width, true);
  view.setUint32(5, height, true);
  new Uint8Array(out, 9).set(body);
  return out;
}

// width 1024 and height mb*256 make width*height*4 exactly mb mebibytes.
function sizedBuf(mb: number): ArrayBuffer {
  return buf(0, 1024, mb * 256);
}

function harness() {
  const fetches: { key: string; hash: string; d: ReturnType<typeof deferred<ArrayBuffer>> }[] = [];
  const decodes: { header: Header; d: ReturnType<typeof deferred<Bitmap>>; done: boolean }[] = [];
  const bitmaps: Bitmap[] = [];
  const ready: string[] = [];

  const cache = createImageCache({
    key: "sess::shell",
    fetch: (key, hash) => {
      const d = deferred<ArrayBuffer>();
      fetches.push({ key, hash, d });
      return d.promise;
    },
    decode: (header) => {
      const d = deferred<Bitmap>();
      decodes.push({ header, d, done: false });
      return d.promise;
    },
    onReady: (hash) => ready.push(hash),
  });

  const fetchesFor = (hash: string) => fetches.filter((f) => f.hash === hash);

  async function load(hash: string, mb: number): Promise<Bitmap> {
    cache.get(hash);
    fetchesFor(hash)[fetchesFor(hash).length - 1].d.resolve(sizedBuf(mb));
    await flush();
    const open = decodes.filter((x) => !x.done);
    const pending = open[open.length - 1];
    pending.done = true;
    const bitmap: Bitmap = {
      width: pending.header.width,
      height: pending.header.height,
      close: vi.fn<() => void>(),
    };
    bitmaps.push(bitmap);
    pending.d.resolve(bitmap);
    await flush();
    return bitmap;
  }

  return { cache, fetches, decodes, ready, fetchesFor, load };
}

describe("constants", () => {
  it("budget is 256 MB and the hard ceiling is 512 MB", () => {
    expect(IMAGE_BUDGET_BYTES).toBe(256 * MB);
    expect(IMAGE_CEILING_BYTES).toBe(512 * MB);
  });
});

describe("parseImageHeader", () => {
  it("reads an RGBA header with its pixel bytes", () => {
    const body = new Uint8Array([1, 2, 3, 4]);
    const h = parseImageHeader(buf(0, 1, 1, body));
    expect(h.kind).toBe(0);
    expect(h.width).toBe(1);
    expect(h.height).toBe(1);
    expect(Array.from(h.body)).toEqual([1, 2, 3, 4]);
  });

  it("reads an encoded-file header little-endian", () => {
    const body = new Uint8Array([0x89, 0x50, 0x4e, 0x47]);
    const h = parseImageHeader(buf(1, 300, 70000, body));
    expect(h.kind).toBe(1);
    expect(h.width).toBe(300);
    expect(h.height).toBe(70000);
    expect(Array.from(h.body)).toEqual([0x89, 0x50, 0x4e, 0x47]);
  });
});

describe("fetch and ready", () => {
  it("fetches once per hash while in flight and while cached", async () => {
    const t = harness();
    expect(t.cache.get("a")).toBeUndefined();
    expect(t.cache.get("a")).toBeUndefined();
    expect(t.fetchesFor("a")).toHaveLength(1);

    await t.load("a", 1);
    expect(t.cache.get("a")).toBeDefined();
    expect(t.fetchesFor("a")).toHaveLength(1);
  });

  it("passes the key the cache was built with to the fetch", () => {
    const t = harness();
    t.cache.get("a");
    expect(t.fetches[0].key).toBe("sess::shell");
    expect(t.fetches[0].hash).toBe("a");
  });

  it("calls ready with the hash only once the bitmap is decoded", async () => {
    const t = harness();
    t.cache.get("a");
    t.fetchesFor("a")[0].d.resolve(sizedBuf(1));
    await flush();
    expect(t.ready).toEqual([]);

    const bitmap = { width: 1024, height: 256, close: vi.fn<() => void>() };
    t.decodes[0].d.resolve(bitmap);
    await flush();
    expect(t.ready).toEqual(["a"]);
    expect(t.cache.get("a")).toBe(bitmap);
  });

  it("accounts width x height x 4 bytes per bitmap", async () => {
    const t = harness();
    await t.load("a", 10);
    await t.load("b", 5);
    expect(t.cache.bytes()).toBe(15 * MB);
  });
});

describe("budget eviction", () => {
  it("drops the least recently drawn unreferenced image and closes its bitmap", async () => {
    const t = harness();
    t.cache.setVisible(new Set());
    const a = await t.load("a", 100);
    t.cache.touch("a");
    const b = await t.load("b", 100);
    t.cache.touch("b");
    const c = await t.load("c", 100);

    expect(a.close).toHaveBeenCalledTimes(1);
    expect(b.close).not.toHaveBeenCalled();
    expect(c.close).not.toHaveBeenCalled();
    expect(t.cache.bytes()).toBe(200 * MB);
    expect(t.cache.get("b")).toBe(b);
    expect(t.cache.get("c")).toBe(c);
  });

  it("fetches an evicted hash again on the next request", async () => {
    const t = harness();
    t.cache.setVisible(new Set());
    await t.load("a", 100);
    t.cache.touch("a");
    await t.load("b", 100);
    t.cache.touch("b");
    await t.load("c", 100);
    expect(t.fetchesFor("a")).toHaveLength(1);

    expect(t.cache.get("a")).toBeUndefined();
    expect(t.fetchesFor("a")).toHaveLength(2);
  });

  it("never evicts referenced images, and evicts them on the next insertion once unreferenced", async () => {
    const t = harness();
    t.cache.setVisible(new Set(["a", "b", "c"]));
    const a = await t.load("a", 150);
    const b = await t.load("b", 150);
    const c = await t.load("c", 10);

    expect(a.close).not.toHaveBeenCalled();
    expect(b.close).not.toHaveBeenCalled();
    expect(c.close).not.toHaveBeenCalled();
    expect(t.cache.bytes()).toBe(310 * MB);

    t.cache.setVisible(new Set(["b", "c", "d"]));
    await t.load("d", 1);

    expect(a.close).toHaveBeenCalledTimes(1);
    expect(b.close).not.toHaveBeenCalled();
    expect(t.cache.bytes()).toBe(161 * MB);
  });

  it("gives an entry whose decode resolves after its row stopped referencing it the current tick", async () => {
    const t = harness();
    t.cache.setVisible(new Set());

    t.cache.get("b");
    const bFetch = t.fetchesFor("b")[0];

    const a = await t.load("a", 100);
    t.cache.touch("a");
    await t.load("d", 1);
    t.cache.touch("d");

    bFetch.d.resolve(sizedBuf(100));
    await flush();
    const bBitmap: Bitmap = { width: 1024, height: 100 * 256, close: vi.fn<() => void>() };
    const open = t.decodes.filter((x) => !x.done);
    open[open.length - 1].d.resolve(bBitmap);
    await flush();

    await t.load("c", 100);

    expect(a.close).toHaveBeenCalledTimes(1);
    expect(bBitmap.close).not.toHaveBeenCalled();
  });
});

describe("hard ceiling", () => {
  it("does not decode a new referenced image above 512 MB and retries once the visible set shrinks", async () => {
    const t = harness();
    t.cache.setVisible(new Set(["a", "b", "c", "d", "e"]));
    await t.load("a", 130);
    await t.load("b", 130);
    await t.load("c", 130);
    await t.load("d", 130);
    const decodesBefore = t.decodes.length;

    t.cache.get("e");
    t.fetchesFor("e")[0].d.resolve(sizedBuf(10));
    await flush();
    expect(t.decodes).toHaveLength(decodesBefore);
    expect(t.cache.get("e")).toBeUndefined();

    t.cache.setVisible(new Set(["d", "e"]));
    t.cache.get("e");
    expect(t.fetchesFor("e")).toHaveLength(2);
    t.fetchesFor("e")[1].d.resolve(sizedBuf(10));
    await flush();

    expect(t.decodes).toHaveLength(decodesBefore + 1);
  });
});

describe("failures", () => {
  it("never retries a decode failure", async () => {
    const t = harness();
    t.cache.get("a");
    t.fetchesFor("a")[0].d.resolve(sizedBuf(1));
    await flush();
    t.decodes[0].d.reject(new Error("bad image"));
    await flush();

    for (let i = 0; i < 5; i++) {
      expect(t.cache.get("a")).toBeUndefined();
      await flush();
    }
    expect(t.fetchesFor("a")).toHaveLength(1);
    expect(t.decodes).toHaveLength(1);
    expect(t.ready).toEqual([]);
  });

  it("retries a fetch failure at most twice, each from setVisible", async () => {
    const t = harness();
    t.cache.get("a");
    t.fetchesFor("a")[0].d.reject(new Error("gone"));
    await flush();

    for (let attempt = 1; attempt <= 2; attempt++) {
      expect(t.cache.setVisible(new Set(["a"]))).toEqual(["a"]);
      expect(t.fetchesFor("a")).toHaveLength(attempt + 1);
      t.fetchesFor("a")[attempt].d.reject(new Error("gone"));
      await flush();
    }

    expect(t.cache.setVisible(new Set(["a"]))).toEqual([]);
    expect(t.fetchesFor("a")).toHaveLength(3);
    expect(t.ready).toEqual([]);
  });

  it("never retries a fetch failure from repeated get calls", async () => {
    const t = harness();
    t.cache.get("a");
    t.fetchesFor("a")[0].d.reject(new Error("gone"));
    await flush();

    for (let i = 0; i < 5; i++) {
      expect(t.cache.get("a")).toBeUndefined();
      await flush();
    }
    expect(t.fetchesFor("a")).toHaveLength(1);

    expect(t.cache.setVisible(new Set(["a"]))).toEqual(["a"]);
    expect(t.fetchesFor("a")).toHaveLength(2);
  });

  it("marks the entry failed for good when decode throws synchronously", async () => {
    const fetches: { d: ReturnType<typeof deferred<ArrayBuffer>> }[] = [];
    const ready: string[] = [];
    const cache = createImageCache<Bitmap>({
      key: "k",
      fetch: () => {
        const d = deferred<ArrayBuffer>();
        fetches.push({ d });
        return d.promise;
      },
      decode: () => {
        throw new Error("zero dimension");
      },
      onReady: (hash) => ready.push(hash),
    });

    cache.get("a");
    fetches[0].d.resolve(sizedBuf(1));
    await flush();

    expect(cache.get("a")).toBeUndefined();
    expect(cache.setVisible(new Set(["a"]))).toEqual([]);
    expect(fetches).toHaveLength(1);
    expect(cache.bytes()).toBe(0);
    expect(ready).toEqual([]);
  });
});

describe("byte reservation", () => {
  it("decodes only the first of two referenced fetches that resolve before either decode when the second would pass the ceiling", async () => {
    const t = harness();
    t.cache.setVisible(new Set(["a", "b", "c", "d"]));
    await t.load("a", 250);
    await t.load("b", 250);

    t.cache.get("c");
    t.cache.get("d");
    t.fetchesFor("c")[0].d.resolve(sizedBuf(10));
    t.fetchesFor("d")[0].d.resolve(sizedBuf(10));
    await flush();

    expect(t.decodes.filter((x) => !x.done)).toHaveLength(1);
    expect(t.decodes).toHaveLength(3);
  });

  it("returns the blocked hash from setVisible once the visible set shrinks, and repaints it through ready", async () => {
    const t = harness();
    t.cache.setVisible(new Set(["a", "b", "c", "d"]));
    await t.load("a", 250);
    await t.load("b", 250);
    t.cache.get("c");
    t.cache.get("d");
    t.fetchesFor("c")[0].d.resolve(sizedBuf(10));
    t.fetchesFor("d")[0].d.resolve(sizedBuf(10));
    await flush();

    const unblocked = t.cache.setVisible(new Set(["a", "c", "d"]));
    expect(unblocked).toEqual(["d"]);
    expect(t.fetchesFor("d")).toHaveLength(2);

    t.fetchesFor("d")[1].d.resolve(sizedBuf(10));
    await flush();
    const open = t.decodes.filter((x) => !x.done);
    open[open.length - 1].d.resolve({ width: 1024, height: 2560, close: vi.fn<() => void>() });
    await flush();
    expect(t.ready).toContain("d");
  });

  it("returns an empty list when nothing was blocked or failed", async () => {
    const t = harness();
    await t.load("a", 1);
    expect(t.cache.setVisible(new Set(["a"]))).toEqual([]);
  });

  it("counts an encoded image as width x height x 4 bytes from its header", async () => {
    const t = harness();
    t.cache.get("a");
    t.fetchesFor("a")[0].d.resolve(buf(1, 1024, 25600, new Uint8Array([1, 2, 3])));
    await flush();
    t.decodes[0].d.resolve({ width: 1024, height: 25600, close: vi.fn<() => void>() });
    await flush();

    expect(t.cache.bytes()).toBe(100 * MB);
  });

  it("evicts unreferenced images to make room for an encoded one by its header size", async () => {
    const t = harness();
    t.cache.setVisible(new Set(["b"]));
    const a = await t.load("a", 200);
    t.cache.touch("a");

    t.cache.get("b");
    t.fetchesFor("b")[0].d.resolve(buf(1, 1024, 25600, new Uint8Array([1])));
    await flush();
    t.decodes[t.decodes.length - 1].d.resolve({
      width: 1024,
      height: 25600,
      close: vi.fn<() => void>(),
    });
    await flush();

    expect(a.close).toHaveBeenCalledTimes(1);
    expect(t.cache.bytes()).toBe(100 * MB);
  });
});

describe("releaseAll", () => {
  it("closes every bitmap and fetches again on the next get, staying usable", async () => {
    const t = harness();
    t.cache.setVisible(new Set(["a", "b"]));
    const a = await t.load("a", 1);
    const b = await t.load("b", 1);

    t.cache.releaseAll();

    expect(a.close).toHaveBeenCalledTimes(1);
    expect(b.close).toHaveBeenCalledTimes(1);
    expect(t.cache.bytes()).toBe(0);

    expect(t.cache.get("a")).toBeUndefined();
    expect(t.fetchesFor("a")).toHaveLength(2);

    t.fetchesFor("a")[1].d.resolve(sizedBuf(1));
    await flush();
    const open = t.decodes.filter((x) => !x.done);
    const bitmap = { width: 1024, height: 256, close: vi.fn<() => void>() };
    open[open.length - 1].d.resolve(bitmap);
    await flush();
    expect(t.cache.get("a")).toBe(bitmap);
  });
});

describe("dispose", () => {
  it("closes every decoded bitmap", async () => {
    const t = harness();
    t.cache.setVisible(new Set(["a", "b"]));
    const a = await t.load("a", 1);
    const b = await t.load("b", 1);

    t.cache.dispose();

    expect(a.close).toHaveBeenCalledTimes(1);
    expect(b.close).toHaveBeenCalledTimes(1);
  });

  it("skips the decode and the ready callback when a fetch resolves afterwards", async () => {
    const t = harness();
    t.cache.get("a");
    t.cache.dispose();

    t.fetchesFor("a")[0].d.resolve(sizedBuf(1));
    await flush();

    expect(t.decodes).toHaveLength(0);
    expect(t.ready).toEqual([]);
  });

  it("closes the bitmap and skips ready when a decode resolves afterwards", async () => {
    const t = harness();
    t.cache.get("a");
    t.fetchesFor("a")[0].d.resolve(sizedBuf(1));
    await flush();
    t.cache.dispose();

    const bitmap = { width: 1024, height: 256, close: vi.fn<() => void>() };
    t.decodes[0].d.resolve(bitmap);
    await flush();

    expect(bitmap.close).toHaveBeenCalledTimes(1);
    expect(t.ready).toEqual([]);
  });
});
