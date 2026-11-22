## ADDED Requirements

### Requirement: The previous run's log survives a restart

When Nergal runs under journald (`JOURNAL_STREAM` set) on Linux, the process that owns the log SHALL redirect its stdout and stderr to `<cache dir>/nergal/nergal.log`. Before it opens that file for the new run, it SHALL rename an existing `nergal.log` to `nergal.log.1`, replacing any older `nergal.log.1`. Nergal SHALL keep exactly one previous generation. A failed rename SHALL NOT stop the redirect.

#### Scenario: Restart after a crash keeps the crashed run's output
- **GIVEN** `nergal.log` holds the line `panicked at image.rs:103` from the previous run
- **WHEN** Nergal starts under journald
- **THEN** `nergal.log.1` SHALL hold the line `panicked at image.rs:103`
- **AND** `nergal.log` SHALL hold only output of the new run

#### Scenario: Only one previous generation is kept
- **GIVEN** `nergal.log` holds `run B` and `nergal.log.1` holds `run A`
- **WHEN** Nergal starts under journald
- **THEN** `nergal.log.1` SHALL hold `run B`
- **AND** no file SHALL hold `run A`

#### Scenario: First start without a previous log
- **GIVEN** no `nergal.log` exists
- **WHEN** Nergal starts under journald
- **THEN** Nergal SHALL create `nergal.log`
- **AND** Nergal SHALL NOT create `nergal.log.1`

### Requirement: Only one process owns the log

Before it renames or opens `nergal.log`, a process SHALL take an exclusive non-blocking lock on `<cache dir>/nergal/nergal.log.lock`. At startup it SHALL try once and never wait, so a second launch reaches the single-instance handoff without delay. A process that failed at startup and then passes the single-instance check (it is the primary instance, for example after an in-app relaunch whose parent is still exiting) SHALL retry for up to 3 seconds. The owner SHALL hold the lock for its whole life through a close-on-exec file descriptor, so child processes never inherit it. A process that never gets the lock SHALL leave `nergal.log` and `nergal.log.1` untouched and keep stdout and stderr as it inherited them.

#### Scenario: A second launch leaves the running instance's log alone
- **GIVEN** a running Nergal whose `nergal.log` holds `live run` and that holds the log lock
- **WHEN** a second Nergal process starts under journald before the single-instance handoff
- **THEN** the second process SHALL NOT rename, truncate or open `nergal.log` for writing
- **AND** `nergal.log` SHALL still hold `live run`

#### Scenario: The startup attempt never waits
- **GIVEN** another process holds the log lock
- **WHEN** a new process makes its startup attempt
- **THEN** the attempt SHALL return within 100 ms without the lock

#### Scenario: A relaunched primary waits for the exiting parent
- **GIVEN** a process that holds the log lock and releases it 500 ms later
- **WHEN** a primary instance that failed its startup attempt retries
- **THEN** it SHALL take the lock within the 3-second window
- **AND** it SHALL rotate and open the log as the owner

#### Scenario: The retry gives up after 3 seconds
- **GIVEN** another process holds the log lock and never releases it
- **WHEN** a primary instance retries
- **THEN** the retry SHALL give up after about 3 seconds
- **AND** `nergal.log` and `nergal.log.1` SHALL be unchanged

#### Scenario: The owner keeps the lock after startup finishes
- **GIVEN** the primary instance took the log lock at startup or on its retry
- **WHEN** Tauri `setup` returns and the app keeps running
- **THEN** a new lock attempt from another process SHALL still fail

#### Scenario: Children do not inherit the lock
- **GIVEN** the owner holds the log lock
- **WHEN** it spawns a child process and then drops its lock guard
- **THEN** a new lock attempt SHALL succeed while the child is still running
