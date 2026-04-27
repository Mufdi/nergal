import { atom } from "jotai";
import type { Config } from "@/lib/types";

export const configAtom = atom<Config>({
  claude_binary: "claude",
  plans_directory: "",
  transcripts_directory: "",
  hook_socket_path: "",
  default_shell: "/bin/bash",
  theme_mode: "dark",
  preferred_editor: "",
  terminal_kitty_keyboard: true,
});
