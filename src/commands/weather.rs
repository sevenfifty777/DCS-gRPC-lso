//! `lso.exe weather`: one-shot dump of the raw mission weather and time as LSO reads them.
//!
//! Used for the live smoke test of the `CustomService.Eval` weather source and to record the raw
//! values of each calibration mission (`docs/CASE_RECOVERY_DETECTION_PLAN_2026-09-26.md`,
//! section 6). Read-only towards DCS: it runs the fixed chunk of `crate::mission_weather` and
//! prints JSON on stdout. With `--output`, the same JSON is also written to a new file (never
//! overwriting one), because log lines share stdout and would otherwise mix with it.
//!
//! With `--carrier <unit name>`, the carrier's position is read (`GetTransform`) and the
//! predicted case is added (`crate::recovery_case`), so a calibration run records LSO's
//! prediction before the pilot checks in with Marshal. Without it, "dark" cannot be computed
//! and the prediction is usually `indeterminate`.

use std::io::Write;
use std::path::PathBuf;
use std::time::{SystemTime, UNIX_EPOCH};

use serde::Serialize;
use tonic::transport::Uri;

use crate::client::{CustomClient, UnitClient};
use crate::mission_weather::{
    query_mission_weather, MissionWeatherQuery, MISSION_WEATHER_SNIPPET_VERSION,
};
use crate::recovery_case::{assess_recovery_case, CaseAssessment};

#[derive(clap::Parser)]
pub struct Opts {
    /// The URI of DCS-gRPC.
    #[clap(long, default_value = "http://127.0.0.1:50051")]
    uri: Uri,

    /// Environment variable containing the optional DCS-gRPC X-API-Key token.
    #[clap(long, default_value = "DCS_GRPC_API_KEY")]
    api_key_env: String,

    /// Also include the unparsed JSON reply of the Lua chunk.
    #[clap(long)]
    raw: bool,

    /// Also write the JSON to this new file (refused if it already exists).
    #[clap(long)]
    output: Option<PathBuf>,

    /// DCS unit name of the carrier, to predict the ordered case at its position.
    #[clap(long)]
    carrier: Option<String>,
}

#[derive(Serialize)]
struct WeatherDump<'a> {
    snippet_version: u32,
    /// Wall-clock time of the query on the LSO host, Unix milliseconds.
    captured_unix_ms: u64,
    query: &'a MissionWeatherQuery,
    #[serde(skip_serializing_if = "Option::is_none")]
    carrier: Option<&'a str>,
    recovery_case: CaseAssessment,
    #[serde(skip_serializing_if = "Option::is_none")]
    raw_reply: Option<&'a str>,
}

pub async fn execute(opts: Opts) -> Result<(), crate::error::Error> {
    let channel = crate::client::connect_authenticated(opts.uri, &opts.api_key_env).await?;
    let mut client = CustomClient::new(channel.clone());
    let (query, raw_reply) = query_mission_weather(&mut client).await;
    let carrier_lat_lon = match &opts.carrier {
        Some(name) => match UnitClient::new(channel).get_transform(name.as_str()).await {
            Ok(transform) => Some((transform.lat, transform.lon)),
            Err(err) => {
                tracing::warn!(carrier = %name, ?err, "carrier position unavailable");
                None
            }
        },
        None => None,
    };
    let recovery_case = assess_recovery_case(&query, carrier_lat_lon);
    let captured_unix_ms = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_millis()
        .min(u64::MAX as u128) as u64;
    let dump = WeatherDump {
        snippet_version: MISSION_WEATHER_SNIPPET_VERSION,
        captured_unix_ms,
        query: &query,
        carrier: opts.carrier.as_deref(),
        recovery_case,
        raw_reply: if opts.raw { raw_reply.as_deref() } else { None },
    };
    let json = serde_json::to_string_pretty(&dump)?;
    if let Some(path) = &opts.output {
        let mut file = std::fs::OpenOptions::new()
            .write(true)
            .create_new(true)
            .open(path)
            .map_err(|err| crate::error::Error::file_at(path, err))?;
        file.write_all(json.as_bytes())
            .and_then(|()| file.write_all(b"\n"))
            .and_then(|()| file.sync_all())
            .map_err(|err| crate::error::Error::file_at(path, err))?;
        tracing::info!(path = %path.display(), "mission weather written");
    }
    println!("{json}");
    Ok(())
}
