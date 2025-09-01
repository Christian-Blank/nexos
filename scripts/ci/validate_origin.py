#!/usr/bin/env python3
import sys, pathlib, json, yaml, re

ROOT = pathlib.Path(__file__).resolve().parents[2]
errs = []

def must_exist(path):
    p = ROOT / path
    if not p.exists():
        errs.append(f"Missing required file: {path}")
    return p

# Required files
ns = must_exist("docs/01-north-star.md")
pb = must_exist("docs/00-project-brief.md")
eg = must_exist("docs/exec_graph.yaml")
adr = must_exist("docs/ADRs/0001-cli-contracts.md")
play = must_exist("docs/PLAYBOOK.md")

# Exec graph must have at least one feature + one issue
if eg.exists():
    # Handle template variables in YAML before parsing
    yaml_content = eg.read_text(encoding="utf-8")
    # Replace common template variables
    import datetime
    today = datetime.date.today().strftime("%Y%m%d")
    yaml_content = yaml_content.replace("${TODAY}", today)
    
    data = yaml.safe_load(yaml_content) or {}
    nodes = data.get("nodes", [])
    ids = [n.get("id") for n in nodes]
    types = [n.get("type") for n in nodes]
    if not any(t == "feature" for t in types):
        errs.append("exec_graph.yaml: no 'feature' node present")
    if not any(t == "issue" for t in types):
        errs.append("exec_graph.yaml: no 'issue' node present")
    # Light ID sanity
    id_re = re.compile(r"^[A-Z][A-Z0-9.-]*[a-z0-9-]*")
    for nid in ids:
        if not nid:
            errs.append("exec_graph.yaml: node with missing id")
        elif not id_re.match(nid):
            errs.append(f"exec_graph.yaml: suspicious id '{nid}'")

print(json.dumps({"passed": not errs, "errors": errs}, indent=2))
sys.exit(1 if errs else 0)
