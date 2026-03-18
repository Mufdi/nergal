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
  plans_directory: string;
  transcripts_directory: string;
  hook_socket_path: string;
  default_shell: string;
  theme_mode: string;
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
}

export interface DiffLine {
  type: "addition" | "deletion" | "context";
  content: string;
  line_number?: number;
}
