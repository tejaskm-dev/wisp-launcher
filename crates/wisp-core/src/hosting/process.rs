//! A running server: its console (kept, and passed on as it comes), the commands typed into it,
//! who's on, and stopping it carefully so it saves its world first.

use std::collections::VecDeque;
use std::process::Stdio;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};
use std::time::Duration;

use serde::Serialize;
use tokio::io::{AsyncBufReadExt, AsyncRead, AsyncWriteExt, BufReader};
use tokio::sync::{mpsc, oneshot, watch};

use super::install::StartPlan;

/// How much of the console is kept.
const CONSOLE_LINES: usize = 1500;
/// How long a server gets to save and stop before it's ended.
const STOP_GRACE: Duration = Duration::from_secs(90);
/// Console lines are passed on in batches this far apart, not one event per line.
const BATCH_EVERY: Duration = Duration::from_millis(120);

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "kebab-case")]
pub enum RunState {
    /// Java has started; the world is loading.
    Starting,
    /// Players can join.
    Running,
    Stopping,
    Stopped,
    Crashed,
}

impl RunState {
    pub fn is_over(self) -> bool {
        matches!(self, Self::Stopped | Self::Crashed)
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ConsoleLine {
    /// Unix milliseconds.
    pub at: i64,
    pub text: String,
    /// "info", "warn" or "error" as the server logged it, "input" for a command typed in, or
    /// "wisp" for what Wisp itself did.
    pub level: &'static str,
}

#[derive(Debug, Clone, Serialize)]
#[serde(tag = "kind", rename_all = "kebab-case", rename_all_fields = "camelCase")]
pub enum ServerEvent {
    Lines { lines: Vec<ConsoleLine> },
    State { state: RunState },
    Players { players: Vec<String> },
}

/// How a server ended.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Exit {
    pub code: Option<i32>,
    pub crashed: bool,
    /// What went wrong, when the server said: "eula" (not agreed to) or "port" (taken).
    pub reason: Option<&'static str>,
}

/// What the server said that Wisp acts on.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Said {
    Ready,
    Joined(String),
    Left(String),
    NeedsEula,
    PortTaken,
    Stopping,
}

/// Reads one console line. Vanilla, Fabric, Forge and NeoForge all print these the same way
/// (Forge adds a logger name in brackets before the colon).
pub fn read_line(line: &str) -> Option<Said> {
    let message = line.rsplit_once("]: ").map_or(line, |(_, message)| message).trim_end();
    if message.starts_with("Done (") && message.contains("For help, type") {
        return Some(Said::Ready);
    }
    if line.contains("You need to agree to the EULA") {
        return Some(Said::NeedsEula);
    }
    if line.contains("FAILED TO BIND TO PORT") {
        return Some(Said::PortTaken);
    }
    if message == "Stopping server" || message == "Stopping the server" {
        return Some(Said::Stopping);
    }
    let name_before = |suffix: &str| {
        let name = message.strip_suffix(suffix)?;
        (!name.is_empty() && name.len() <= 16 && name.chars().all(|c| c.is_ascii_alphanumeric() || c == '_')).then(|| name.to_string())
    };
    if let Some(name) = name_before(" joined the game") {
        return Some(Said::Joined(name));
    }
    name_before(" left the game").map(Said::Left)
}

fn level_of(line: &str) -> &'static str {
    let end = line.find("]: ").map_or(line.len(), |index| index + 1).min(160);
    let head = line.get(..end).unwrap_or(line);
    // Vanilla, Fabric and Forge write "[Server thread/WARN]"; Paper and its forks "[12:00:01 WARN]".
    if head.contains("/ERROR]") || head.contains("/FATAL]") || head.contains("[ERROR]") || head.contains(" ERROR]") || head.contains(" FATAL]") {
        "error"
    } else if head.contains("/WARN]") || head.contains("[WARN]") || head.contains(" WARN]") {
        "warn"
    } else {
        "info"
    }
}

