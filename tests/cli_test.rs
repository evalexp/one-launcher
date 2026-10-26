use one_launcher::cli::{self, CliArgs};

#[test]
fn test_cli_args_parsing() {
    let args = vec![
        "-plugin".to_string(),
        "plugins/system".to_string(),
        "-input".to_string(),
        "sys: quit".to_string(),
        "-select".to_string(),
        "quit".to_string(),
        "-debug".to_string(),
        "-log-out".to_string(),
        "std".to_string(),
        "-format".to_string(),
        "json".to_string(),
    ];

    let cli = CliArgs::parse_from(&args);
    assert_eq!(cli.plugin.as_deref(), Some("plugins/system"));
    assert_eq!(cli.input.as_deref(), Some("sys: quit"));
    assert_eq!(cli.select.as_deref(), Some("quit"));
    assert!(cli.debug);
    assert_eq!(cli.log_out.as_deref(), Some("std"));
    assert_eq!(cli.format, "json");
    assert!(cli.is_cli_mode());
}

#[test]
fn test_cli_isolated_plugin_query_and_select() {
    let args = vec![
        "-plugin".to_string(),
        "plugins/system".to_string(),
        "-input".to_string(),
        "sys:".to_string(),
        "-log-level".to_string(),
        "silent".to_string(),
    ];

    let cli = CliArgs::parse_from(&args);
    let code = cli::run_cli(&cli);
    assert_eq!(code, 0);
}
