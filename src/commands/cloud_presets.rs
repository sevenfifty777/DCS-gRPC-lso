//! `lso.exe cloud-presets`: regenerate `data/dcs_cloud_presets.json` from a DCS install.
//!
//! Offline maintenance command, run after a DCS update that changes the cloud presets. It reads
//! `Config/Effects/clouds.lua` and the version from `autoupdate.cfg`, and writes the reduced table
//! that is embedded in the binary (`crate::cloud_presets`). It never talks to DCS-gRPC.

use std::path::PathBuf;

use crate::cloud_presets::parse_clouds_lua;

#[derive(clap::Parser)]
pub struct Opts {
    /// Root of the DCS World install.
    #[clap(long, default_value = r"C:\Program Files\Eagle Dynamics\DCS World")]
    dcs_root: PathBuf,

    /// File to write (replaced if it exists; the checked-in table is tracked by git).
    #[clap(long, default_value = "data/dcs_cloud_presets.json")]
    output: PathBuf,
}

pub fn execute(opts: Opts) -> Result<(), crate::error::Error> {
    let clouds_path = opts
        .dcs_root
        .join("Config")
        .join("Effects")
        .join("clouds.lua");
    let text = std::fs::read_to_string(&clouds_path)
        .map_err(|err| crate::error::Error::file_at(&clouds_path, err))?;
    let version = read_dcs_version(&opts.dcs_root);
    let table = parse_clouds_lua(&text, version.clone()).map_err(|err| {
        crate::error::Error::InvalidConfiguration(format!("{}: {err}", clouds_path.display()))
    })?;
    let json = serde_json::to_string_pretty(&table)?;
    std::fs::write(&opts.output, json + "\n")
        .map_err(|err| crate::error::Error::file_at(&opts.output, err))?;
    tracing::info!(
        presets = table.presets.len(),
        dcs_version = version.as_deref().unwrap_or("unknown"),
        output = %opts.output.display(),
        "cloud preset table written"
    );
    Ok(())
}

/// `version` field of `autoupdate.cfg` (a JSON file), if readable.
fn read_dcs_version(dcs_root: &std::path::Path) -> Option<String> {
    let text = std::fs::read_to_string(dcs_root.join("autoupdate.cfg")).ok()?;
    let value: serde_json::Value = serde_json::from_str(&text).ok()?;
    value.get("version")?.as_str().map(str::to_string)
}