type Events = Arc<dyn Fn(ServerEvent) + Send + Sync>;

pub struct ServerProcess {
    stdin: tokio::sync::Mutex<Option<tokio::process::ChildStdin>>,
    console: Mutex<VecDeque<ConsoleLine>>,
    players: Mutex<Vec<String>>,
    state: watch::Sender<RunState>,
    stop_asked: AtomicBool,
    started_at: i64,
    ready_at: Mutex<Option<i64>>,
    exit: Mutex<Option<Exit>>,
    reason: Mutex<Option<&'static str>>,
    kill: Mutex<Option<oneshot::Sender<()>>>,
    lines: mpsc::UnboundedSender<ConsoleLine>,
    events: Events,
    now: fn() -> i64,
}

fn lock<T>(mutex: &Mutex<T>) -> std::sync::MutexGuard<'_, T> {
    mutex.lock().unwrap_or_else(|poisoned| poisoned.into_inner())
}

/// Starts the server. `events` hears about console lines, the state and who's on as they change.
pub fn start(plan: &StartPlan, now: fn() -> i64, events: Events) -> std::io::Result<Arc<ServerProcess>> {
    let mut command = tokio::process::Command::new(&plan.java);
    command
        .args(&plan.args)
        .current_dir(&plan.dir)
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        // If Wisp goes, the server goes with it rather than holding its port and world.
        .kill_on_drop(true);
    #[cfg(windows)]
    command.creation_flags(0x0800_0000); // CREATE_NO_WINDOW
    let mut child = command.spawn()?;
    let (lines_tx, mut lines_rx) = mpsc::unbounded_channel::<ConsoleLine>();
    let (kill_tx, kill_rx) = oneshot::channel();
    let (state, _) = watch::channel(RunState::Starting);
    let process = Arc::new(ServerProcess {
        stdin: tokio::sync::Mutex::new(child.stdin.take()),
        console: Mutex::new(VecDeque::with_capacity(CONSOLE_LINES)),
        players: Mutex::new(Vec::new()),
        state,
        stop_asked: AtomicBool::new(false),
        started_at: now(),
        ready_at: Mutex::new(None),
        exit: Mutex::new(None),
        reason: Mutex::new(None),
        kill: Mutex::new(Some(kill_tx)),
        lines: lines_tx,
        events: events.clone(),
        now,
    });
    (events)(ServerEvent::State { state: RunState::Starting });

    let mut readers = Vec::new();
    if let Some(stdout) = child.stdout.take() {
        readers.push(tokio::spawn(read_output(stdout, process.clone())));
    }
    if let Some(stderr) = child.stderr.take() {
        readers.push(tokio::spawn(read_output(stderr, process.clone())));
    }
    // Console lines go out in small batches.
    {
        let events = events.clone();
        tokio::spawn(async move {
            let mut batch = Vec::new();
            loop {
                let Some(line) = lines_rx.recv().await else {
                    break;
                };
                batch.push(line);
                tokio::time::sleep(BATCH_EVERY).await;
                while let Ok(line) = lines_rx.try_recv() {
                    batch.push(line);
                }
                (events)(ServerEvent::Lines { lines: std::mem::take(&mut batch) });
            }
        });
    }
    {
        let process = process.clone();
        tokio::spawn(async move {
            let status = tokio::select! {
                status = child.wait() => status,
                _ = kill_rx => {
                    let _ = child.start_kill();
                    child.wait().await
                }
            };
            for reader in readers {
                let _ = tokio::time::timeout(Duration::from_secs(2), reader).await;
            }
            let code = status.ok().and_then(|status| status.code());
            let asked = process.stop_asked.load(Ordering::SeqCst);
            let reason = *lock(&process.reason);
            let crashed = !asked && (code != Some(0) || reason.is_some());
            *lock(&process.exit) = Some(Exit { code, crashed, reason });
            lock(&process.players).clear();
            *process.stdin.lock().await = None;
            process.set_state(if crashed { RunState::Crashed } else { RunState::Stopped });
            (process.events)(ServerEvent::Players { players: Vec::new() });
        });
    }
    Ok(process)
}

