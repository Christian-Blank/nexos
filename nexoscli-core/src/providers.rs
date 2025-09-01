use std::fs::OpenOptions;
use std::io::Write;
use std::path::PathBuf;
use std::sync::Arc;
use time::{format_description::well_known::Rfc3339, OffsetDateTime};
use uuid::Uuid;

#[derive(Clone)]
pub struct Clock;
impl Clock {
    pub fn now_rfc3339() -> String {
        OffsetDateTime::now_utc().format(&Rfc3339).unwrap()
    }
}

#[derive(Clone)]
pub struct Entropy;
impl Entropy {
    pub fn uuid_v4() -> String {
        Uuid::new_v4().to_string()
    }
}

#[derive(Clone)]
pub struct Io;

impl Io {
    pub fn ensure_parent(path: &PathBuf) -> std::io::Result<()> {
        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent)?;
        }
        Ok(())
    }

    pub fn append_line(path: &PathBuf, line: &str) -> std::io::Result<()> {
        Self::ensure_parent(path)?;
        let mut f = OpenOptions::new().create(true).append(true).open(path)?;
        f.write_all(line.as_bytes())?;
        f.write_all(b"\n")?;
        Ok(())
    }
}

#[derive(Clone)]
pub struct Providers {
    pub run_id: String,
    pub event_log_path: PathBuf,
}

impl Providers {
    pub fn new(cwd: PathBuf) -> Self {
        let run = std::env::var("NEXOS_RUN_ID").unwrap_or_else(|_| Entropy::uuid_v4());
        let path = cwd.join(".nexoscli").join("log").join("events.jsonl");
        Self { run_id: run, event_log_path: path }
    }
    pub fn arc(self) -> Arc<Self> { Arc::new(self) }
}
