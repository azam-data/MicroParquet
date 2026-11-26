//! Phase 14: the optional local model. Nothing here is needed to run the program: with no model in
//! `portable_data/assistant/` the assistant searches the guide book alone.
//!
//! The model runs in `llama-server` (the llama.cpp project's own program), started by MicroParquet on this PC, listening on
//! `127.0.0.1` only (this PC itself, no network card), and stopped when MicroParquet closes. MicroParquet talks to it with plain
//! HTTP over that loopback address, so no network library is added and no byte leaves the PC. The question and the guide-book
//! sections go to the model; the bank's data never does.

use anyhow::{anyhow, bail, Context, Result};
use std::io::{Read, Write};
use std::net::{Ipv4Addr, SocketAddr, TcpListener, TcpStream};
use std::path::{Path, PathBuf};
use std::process::{Child, Command, Stdio};
use std::time::{Duration, Instant};

/// The folder (inside `portable_data`) where the model program and the model file are copied.
pub const ASSISTANT_DIR: &str = "assistant";
/// How long a model may take to load (a 2 GB file from a USB stick can take a minute or two).
pub const LOAD_TIMEOUT: Duration = Duration::from_secs(300);
/// How long one answer may take on an ordinary office PC.
pub const ANSWER_TIMEOUT: Duration = Duration::from_secs(600);

/// What was found in the assistant folder.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct Setup {
    pub dir: PathBuf,
    /// `llama-server` (`.exe` on Windows).
    pub server: Option<PathBuf>,
    /// The model file (`.gguf`).
    pub model: Option<PathBuf>,
    pub model_bytes: u64,
    /// What is missing or unusual, in words.
    pub notes: Vec<String>,
}

impl Setup {
    /// Both the program and a model are there.
    pub fn complete(&self) -> bool {
        self.server.is_some() && self.model.is_some()
    }

    /// One line for the panel: what is ready, or what to copy in.
    pub fn status_line(&self) -> String {
        match (&self.server, &self.model) {
            (Some(_), Some(m)) => format!(
                "Model found: {} ({:.1} GB). Click \"Start the model\".",
                m.file_name().map(|n| n.to_string_lossy().to_string()).unwrap_or_default(),
                self.model_bytes as f64 / 1_073_741_824.0
            ),
            (None, Some(_)) => "A model file is there, but the model program (llama-server.exe) is not. Copy it into the folder shown below.".to_string(),
            (Some(_), None) => "The model program is there, but no model file (.gguf). Copy one into the folder shown below.".to_string(),
            (None, None) => "No model installed. The assistant searches the guide book. To add a model, copy it into the folder shown below.".to_string(),
        }
    }
}

fn walk(dir: &Path, depth: usize, out: &mut Vec<PathBuf>) {
    let Ok(rd) = std::fs::read_dir(dir) else { return };
    let mut entries: Vec<_> = rd.filter_map(|e| e.ok()).collect();
    entries.sort_by_key(|e| e.file_name());
    for e in entries {
        let p = e.path();
        if p.is_dir() {
            if depth > 0 {
                walk(&p, depth - 1, out);
            }
        } else {
            out.push(p);
        }
    }
}

/// Looks in `dir` and one or two folders below it (an unzipped llama.cpp folder may be copied in whole).
pub fn detect(dir: &Path) -> Setup {
    let mut files = Vec::new();
    walk(dir, 2, &mut files);
    let name = |p: &PathBuf| p.file_name().map(|n| n.to_string_lossy().to_lowercase()).unwrap_or_default();
    let server = files.iter().find(|p| {
        let n = name(p);
        n == "llama-server.exe" || n == "llama-server"
    });
    let mut models: Vec<&PathBuf> = files.iter().filter(|p| name(p).ends_with(".gguf") && !name(p).starts_with("mmproj")).collect();
    models.sort_by_key(|p| name(p));
    let mut notes = Vec::new();
    if models.len() > 1 {
        notes.push(format!("{} model files found; using {}. Keep only the one you want in the folder.", models.len(), name(models[0])));
    }
    let model = models.first().map(|p| (*p).clone());
    let model_bytes = model.as_ref().and_then(|m| std::fs::metadata(m).ok()).map(|m| m.len()).unwrap_or(0);
    if model.is_some() && model_bytes < 50_000_000 {
        notes.push("The model file is under 50 MB, which is too small for a real model: the copy may be incomplete.".to_string());
    }
    Setup { dir: dir.to_path_buf(), server: server.cloned(), model, model_bytes, notes }
}

