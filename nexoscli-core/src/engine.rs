use serde_json::{json, Value};
use std::{fs, path::PathBuf};
use time::{OffsetDateTime, format_description::well_known::Rfc3339};

pub trait WorkflowEngine {
    fn start(&self, wf: &str, key: &str, attrs: Value, input: Value) -> anyhow::Result<()>;
    fn signal(&self, wf: &str, key: &str, name: &str, attrs: Value, payload: Value) -> anyhow::Result<()>;
    fn query (&self, wf: &str, key: &str, name: &str, args: Value) -> anyhow::Result<Value>;
}

#[derive(Clone, Copy, Debug)]
pub enum EngineKind { Local, TemporalStub }

pub struct LocalEngine;
impl WorkflowEngine for LocalEngine {
    fn start(&self, _wf: &str, _key: &str, _attrs: Value, _input: Value) -> anyhow::Result<()> { Ok(()) }
    fn signal(&self, _wf: &str, _key: &str, _name: &str, _attrs: Value, _payload: Value) -> anyhow::Result<()> { Ok(()) }
    fn query (&self, _wf: &str, _key: &str, _name: &str, _args: Value) -> anyhow::Result<Value> { Ok(Value::Null) }
}

pub struct TemporalOutboxEngine;
impl WorkflowEngine for TemporalOutboxEngine {
    fn start(&self, wf: &str, key: &str, attrs: Value, input: Value) -> anyhow::Result<()> {
        write_outbox("start", wf, key, None, attrs, input)
    }
    fn signal(&self, wf: &str, key: &str, name: &str, attrs: Value, payload: Value) -> anyhow::Result<()> {
        write_outbox("signal", wf, key, Some(name), attrs, payload)
    }
    fn query (&self, wf: &str, key: &str, name: &str, args: Value) -> anyhow::Result<Value> {
        write_outbox("query", wf, key, Some(name), json!({}), args)?;
        Ok(json!({"queued": true}))
    }
}

fn write_outbox(op: &str, wf: &str, key: &str, name: Option<&str>, attrs: Value, body: Value) -> anyhow::Result<()> {
    let now = OffsetDateTime::now_utc().format(&Rfc3339)?;
    let dir = PathBuf::from(".nexoscli").join("engine").join("outbox");
    fs::create_dir_all(&dir)?;
    let fname = format!("{}-{}-{}-{}.json",
        now.replace([':', '-','T','Z'], "_"),
        op, wf, sanitize(key)
    );
    let env = json!({
      "op": op, "workflow": wf, "key": key, "name": name,
      "attrs": attrs, "body": body,
      "ts": now
    });
    fs::write(dir.join(fname), serde_json::to_vec_pretty(&env)?)?;
    Ok(())
}

fn sanitize(s: &str) -> String {
    s.chars().map(|c| if c.is_ascii_alphanumeric() { c } else { '_' }).collect()
}

pub fn engine_from(kind: EngineKind) -> Box<dyn WorkflowEngine + Send + Sync> {
    match kind {
        EngineKind::Local => Box::new(LocalEngine),
        EngineKind::TemporalStub => Box::new(TemporalOutboxEngine),
    }
}
