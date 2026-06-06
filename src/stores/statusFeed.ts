import { atom } from "jotai";

/// Mirrors `ProviderStatus` in src-tauri/src/feeds.rs.
export interface ProviderStatus {
  provider: string;
  /// Statuspage indicator: "none" | "minor" | "major" | "critical",
  /// plus "unknown" when the status page itself was unreachable.
  indicator: string;
  description: string;
  url: string;
}

export const providerStatusAtom = atom<ProviderStatus[]>([]);

/// Providers with an active incident — what the StatusBar chip renders.
/// "unknown" is a fetch failure on our side, not an incident.
export const activeIncidentsAtom = atom<ProviderStatus[]>((get) =>
  get(providerStatusAtom).filter(
    (s) => s.indicator !== "none" && s.indicator !== "unknown",
  ),
);