// ------------------------------------------------------------------------------------------ the loopback HTTP

/// One request and its reply over loopback. HTTP/1.0 with "Connection: close", so the reply is never chunked by the server;
/// a chunked reply is read anyway.
pub fn http(port: u16, method: &str, path: &str, body: Option<&str>, timeout: Duration) -> Result<(u16, String)> {
    let addr = SocketAddr::from((Ipv4Addr::LOCALHOST, port));
    let mut s = TcpStream::connect_timeout(&addr, Duration::from_secs(3)).with_context(|| format!("The model program does not answer on port {}.", port))?;
    s.set_read_timeout(Some(timeout)).ok();
    s.set_write_timeout(Some(Duration::from_secs(10))).ok();
    let body = body.unwrap_or("");
    let req = format!(
        "{} {} HTTP/1.0\r\nHost: 127.0.0.1\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{}",
        method,
        path,
        body.len(),
        body
    );
    s.write_all(req.as_bytes()).context("The question could not be sent to the model program.")?;
    let mut raw = Vec::new();
    s.read_to_end(&mut raw).context("The model program stopped answering (it may have run out of memory or taken too long).")?;
    parse_response(&raw)
}

/// Status code and body of a raw HTTP reply.
pub fn parse_response(raw: &[u8]) -> Result<(u16, String)> {
    let split = raw.windows(4).position(|w| w == b"\r\n\r\n").ok_or_else(|| anyhow!("The model program's reply was cut short."))?;
    let head = String::from_utf8_lossy(&raw[..split]).to_string();
    let body = &raw[split + 4..];
    let status: u16 = head.lines().next().and_then(|l| l.split_whitespace().nth(1)).and_then(|c| c.parse().ok()).ok_or_else(|| anyhow!("The model program's reply had no status."))?;
    let chunked = head.lines().any(|l| l.to_lowercase().starts_with("transfer-encoding") && l.to_lowercase().contains("chunked"));
    let body = if chunked { dechunk(body)? } else { body.to_vec() };
    Ok((status, String::from_utf8_lossy(&body).to_string()))
}

fn dechunk(mut b: &[u8]) -> Result<Vec<u8>> {
    let mut out = Vec::new();
    loop {
        let eol = b.windows(2).position(|w| w == b"\r\n").ok_or_else(|| anyhow!("The model program's reply was cut short."))?;
        let size_text = String::from_utf8_lossy(&b[..eol]).to_string();
        let size = usize::from_str_radix(size_text.split(';').next().unwrap_or("").trim(), 16).map_err(|_| anyhow!("The model program's reply was not understood."))?;
        b = &b[eol + 2..];
        if size == 0 {
            return Ok(out);
        }
        if b.len() < size {
            bail!("The model program's reply was cut short.");
        }
        out.extend_from_slice(&b[..size]);
        b = &b[size..];
        b = b.strip_prefix(b"\r\n").unwrap_or(b);
    }
}

/// The text of the model's reply from an OpenAI-style chat reply (`choices[0].message.content`).
pub fn chat_reply_text(body: &str) -> Result<String> {
    let v: serde_json::Value = serde_json::from_str(body).map_err(|_| anyhow!("The model program's reply was not understood."))?;
    if let Some(e) = v.get("error") {
        let msg = e.get("message").and_then(|m| m.as_str()).unwrap_or("no reason given");
        bail!("The model program refused the question: {}", msg);
    }
    v.pointer("/choices/0/message/content").and_then(|c| c.as_str()).map(|s| s.to_string()).ok_or_else(|| anyhow!("The model program's reply held no answer."))
}