/// Reads the server's output to the end (past bytes that aren't UTF-8 too: an unread pipe would
/// stall the server once full).
async fn read_output(stream: impl AsyncRead + Unpin, process: Arc<ServerProcess>) {
    let mut reader = BufReader::new(stream);
    let mut buffer = Vec::new();
    loop {
        buffer.clear();
        match reader.read_until(b'\n', &mut buffer).await {
            Ok(0) | Err(_) => break,
            Ok(_) => {}
        }
        let text = plain(&String::from_utf8_lossy(&buffer)).trim_end().to_string();
        if text.is_empty() {
            continue;
        }
        process.heard(&text);
        process.log(ConsoleLine { at: (process.now)(), level: level_of(&text), text });
    }
}

/// The text without terminal colour codes (NeoForge colours its console, which shows as `[m[32m`
/// anywhere but a terminal).
fn plain(text: &str) -> String {
    let mut out = String::with_capacity(text.len());
    let mut chars = text.chars().peekable();
    while let Some(c) = chars.next() {
        if c != '\u{1b}' {
            out.push(c);
            continue;
        }
        // CSI: ESC [ … final byte in @–~.
        if chars.next_if_eq(&'[').is_some() {
            for next in chars.by_ref() {
                if ('@'..='~').contains(&next) {
                    break;
                }
            }
        }
    }
    out
}

impl ServerProcess {
    fn set_state(&self, state: RunState) {
        if *self.state.borrow() != state {
            self.state.send_replace(state);
            (self.events)(ServerEvent::State { state });
        }
    }

    fn log(&self, line: ConsoleLine) {
        {
            let mut console = lock(&self.console);
            if console.len() == CONSOLE_LINES {
                console.pop_front();
            }
            console.push_back(line.clone());
        }
        let _ = self.lines.send(line);
    }

    fn heard(&self, line: &str) {
        let Some(said) = read_line(line) else {
            return;
        };
        match said {
            Said::Ready => {
                *lock(&self.ready_at) = Some((self.now)());
                if self.state() == RunState::Starting {
                    self.set_state(RunState::Running);
                }
            }
            Said::Joined(name) => {
                let players = {
                    let mut players = lock(&self.players);
                    if !players.contains(&name) {
                        players.push(name);
                    }
                    players.clone()
                };
                (self.events)(ServerEvent::Players { players });
            }
            Said::Left(name) => {
                let players = {
                    let mut players = lock(&self.players);
                    players.retain(|player| *player != name);
                    players.clone()
                };
                (self.events)(ServerEvent::Players { players });
            }
            Said::NeedsEula => *lock(&self.reason) = Some("eula"),
            Said::PortTaken => *lock(&self.reason) = Some("port"),
            Said::Stopping => {
                if !self.state().is_over() {
                    self.set_state(RunState::Stopping);
                }
            }
        }
    }

    pub fn state(&self) -> RunState {
        *self.state.borrow()
    }

    pub fn players(&self) -> Vec<String> {
        lock(&self.players).clone()
    }

    /// Writes a line of Wisp's own into the console (what it did about the server).
    pub fn note(&self, text: &str) {
        self.log(ConsoleLine { at: (self.now)(), text: format!("[Wisp] {text}"), level: "wisp" });
    }

    pub fn console(&self) -> Vec<ConsoleLine> {
        lock(&self.console).iter().cloned().collect()
    }

    pub fn started_at(&self) -> i64 {
        self.started_at
    }

    /// When players could first join.
    pub fn ready_at(&self) -> Option<i64> {
        *lock(&self.ready_at)
    }

    /// How it ended, once it has.
    pub fn exit(&self) -> Option<Exit> {
        lock(&self.exit).clone()
    }

