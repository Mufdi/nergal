import { atom } from "jotai";
import type { Config } from "@/lib/types";
import { DEFAULT_THEME_ID } from "@/lib/themes";

export const settingsOpenAtom = atom(false);

export const configAtom = atom<Config>({
  claude_binary: "claude",
  plans_directory: "",
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
});
