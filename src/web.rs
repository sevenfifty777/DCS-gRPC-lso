use axum::{extract::State, response::Html, routing::get, Json, Router};
use tokio::net::TcpListener;

use crate::db::{SharedDb, StoredPass};

#[derive(Clone)]
struct AppState {
    db: SharedDb,
}

/// Serve the LSO web greenie board on `0.0.0.0:<port>`.
///
/// The server runs until an OS-level error occurs (e.g. port in use).
/// It is intended to be spawned as a background tokio task.
pub async fn serve(db: SharedDb, port: u16) -> std::io::Result<()> {
    let state = AppState { db };
    let app = Router::new()
        .route("/", get(handler_html))
        .route("/api/passes", get(handler_passes))
        .with_state(state);

    let addr = format!("0.0.0.0:{}", port);
    tracing::info!(addr = %addr, "LSO web dashboard listening");
    let listener = TcpListener::bind(&addr).await?;
    axum::serve(listener, app).await
}

async fn handler_html() -> Html<&'static str> {
    Html(DASHBOARD_HTML)
}

async fn handler_passes(State(state): State<AppState>) -> Json<Vec<StoredPass>> {
    let passes = tokio::task::spawn_blocking(move || state.db.all_passes())
        .await
        .unwrap_or_else(|_| Ok(vec![]))
        .unwrap_or_default();
    Json(passes)
}

const DASHBOARD_HTML: &str = r#"<!DOCTYPE html>
<html lang="en">
<head>
  <meta charset="UTF-8">
  <meta name="viewport" content="width=device-width, initial-scale=1.0">
  <title>LSO Greenie Board</title>
  <style>
    *, *::before, *::after { box-sizing: border-box; }
    body  { background: #111827; color: #f3f4f6; font-family: 'Courier New', monospace; margin: 0; padding: 1.5rem; }
    h1   { color: #60a5fa; margin: 0 0 1rem; font-size: 1.4rem; letter-spacing: .05em; }
    table { border-collapse: collapse; width: 100%; font-size: .9rem; }
    thead th { background: #1f2937; color: #9ca3af; padding: .5rem .75rem; text-align: left; border-bottom: 2px solid #374151; white-space: nowrap; }
    tbody td { padding: .4rem .75rem; border-bottom: 1px solid #1f2937; }
    tbody tr:hover td { background: #1f2937; }
    .g-UNI { color: #facc15; font-weight: bold; text-shadow: 0 0 6px #fbbf24; }
    .g-OK  { color: #4ade80; font-weight: bold; }
    .g-OKP { color: #86efac; }
    .g-NG  { color: #fde68a; font-weight: bold; }
    .g-Cut { color: #f87171; font-weight: bold; }
    .g-B   { color: #9ca3af; }
    .g-WO  { color: #9ca3af; }
    .empty { color: #4b5563; padding: 1rem .75rem; }
    .lso-notes { color: #d1d5db; font-size: .8rem; max-width: 28rem; white-space: normal; }
    .pts   { color: #6b7280; font-size: .8rem; }
    .esf   { color: #a78bfa; font-size: .85rem; }
    .gdate { color: #93c5fd; font-size: .8rem; white-space: nowrap; }
    #status { margin-top: .75rem; color: #4b5563; font-size: .75rem; }
  </style>
</head>
<body>
  <h1>&#x2708;&#xFE0F; LSO Greenie Board</h1>
  <table>
    <thead>
      <tr><th>#</th><th>Timestamp</th><th>Grade Date</th><th>Mission Time</th><th>Pilot</th><th>Aircraft</th><th>Map</th><th>Grade</th><th>Pts</th><th>Wire/Spot</th><th>DCS Grade</th><th>LSO Notes</th></tr>
    </thead>
    <tbody id="rows"><tr><td class="empty" colspan="12">Loading&#x2026;</td></tr></tbody>
  </table>
  <div id="status"></div>
  <script>
    function esc(v) {
      if (v == null) return '-';
      return String(v)
        .replace(/&/g, '&amp;')
        .replace(/</g, '&lt;')
        .replace(/>/g, '&gt;')
        .replace(/"/g, '&quot;');
    }
    // Map NAVAIR grade labels → CSS suffix (matches .g-* classes above).
    function gradeClass(g) {
      return ({'_OK_':'UNI','OK':'OK','(OK)':'OKP','--':'NG','C':'Cut','B':'B','WO':'WO'})[g] || '';
    }
    // NAVAIR points table — used client-side when the server field is absent.
    function gradePoints(g) {
      return ({'_OK_':5.0,'OK':4.0,'(OK)':3.0,'--':2.0,'C':0.0,'B':2.5,'WO':1.0})[g];
    }
    async function refresh() {
      try {
        const resp = await fetch('/api/passes');
        if (!resp.ok) throw new Error('HTTP ' + resp.status);
        const passes = await resp.json();
        const tbody = document.getElementById('rows');
        if (passes.length === 0) {
          tbody.innerHTML = '<tr><td class="empty" colspan="12">No passes recorded yet.</td></tr>';
        } else {
          tbody.innerHTML = passes.map((p, i) => {
            const n = passes.length - i;
            const gc = gradeClass(p.pass_grade);
            // Use server-provided grade_points; fall back to client-side table if absent.
            const pts = (p.grade_points !== undefined && p.grade_points !== null)
              ? p.grade_points
              : gradePoints(p.pass_grade);
            const ptsStr = pts !== undefined ? Number(pts).toFixed(p.spot != null ? 2 : 1) : '-';
            return '<tr>'
              + '<td>' + n + '</td>'
              + '<td>' + esc(p.timestamp) + '</td>'
              + '<td class="gdate">' + esc(p.grade_date) + '</td>'
              + '<td class="gdate">' + esc(p.mission_datetime) + '</td>'
              + '<td>' + esc(p.pilot_name) + '</td>'
              + '<td>' + esc(p.aircraft_type) + '</td>'
              + '<td>' + esc(p.map_name) + '</td>'
              + '<td class="g-' + gc + '">' + esc(p.pass_grade) + '</td>'
              + '<td class="pts">' + ptsStr + '</td>'
              + '<td>' + esc(p.spot != null ? p.spot : p.wire) + '</td>'
              + '<td>' + esc(p.dcs_grading) + '</td>'
              + '<td class="lso-notes">' + esc(p.lso_notes) + '</td>'
              + '</tr>';
          }).join('');
        }
        document.getElementById('status').textContent =
          'Updated: ' + new Date().toLocaleTimeString() + ' \u2014 ' + passes.length + ' pass(es)';
      } catch (err) {
        document.getElementById('status').textContent = 'Refresh error: ' + err.message;
      }
    }
    refresh();
    setInterval(refresh, 10000);
  </script>
</body>
</html>"#;
