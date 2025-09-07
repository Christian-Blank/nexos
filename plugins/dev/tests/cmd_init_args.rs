use clap::FromArgMatches;
use nexoscli_plugin_dev::cmd_init::{DevInitArgs, subcommand};

#[test]
fn parse_dev_init_args_flags() {
    // Simulate: dev init --dry-run --with-config
    let m = subcommand()
        .try_get_matches_from(vec!["init", "--dry-run", "--with-config"])
        .expect("arg parsing should succeed");
    let args = DevInitArgs::from_arg_matches(&m).expect("FromArgMatches");
    assert!(args.dry_run);
    assert!(args.with_config);
    assert!(!args.yes);
}
