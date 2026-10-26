mod client;
mod cloud_presets;
mod commands;
mod data;
mod db;
mod draw;
mod error;
mod flown_approach;
mod grading;
mod logging;
mod lso_notation;
mod metrics;
mod mission_weather;
mod recovery_case;
mod tasks;
mod telemetry;
#[cfg(test)]
mod tests;
mod track;
mod transform;
mod utils;

use clap::{ArgAction, Parser};
use tracing_subscriber::layer::SubscriberExt;
use tracing_subscriber::util::SubscriberInitExt;
use tracing_subscriber::{filter, fmt};
use utils::shutdown::Shutdown;

#[derive(clap::Parser)]
#[clap(version = env!("CARGO_PKG_VERSION"))]
struct Opts {
    /// A level of verbosity, and can be used multiple times
    #[clap(short, long, action = ArgAction::Count)]
    verbose: u8,
    /// Enable colorized output
    #[clap(long)]
    color: bool,
    /// Also write the log to one UTF-8 file per UTC day in this directory
    /// (`lso.YYYY-MM-DD.log`), created if needed, keeping the newest `--log-max-files` files.
    /// The console output is unchanged. Global flag: goes before the subcommand.
    #[clap(long, value_name = "DIR")]
    log_dir: Option<std::path::PathBuf>,
    /// Number of daily log files kept in `--log-dir`; the oldest are deleted at rotation.
    #[clap(long, default_value_t = logging::DEFAULT_MAX_LOG_FILES, value_name = "N")]
    log_max_files: usize,
    #[clap(subcommand)]
    command: Command,
}

#[derive(clap::Parser)]
enum Command {
    /// Connect to DCS-gRPC to track carrier recoveries.
    Run(Box<commands::run::Opts>),

    /// Extract carrier recoveries from ACMI recordings (must be recordings created by the LSO;
    /// recordings directly from TacView will not work).
    File(commands::file::Opts),

    /// Offline diagnostic: replay already-recorded JSON reports with an artificially reduced
    /// pre-groove sampling cadence and compare the resulting gates/grade to the full cadence
    /// actually recorded. Never affects live recording or the fork.
    CadenceAb(commands::cadence_ab::Opts),

    /// Offline diagnostic: compare recorded groove entry with the current Case I roll-out detector.
    /// Reads schema-v3 JSON reports without modifying them.
    GrooveAb(commands::groove_ab::Opts),

    /// PROTOTYPE offline diagnostic: re-grade recorded JSON reports under the candidate
    /// CATOBAR grading policies (see `CatobarGradingPolicy`) and print a Markdown table.
    /// Reads reports without modifying them; never affects live grading.
    GradeAb(commands::grade_ab::Opts),

    /// Diagnostic: print the raw mission weather and time LSO reads through
    /// `CustomService.Eval` (requires `evalEnabled = true` on the server). Read-only.
    Weather(commands::weather::Opts),

    /// Maintenance: regenerate `data/dcs_cloud_presets.json` from a DCS install's
    /// `Config/Effects/clouds.lua`. Offline; never talks to DCS-gRPC.
    CloudPresets(commands::cloud_presets::Opts),
}

#[tokio::main]
async fn main() {
    let opts: Opts = Opts::parse();
    let max_level = match opts.verbose {
        0 => tracing::Level::INFO,
        1 => tracing::Level::DEBUG,
        _ => tracing::Level::TRACE,
    };
    // The rolling file writer is opened before the subscriber exists, so a bad `--log-dir` is
    // reported on stderr and stops the process instead of logging to nowhere.
    let (file_layer, file_guard) = match opts
        .log_dir
        .as_deref()
        .map(|dir| logging::rolling_writer(dir, opts.log_max_files))
    {
        None => (None, None),
        Some(Ok((writer, guard))) => (
            Some(fmt::layer().with_ansi(false).with_writer(writer)),
            Some(guard),
        ),
        Some(Err(err)) => {
            eprintln!("LSO cannot start: {err}");
            std::process::exit(2);
        }
    };
    tracing_subscriber::registry()
        .with(filter::filter_fn(move |m| {
            m.target().starts_with("lso") && m.level() <= &max_level
        }))
        .with(fmt::layer().with_ansi(opts.color))
        .with(file_layer)
        .init();
    if let Some(dir) = &opts.log_dir {
        tracing::info!(
            log_dir = %dir.display(),
            max_files = opts.log_max_files,
            "daily log file rotation enabled"
        );
    }

    // shutdown gracefully on CTRL+C
    let shutdown = Shutdown::new();
    let shutdown_handle = shutdown.handle();
    tokio::task::spawn(async {
        match tokio::signal::ctrl_c().await {
            Ok(()) => shutdown.shutdown().await,
            Err(err) => tracing::error!(?err, "failed to install Ctrl-C handler"),
        }
    });

    let result = match opts.command {
        Command::Run(opts) => commands::run::execute(*opts, shutdown_handle).await,
        Command::File(opts) => commands::file::execute(opts),
        Command::CadenceAb(opts) => commands::cadence_ab::execute(opts),
        Command::GrooveAb(opts) => commands::groove_ab::execute(opts),
        Command::GradeAb(opts) => commands::grade_ab::execute(opts),
        Command::Weather(opts) => commands::weather::execute(opts).await,
        Command::CloudPresets(opts) => commands::cloud_presets::execute(opts),
    };
    let failed = if let Err(err) = result {
        tracing::error!(error = %err, error_chain = ?err, "LSO terminated with an error");
        true
    } else {
        false
    };
    // `process::exit` runs no destructors: flush the file writer's queue explicitly first.
    drop(file_guard);
    if failed {
        std::process::exit(1);
    }
}
