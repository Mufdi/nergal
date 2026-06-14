export interface PtyOutput {
  id: string;
  data: number[];
}

export interface HookEvent {
  session_id: string;
  cluihud_session_id?: string;
  event_type: string;
  tool_name?: string;
  tool_input?: unknown;
  stop_reason?: string;
  transcript_path?: string;
}

export interface Task {
  id: string;
  subject: string;
  description?: string;
  status: "pending" | "in_progress" | "completed";
  active_form?: string;
  blocked_by?: string[];
}

export interface CostSummary {
  input_tokens: number;
  output_tokens: number;
  cache_read: number;
  cache_write: number;
  total_usd: number;
}

export interface Config {
  claude_binary: string;
  transcripts_directory: string;
  hook_socket_path: string;
  default_shell: string;
  theme_mode: string;
  preferred_editor: string;
  terminal_kitty_keyboard: boolean;
  sidebar_dot_grid: boolean;
  panel_focus_pulse: boolean;
  panel_glow: boolean;
  scratchpad_path: string | null;
  default_agent: string | null;
  agent_overrides: Record<string, string>;
  custom_themes: CustomTheme[];
  keymap_overrides: Record<string, string>;
}

export interface CustomThemeFonts {
  interface: string;
  terminal: string;
  markdown: string;
}

export interface CustomTheme {
  id: string;
  label: string;
  base_id: string;
  primary: string;
  fonts: CustomThemeFonts;
}

export interface PathValidation {
  exists: boolean;
  is_dir: boolean;
  is_file: boolean;
  is_executable: boolean;
  resolved_path: string | null;
  error: string | null;
}

// ── wezterm-term renderer IPC types (Phase 4) ─────────────────────────

export interface CellSnapshot {
  ch: string;
  fg: [number, number, number, number] | null;
  bg: [number, number, number, number] | null;
  bold: boolean;
  italic: boolean;
  underline: boolean;
  reverse: boolean;
  hyperlink: string | null;
}

export interface CursorSnapshot {
  x: number;
  y: number;
  visible: boolean;
}

export interface GridRow {
  index: number;
  cells: CellSnapshot[];
}

export interface GridUpdate {
  sessionId: string;
  cols: number;
  totalRows: number;
  rows: GridRow[];
  cursor: CursorSnapshot;
  title: string | null;
  scrollOffset: number;
  isAltScreen: boolean;
}

export interface TerminalKeyEvent {
  code: string;
  key: string;
  text?: string;
  ctrl: boolean;
  shift: boolean;
  alt: boolean;
  meta: boolean;
}

export interface SessionInfo {
  id: string;
  active: boolean;
  cwd?: string;
}

export interface WorkspaceInfo {
  path: string;
  name: string;
  sessions: SessionInfo[];
}

export type ApprovalOption =
  | "accept_clear_context"
  | "accept_auto_edits"
  | "accept_manual_edits"
  | { feedback: string };

export type PlanMode = "view" | "edit";

export interface ActivityEntry {
  id: string;
  timestamp: number;
  type: "tool_use" | "file_modified" | "session" | "task" | "plan" | "error";
  message: string;
  detail?: string;
  /// Tool lifecycle: "running" set on PreToolUse, "done" patched on PostToolUse.
  status?: "running" | "done";
  /// Wall-clock pre→post duration, set when the tool completes.
  durationMs?: number;
  /// Files touched by the tool, derived from tool_input (Edit/Write/Read…).
  files?: string[];
  /// Raw tool name (for pre/post pairing, filtering, grouping).
  toolName?: string;
}

export interface DiffLine {
  type: "addition" | "deletion" | "context";
  content: string;
  line_number?: number;
}

export interface AvailableAgent {
  id: string;
  display_name: string;
  installed: boolean;
  binary_path: string | null;
  config_path: string | null;
  version: string | null;
  capabilities: string[];
  /// Kebab-case permission presets the adapter maps to real CLI flags
  /// ("default", "plan", "accept-edits", "auto", "bypass"). Drives the
  /// launch-options list in the agent picker.
  permission_presets: string[];
  /// Whether the adapter maps `allow_skip_in_cycle` to a real flag (CC
  /// `--allow-dangerously-skip-permissions`).
  allow_skip_cycle_supported: boolean;
}

export interface ThemePalette {
  id: string;
  isDark: boolean;
  surface: string;
  foreground: string;
  card: string;
  secondary: string;
  mutedForeground: string;
  border: string;
  accent: string;
}

export interface PlanSummary {
  name: string;
  path: string;
  modified: number;
}

export type PlanCapabilityWire =
  | { kind: "FileBased"; dir: string; label: string }
  | { kind: "NotApplicable" };

export type SessionPlansResponse =
  | { capability: "FileBased"; dir: string; plans: PlanSummary[] }
  | { capability: "NotApplicable"; plans: PlanSummary[] };

export interface ObsidianConfig {
  vault_root: string | null;
  vault_name: string | null;
  session_log_path: string | null;
  quick_capture_path: string | null;
  moc_path: string | null;
  templates_path: string | null;
  backlinks_enabled: boolean;
  render_wikilinks: boolean;
  /** Vault-relative folder that scopes vault search + the @@ picker. Null/empty = whole vault. */
  search_subdir: string | null;
}

export type ResolvedObsidianConfig = ObsidianConfig;