    /// Types a command into the server's console (without the slash).
    pub async fn send(&self, command: &str) -> std::io::Result<()> {
        let command = command.trim().trim_start_matches('/');
        if command.is_empty() {
            return Ok(());
        }
        let mut stdin = self.stdin.lock().await;
        let Some(stdin) = stdin.as_mut() else {
            return Err(std::io::Error::other("the server isn't running"));
        };
        stdin.write_all(format!("{command}\n").as_bytes()).await?;
        stdin.flush().await?;
        self.log(ConsoleLine { at: (self.now)(), text: format!("> {command}"), level: "input" });
        Ok(())
    }

    /// Asks the server to save and stop, and waits; ends it if it hasn't after a while.
    pub async fn stop(&self) -> Exit {
        if !self.state().is_over() {
            self.stop_asked.store(true, Ordering::SeqCst);
            self.set_state(RunState::Stopping);
            if self.send("stop").await.is_err() {
                self.kill();
            }
            if tokio::time::timeout(STOP_GRACE, self.wait()).await.is_err() {
                self.kill();
            }
        }
        self.wait().await
    }

    /// Ends the server at once, without saving.
    pub fn kill(&self) {
        self.stop_asked.store(true, Ordering::SeqCst);
        if let Some(kill) = lock(&self.kill).take() {
            let _ = kill.send(());
        }
    }

