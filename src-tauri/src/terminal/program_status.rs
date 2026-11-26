use base64::Engine;
use base64::engine::general_purpose::STANDARD;

pub const MAX_BODY: usize = 4096;

const PREFIX: &[u8] = b"\x1b]7501;";
const ESC: u8 = 0x1b;
const BEL: u8 = 0x07;
const CAN: u8 = 0x18;
const SUB: u8 = 0x1a;

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Osc7501 {
    Query,
    Report(Vec<u8>),
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum ScanState {
    Ground,
    Prefix(usize),
    Body,
    Skip,
}

#[derive(Debug)]
pub struct ProgramStatusScanner {
    state: ScanState,
    saw_esc: bool,
    body: Vec<u8>,
}

impl Default for ProgramStatusScanner {
    fn default() -> Self {
        Self {
            state: ScanState::Ground,
            saw_esc: false,
            body: Vec::new(),
        }
    }
}

impl ProgramStatusScanner {
    pub fn feed(&mut self, bytes: &[u8]) -> Vec<Osc7501> {
        let mut found = Vec::new();
        for &b in bytes {
            self.step(b, &mut found);
        }
        found
    }

    fn step(&mut self, b: u8, found: &mut Vec<Osc7501>) {
        match self.state {
            ScanState::Ground => {
                if b == ESC {
                    self.state = ScanState::Prefix(1);
                }
            }
            ScanState::Prefix(matched) => self.step_prefix(matched, b),
            ScanState::Body | ScanState::Skip => self.step_body(b, found),
        }
    }

    fn step_prefix(&mut self, matched: usize, b: u8) {
        if b == PREFIX[matched] {
            if matched + 1 == PREFIX.len() {
                self.state = ScanState::Body;
                self.saw_esc = false;
                self.body.clear();
            } else {
                self.state = ScanState::Prefix(matched + 1);
            }
        } else if b == ESC {
            self.state = ScanState::Prefix(1);
        } else {
            self.state = ScanState::Ground;
        }
    }

    fn step_body(&mut self, b: u8, found: &mut Vec<Osc7501>) {
        if self.saw_esc {
            self.saw_esc = false;
            if b == b'\\' {
                self.finish(found);
            } else if b == CAN || b == SUB {
                self.abort();
            } else {
                // The aborting ESC may itself start the next prefix.
                self.abort();
                self.state = ScanState::Prefix(1);
                self.step_prefix(1, b);
            }
            return;
        }
        match b {
            BEL => self.finish(found),
            ESC => self.saw_esc = true,
            CAN | SUB => self.abort(),
            _ if self.state == ScanState::Body => {
                if self.body.len() == MAX_BODY {
                    self.body.clear();
                    self.state = ScanState::Skip;
                } else {
                    self.body.push(b);
                }
            }
            _ => {}
        }
    }

    fn finish(&mut self, found: &mut Vec<Osc7501>) {
        if self.state == ScanState::Body {
            let body = std::mem::take(&mut self.body);
            found.push(if body == b"?" {
                Osc7501::Query
            } else {
                Osc7501::Report(body)
            });
        }
        self.abort();
    }

    fn abort(&mut self) {
        self.state = ScanState::Ground;
        self.saw_esc = false;
        self.body.clear();
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, serde::Serialize)]
#[serde(rename_all = "lowercase")]
pub enum RootState {
    Working,
    Blocked,
    Done,
    Idle,
    Error,
}

impl RootState {
    pub fn as_str(&self) -> &'static str {
        match self {
            RootState::Working => "working",
            RootState::Blocked => "blocked",
            RootState::Done => "done",
            RootState::Idle => "idle",
            RootState::Error => "error",
        }
    }

    fn parse(s: &str) -> Option<Self> {
        match s {
            "working" => Some(RootState::Working),
            "blocked" => Some(RootState::Blocked),
            "done" => Some(RootState::Done),
            "idle" => Some(RootState::Idle),
            "error" => Some(RootState::Error),
            _ => None,
        }
    }
}

#[derive(Clone, Debug, PartialEq, Eq, serde::Serialize)]
pub struct ProgramStatus {
    pub state: RootState,
    pub kind: Option<String>,
    pub msg: Option<String>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum ReportAction {
    Set(ProgramStatus),
    Clear,
    Ignore,
}

pub fn parse_report(body: &[u8]) -> ReportAction {
    let Ok(text) = std::str::from_utf8(body) else {
        return ReportAction::Ignore;
    };
    let mut state = None;
    let mut kind = None;
    let mut msg = None;
    for field in text.split(':') {
        let Some((key, value)) = field.split_once('=') else {
            continue;
        };
        match key {
            "id" => return ReportAction::Ignore,
            "state" if state.is_none() => state = Some(value),
            "kind" if kind.is_none() => kind = Some(value),
            "msg" if msg.is_none() => msg = Some(value),
            _ => {}
        }
    }
    match state {
        Some("clear") => ReportAction::Clear,
        Some(s) => match RootState::parse(s) {
            Some(state) => ReportAction::Set(ProgramStatus {
                state,
                kind: kind
                    .filter(|_| state == RootState::Blocked)
                    .map(str::to_string),
                msg: msg.and_then(decode_msg),
            }),
            None => ReportAction::Ignore,
        },
        None => ReportAction::Ignore,
    }
}

// msg reaches other agents as `waiting_for`, so control characters must not survive.
fn decode_msg(encoded: &str) -> Option<String> {
    let raw = STANDARD.decode(encoded).ok()?;
    Some(
        String::from_utf8_lossy(&raw)
            .chars()
            .filter(|c| !c.is_control())
            .collect(),
    )
}

#[cfg(test)]
#[path = "program_status_tests.rs"]
mod tests;
