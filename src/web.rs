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
    .g-OK  { color: #4ade80; font-weight: bold; }
    .g-OKP { color: #86efac; }
    .g-Fair { color: #fde68a; }
    .g-NG  { color: #fb923c; font-weight: bold; }
    .g-Cut { color: #f87171; font-weight: bold; }
    .g-B, .g-WO { color: #9ca3af; }
    .empty { color: #4b5563; padding: 1rem .75rem; }
    #status { margin-top: .75rem; color: #4b5563; font-size: .75rem; }
  </style>
</head>
<body>
  <h1>&#x2708;&#xFE0F; LSO Greenie Board</h1>
  <table>
    <thead>
      <tr><th>#</th><th>Timestamp</th><th>Pilot</th><th>Grade</th><th>Wire</th><th>DCS Grade</th></tr>
    </thead>
    <tbody id="rows"><tr><td class="empty" colspan="6">Loading&#x2026;</td></tr></tbody>
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
    function gradeClass(g) {
      return ({'OK':'OK','(OK)':'OKP','Fair':'Fair','NG':'NG','Cut':'Cut','B':'B','WO':'WO'})[g] || '';
    }
    async function refresh() {
      try {
        const resp = await fetch('/api/passes');
        if (!resp.ok) throw new Error('HTTP ' + resp.status);
        const passes = await resp.json();
        const tbody = document.getElementById('rows');
        if (passes.length === 0) {
          tbody.innerHTML = '<tr><td class="empty" colspan="6">No passes recorded yet.</td></tr>';
        } else {
          tbody.innerHTML = passes.map((p, i) => {
            const n = passes.length - i;
            const gc = gradeClass(p.pass_grade);
            return '<tr>'
              + '<td>' + n + '</td>'
              + '<td>' + esc(p.timestamp) + '</td>'
              + '<td>' + esc(p.pilot_name) + '</td>'
              + '<td class="g-' + gc + '">' + esc(p.pass_grade) + '</td>'
              + '<td>' + esc(p.wire) + '</td>'
              + '<td>' + esc(p.dcs_grading) + '</td>'
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