/// The request body of one chat question.
pub fn chat_request(system: &str, user: &str, max_tokens: u32) -> String {
    serde_json::json!({
        "messages": [
            {"role": "system", "content": system},
            {"role": "user", "content": user},
        ],
        "temperature": 0.1,
        "max_tokens": max_tokens,
        "stream": false,
    })
    .to_string()
}

// ------------------------------------------------------------------------------------------ the running model

/// A started `llama-server`. Dropping it stops the program.
pub struct ModelServer {
    child: Child,
    pub port: u16,
}

fn free_port() -> Result<u16> {
    let l = TcpListener::bind((Ipv4Addr::LOCALHOST, 0)).context("No free port on this PC for the model program.")?;
    Ok(l.local_addr()?.port())
}

impl ModelServer {
    /// Starts the program on a free loopback port and waits until the model is loaded.
    pub fn start(setup: &Setup) -> Result<ModelServer> {
        let server = setup.server.as_ref().ok_or_else(|| anyhow!("The model program (llama-server) is not in the assistant folder."))?;
        let model = setup.model.as_ref().ok_or_else(|| anyhow!("No model file (.gguf) is in the assistant folder."))?;
        let port = free_port()?;
        let log_path = setup.dir.join("model-server.log");
        let log = std::fs::File::create(&log_path).ok();
        let mut cmd = Command::new(server);
        if let Some(parent) = server.parent() {
            // the program's own libraries sit next to it
            cmd.current_dir(parent);
        }
        cmd.arg("-m").arg(model).args(["--host", "127.0.0.1", "--port"]).arg(port.to_string()).args(["-c", "4096"]);
        cmd.stdin(Stdio::null()).stdout(Stdio::null());
        match log {
            Some(f) => cmd.stderr(Stdio::from(f)),
            None => cmd.stderr(Stdio::null()),
        };
        #[cfg(windows)]
        {
            use std::os::windows::process::CommandExt;
            cmd.creation_flags(0x0800_0000); // CREATE_NO_WINDOW: no black console window
        }
        let child = cmd.spawn().with_context(|| format!("The model program could not be started ({}).", server.display()))?;
        let mut me = ModelServer { child, port };
        me.wait_ready(LOAD_TIMEOUT, &log_path)?;
        Ok(me)
    }

    fn wait_ready(&mut self, timeout: Duration, log_path: &Path) -> Result<()> {
        let started = Instant::now();
        loop {
            if let Ok(Some(status)) = self.child.try_wait() {
                let tail = std::fs::read_to_string(log_path).ok().map(|t| t.lines().rev().take(3).collect::<Vec<_>>().into_iter().rev().collect::<Vec<_>>().join(" | ")).unwrap_or_default();
                bail!("The model program stopped while loading ({}). The model may be too big for this PC's memory, or not a model it can read. Last lines of model-server.log: {}", status, tail);
            }
            if let Ok((200, _)) = http(self.port, "GET", "/health", None, Duration::from_secs(3)) {
                return Ok(());
            }
            if started.elapsed() > timeout {
                bail!("The model did not finish loading in {} minutes.", timeout.as_secs() / 60);
            }
            std::thread::sleep(Duration::from_millis(500));
        }
    }

    /// One question to the model; its words.
    pub fn chat(&self, system: &str, user: &str, max_tokens: u32) -> Result<String> {
        let (status, body) = http(self.port, "POST", "/v1/chat/completions", Some(&chat_request(system, user, max_tokens)), ANSWER_TIMEOUT)?;
        if status != 200 {
            let detail = chat_reply_text(&body).err().map(|e| e.to_string()).unwrap_or_default();
            bail!("The model program answered with status {}. {}", status, detail);
        }
        chat_reply_text(&body)
    }
}

impl Drop for ModelServer {
    fn drop(&mut self) {
        let _ = self.child.kill();
        let _ = self.child.wait();
    }
}
