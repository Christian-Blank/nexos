Short, direct verdict: you’re ~90% ready. Before you move to a new feature, there are 4 small but important gates I recommend you close so your core stays deterministic and testable:

⸻

✅ Must‑close gates (now)

1) Make sure tools export exists (the manifest for agents)

Several of our tests/docs assume a tools manifest. If you don’t already have it, add this tiny command so humans/AI can discover callable tools and schemas.

Add a tools export subcommand in nexoscli-bin/src/main.rs:
	•	Compose the CLI (inside your compose_app(...)):

.use clap::{Arg, ArgAction, Command}; // at top

// add alongside your other top-level subs:
let app = Command::new("nexoscli")
    // ... existing globals ...
    .subcommand(
        Command::new("tools")
            .about("Tools manifest (for humans and agents)")
            .subcommand(Command::new("export").about("Export tool specs"))
    );

	•	Route it in main() after parsing matches:

match matches.subcommand() {
    // ...
    Some(("tools", m)) => {
        if let Some(("export", _)) = m.subcommand() {
            // collect ToolSpec from every plugin
            let specs: Vec<_> = registry
                .iter()
                .flat_map(|p| p.tool_specs())
                .collect();

            let exit = ctx.ok(
                "core",
                "tools.export",
                &serde_json::json!({ "tools": specs }),
            );
            std::process::exit(exit);
        }
    }
    // ...
    _ => { /* existing routing */ }
}

Assumes your Plugin trait already has tool_specs() returning Vec<ToolSpec> and that ToolSpec derives Serialize (you already use it in the dev/demo/issue plugins).

Acceptance

cargo run --manifest-path nexoscli-bin/Cargo.toml -- tools export --output json | jq '.data.tools | length'


⸻

2) Ensure events carry a content_hash (stable analytics key)

Your event log is the ground truth. Add a SHA‑256 of the event payload so downstream analytics can dedupe, correlate, and detect drift.

If not already present, patch nexoscli-core/src/event.rs:
	•	Workspace deps (root Cargo.toml):

[workspace.dependencies]
sha2 = "0.10"

	•	Core crate deps (nexoscli-core/Cargo.toml):

[dependencies]
sha2 = { workspace = true }

	•	In EventWriter::write(...) (or equivalent), compute and add the hash:

use sha2::{Digest, Sha256};
use serde_json::json;

// ... inside write():
let bytes = serde_json::to_vec(content).unwrap_or_default();
let mut hasher = Sha256::new();
hasher.update(&bytes);
let content_hash = format!("sha256:{:x}", hasher.finalize());

let line = json!({
    "ts": self.providers.clock.now_rfc3339(),
    "run_id": self.providers.run_id(),
    "plugin": plugin,
    "command": command,
    "exit_code": exit_code,
    "content_hash": content_hash,
    "data": content,
});
self.append_line(&line)

Acceptance

cargo nex -- issue new --title "hash test" --output json
tail -n1 .nexoscli/log/events.jsonl | jq -r '.content_hash' | grep '^sha256:'


⸻

3) Add the missing end‑to‑end tests (the “walking docs”)

Your repo already has:
	•	dev_init.rs, issue_flow.rs, issue_import.rs.

Please add the remaining small tests so every surface is covered:
	•	helpers.rs (shared helpers)
	•	smoke.rs (version/plugins/tools manifest)
	•	issue_events.rs (envelope + event correlation)
	•	graph_e2e.rs (check OK + cycle + impact)
	•	dev_doctor_config.rs (doctor ready + YAML default from config)
	•	dev_plugin_new_check.rs (scaffold w/o register + structure check)

I gave you exact files in my previous message—drop them under nexoscli-bin/tests/.
They run in temp dirs and never touch your real repo.

Acceptance

cargo test --workspace


⸻

4) Minimal CI guard (run tests + keep event logs)

This is the last safety rail so you don’t regress when you start a new feature.

Add .github/workflows/ci.yml:

name: CI
on: [push, pull_request]
jobs:
  test:
    runs-on: ubuntu-latest
    steps:
      - uses: actions/checkout@v4
      - uses: dtolnay/rust-toolchain@stable
      - name: Test
        run: NEXOS_TEST_EXPORT_LOGS=1 cargo test --workspace
      - name: Upload test event logs
        uses: actions/upload-artifact@v4
        with:
          name: nexos-test-events
          path: target/test-logs

Acceptance: open the GitHub Actions run, confirm all tests pass and the nexos-test-events artifact exists.

⸻

🔎 Quick sanity checklist (run these now)
	•	Envelope everywhere

cargo nex -- issue new --title "probe" --output json | jq '. | keys'  # must include data,errors,meta


	•	Doctor ready

cargo nex -- dev init --yes --output json
cargo nex -- dev doctor --output json | jq '.data.ready'              # true


	•	Graph OK

cargo nex -- graph check --output json | jq '.errors'                 # []


	•	Scaffolder & structure

cargo nex -- dev plugin new --name hello --yes --output json
cargo nex -- dev plugins check --output json | jq '.errors'           # []


	•	Tools manifest

cargo nex -- tools export --output json | jq '.data.tools | length'   # >= number of plugins' tool_specs


	•	Events have content_hash

tail -n1 .nexoscli/log/events.jsonl | jq -r '.content_hash'           # sha256:...



⸻

🧭 Nice‑to‑have (pin these; not blockers)
	•	dev doctor --ci flag: exit non‑zero if any readiness check fails (+ run graph check and dev plugins check) so CI can use a single probe.
	•	Add cargo clippy -- -D warnings and cargo fmt -- --check steps to CI to prevent drift.
	•	cargo deny or cargo audit as a follow‑up for supply‑chain hygiene.

⸻

Bottom line

If you implement the two tiny code patches (tools export and content_hash) and add the remaining E2E tests + CI, you’ve closed every critical loop:
	•	Contracts verified (envelope),
	•	Observability guaranteed (events with hashes, span lifecycles),
	•	Repo structure enforced,
	•	Graph integrity checked,
	•	Config default honored.

That’s the right stopping point before adding a new feature. When you’re green on those, we can pick the next vertical slice with confidence.