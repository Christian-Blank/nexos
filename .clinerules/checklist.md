# Cline Definition of Ready / Done

**Ready**
- [ ] Problem ≤ 3 sentences
- [ ] DRI named
- [ ] Scope label chosen (matches .clinerules/scope.json)
- [ ] ≤ 8 binary acceptance criteria
- [ ] Exec graph node referenced (or to be added)

**Done**
- [ ] E2E passes; JSON envelopes stable
- [ ] Events present; spans where applicable
- [ ] `cargo test --workspace` green
- [ ] `nexoscli dev doctor --ci` green
- [ ] Docs/READMEs updated (short, link to specs)
- [ ] Cline rules updated if surface changed
- [ ] Task bundle created (`bundle.tgz`)
- [ ] Conventional commit includes Task ID (and ADR ID if contracts changed)