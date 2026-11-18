import type { HooksStatus } from "@/lib/types";

export function shouldOpenHooksDialog(status: HooksStatus | null): boolean {
  return status !== null && !status.registered;
}

export function canConfigure(status: HooksStatus): boolean {
  return status.binary_on_path;
}
