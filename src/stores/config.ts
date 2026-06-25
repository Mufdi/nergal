import { atom } from "jotai";
import type { Config } from "@/lib/types";
import { DEFAULT_THEME_ID } from "@/lib/themes";

export const settingsOpenAtom = atom(false);

/// When set, the SettingsPanel jumps to this section on its next open and
/// clears the atom. Lets callers deep-link into a section (e.g. the update
/// toast's "Open About" button) instead of landing on the default.
export const settingsRequestedSectionAtom = atom<string | null>(null);

export const configAtom = atom<Config>({
  claude_binary: "claude",
  transcripts_directory: "",
  hook_socket_path: "",
  default_shell: "/bin/bash",
  theme_mode: DEFAULT_THEME_ID,
  preferred_editor: "",
  terminal_kitty_keyboard: true,
  sidebar_dot_grid: false,
  panel_focus_pulse: false,
  panel_glow: false,
  scratchpad_path: null,
  default_agent: null,
  agent_overrides: {},
  custom_themes: [],
  keymap_overrides: {},
  mcp_server_enabled: false,
});
