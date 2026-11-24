Changes since round 2:
1. Eviction protects the visible set (hashes referenced by rowImages, passed via setVisible after each grid update), not a paint pass; overshoot while the visible set exceeds the budget; scenario + test rewritten as proposed.
2. Cache and fetch use the attach key (${sessionId}::${shellId} for quake), never ownerSessionId.
3. iTerm2 fact corrected (downscaled non-PNG/GIF/WebP → Rgba8); BMP test must avoid downscale; WebP note in D3.
4. createImageBitmap with imageOrientation "none".
5. Post-dispose fetch skips decode; test asserts decode not called.
