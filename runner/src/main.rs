mod setup_wkspc;

const RESULTS_DIR: &str = "results/";
const RESEARCH_WORKSPACE_DIR: &str = "research-workspace/";

fn run() -> Result<(), failure::Error> {
    let matches = clap::App::new("runner")
        .subcommand(crate::setup_wkspc::cli_options())
        .setting(clap::AppSettings::SubcommandRequiredElseHelp)
        .setting(clap::AppSettings::DisableVersion)
        .get_matches();

    match matches.subcommand() {
        ("setup_wkspc", Some(sub_m)) => crate::setup_wkspc::run(sub_m),
        _ => {
            unreachable!();
        }
    }
}

fn main() {
    use console::style;

    env_logger::init();

    std::env::set_var("RUST_BACKTRACE", "1");

    // If an error returned, try to print something helpful
    if let Err(err) = run() {
        const MESSAGE: &str = r#"== ERROR ==================================================================================
`runner` encountered an error. The command log above may offer clues. If the error pertains to SSH,
you may be able to get useful information by setting the RUST_LOG=debug environment variable. It is
recommended that you use `debug` builds of `runner`, rather than `release`, as the performance of
`runner` is not that important and is almost always dominated by the experiment being run.
"#;

        println!("{}", style(MESSAGE).red().bold());

        // Errors from SSH commands
        if err.downcast_ref::<spurs::SshError>().is_some() {
            println!("An error occurred while attempting to run a command over SSH");
        }

        // Print error and backtrace
        println!(
            "`runner` encountered the following error:\n{}\n{}",
            err.as_fail(),
            err.backtrace(),
        );

        std::process::exit(101);

    }
}