    /// Waits for the server to end.
    pub async fn wait(&self) -> Exit {
        let mut state = self.state.subscribe();
        let _ = state.wait_for(|state| state.is_over()).await;
        self.exit().unwrap_or(Exit { code: None, crashed: false, reason: None })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn reads_papers_levels_too() {
        assert_eq!(level_of("[16:15:19 WARN]: Failed to load eula.txt"), "warn");
        assert_eq!(level_of("[16:15:19 ERROR]: Could not load plugin"), "error");
        assert_eq!(level_of("[16:15:19 INFO]: Done (3.1s)! For help, type \"help\""), "info");
        assert_eq!(read_line("[16:15:19 INFO]: Done (3.1s)! For help, type \"help\""), Some(Said::Ready));
    }

    #[test]
    fn takes_colour_codes_out_of_console_lines() {
        assert_eq!(plain("\u{1b}[m\u{1b}[32m[15:52:31.192] [main/INFO] [LaunchServiceHandler/MODLAUNCHER]: Launching\u{1b}[0m"), "[15:52:31.192] [main/INFO] [LaunchServiceHandler/MODLAUNCHER]: Launching");
        assert_eq!(plain("no codes [here]"), "no codes [here]");
    }

    #[test]
    fn reads_what_servers_say() {
        assert_eq!(read_line(r#"[12:00:01] [Server thread/INFO]: Done (3.912s)! For help, type "help""#), Some(Said::Ready));
        assert_eq!(read_line(r#"[12:00:01] [Server thread/INFO] [minecraft/DedicatedServer]: Done (21.40s)! For help, type "help""#), Some(Said::Ready));
        assert_eq!(read_line("[12:01:00] [Server thread/INFO]: Alex_2 joined the game"), Some(Said::Joined("Alex_2".into())));
        assert_eq!(read_line("[12:01:00] [Server thread/INFO] [minecraft/MinecraftServer]: Steve left the game"), Some(Said::Left("Steve".into())));
        // Chat that only looks like it.
        assert_eq!(read_line("[12:01:00] [Server thread/INFO]: <Steve> Alex joined the game"), None);
        assert_eq!(read_line("[12:00:00] [ServerMain/WARN]: Failed to load eula.txt"), None);
        assert_eq!(read_line("[12:00:00] [ServerMain/INFO]: You need to agree to the EULA in order to run the server. Go to eula.txt for more info."), Some(Said::NeedsEula));
        assert_eq!(read_line("[12:00:00] [Server thread/WARN]: **** FAILED TO BIND TO PORT!"), Some(Said::PortTaken));
        assert_eq!(read_line("[12:09:00] [Server thread/INFO]: Stopping server"), Some(Said::Stopping));
        assert_eq!(level_of("[12:00:00] [Server thread/WARN]: Can't keep up!"), "warn");
        assert_eq!(level_of("[12:00:00] [Server thread/ERROR]: Encountered an unexpected exception"), "error");
        assert_eq!(level_of("[12:00:00] [Server thread/INFO]: ERROR is just a word here"), "info");
    }

    /// Runs a stand-in server (a shell script) and talks to it.
    #[cfg(unix)]
    #[tokio::test]
    async fn runs_talks_to_and_stops_a_server() {
        let dir = std::env::temp_dir().join(format!("wisp-server-process-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let script = r#"echo '[12:00:00] [Server thread/INFO]: Starting minecraft server'
echo '[12:00:01] [Server thread/INFO]: Done (1.0s)! For help, type "help"'
while read line; do
  case "$line" in
    join) echo '[12:00:02] [Server thread/INFO]: Alex joined the game' ;;
    leave) echo '[12:00:03] [Server thread/INFO]: Alex left the game' ;;
    stop) echo '[12:00:04] [Server thread/INFO]: Stopping server'; exit 0 ;;
  esac
done"#;
        let plan = StartPlan { java: "/bin/sh".into(), args: vec!["-c".into(), script.into()], dir: dir.clone() };
        let heard = Arc::new(Mutex::new(Vec::new()));
        let events: Events = {
            let heard = heard.clone();
            Arc::new(move |event| lock(&heard).push(event))
        };
        let server = start(&plan, || 7, events).unwrap();
        let mut state = server.state.subscribe();
        state.wait_for(|state| *state == RunState::Running).await.unwrap();
        server.send("/join").await.unwrap();
        for _ in 0..50 {
            if server.players() == ["Alex"] {
                break;
            }
            tokio::time::sleep(Duration::from_millis(20)).await;
        }
        assert_eq!(server.players(), ["Alex"]);
        server.send("leave").await.unwrap();
        let exit = server.stop().await;
        assert_eq!(exit, Exit { code: Some(0), crashed: false, reason: None });
        assert_eq!(server.state(), RunState::Stopped);
        assert!(server.players().is_empty());
        let console = server.console();
        assert!(console.iter().any(|line| line.text == "> join" && line.level == "input"));
        assert!(console.iter().any(|line| line.text.ends_with("Stopping server")));
        tokio::time::sleep(BATCH_EVERY * 3).await;
        let heard = lock(&heard);
        assert!(heard.iter().any(|event| matches!(event, ServerEvent::State { state: RunState::Running })));
        assert!(heard.iter().any(|event| matches!(event, ServerEvent::Lines { lines } if !lines.is_empty())));
        std::fs::remove_dir_all(dir).unwrap();
    }

    #[cfg(unix)]
    #[tokio::test]
    async fn a_server_that_dies_crashed_and_one_without_the_eula_says_so() {
        let dir = std::env::temp_dir().join(format!("wisp-server-crash-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let run = |script: &str| StartPlan { java: "/bin/sh".into(), args: vec!["-c".into(), script.into()], dir: dir.clone() };
        let quiet: Events = Arc::new(|_| {});
        let crashed = start(&run("echo boom; exit 1"), || 0, quiet.clone()).unwrap().wait().await;
        assert_eq!(crashed, Exit { code: Some(1), crashed: true, reason: None });
        let eula = start(&run("echo '[12:00:00] [ServerMain/INFO]: You need to agree to the EULA in order to run the server.'; exit 0"), || 0, quiet).unwrap().wait().await;
        assert_eq!(eula.reason, Some("eula"));
        assert!(eula.crashed);
        std::fs::remove_dir_all(dir).unwrap();
    }
}
