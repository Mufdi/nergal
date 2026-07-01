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

/// Mirrors the richer per-provider detail from `get_provider_status_detail`
/// (src-tauri/src/feeds.rs) — rendered natively in the status-chip popover.
export interface ProviderComponentStatus {
  name: string;
  status: string;
}

export interface ProviderIncidentSummary {
  name: string;
  impact: string;
  status: string;
  latest_update: string | null;
  updated_at: string | null;
  shortlink: string;
}

export interface ProviderStatusDetail {
  provider: string;
  page_name: string;
  page_url: string;
  indicator: string;
  description: string;
  components: ProviderComponentStatus[];
  incidents: ProviderIncidentSummary[];
}
