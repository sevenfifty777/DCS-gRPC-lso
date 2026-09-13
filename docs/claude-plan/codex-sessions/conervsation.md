# Analyze LSO recovery data gaps

> i want you to act and think as a rust , gRPC and DCS lua expert
> i need you to analyse all the code in each folder
> the new code are
> D:\\\_DCS\DCS\_gRPC\_analyse\DCS-gRPC-lso-new  lso application
> D:\\\_DCS\DCS\_gRPC\_analyse\rust-server-new  gRpc server 0.9.0
>
> the original code
> D:\\\_DCS\DCS\_gRPC\_analyse\DCS-gRPC-lso-origin
> D:\\\_DCS\DCS\_gRPC\_analyse\rust-server-origin
>
> we have developped lot of new code to improve lso applicatioN
> the goal is to get notation as close as possible to reality but wth DCS constraint
> we recently made massive update (yesterday) compare to commit from sunday in D:\\\_DCS\DCS\_gRPC\_analyse\DCS-gRPC-lso-new
> but this introduce lot of bugs
> i need you to compare the new code with original lso app
> also verify that new gRPC server is optomized and didnt introduce isue
> we are facing issue it seems to get live data without gap for the record recovery
>
> I need you to first analyse , then to point main issue mainly for groove part more in details , other issue should be listed but we will proceed step by step to fix every problem , avoiding massive update
>
> i need you to compare how recovery records was done on original lso app ( was really live or any extrapolation
>
> create document with all analyse and main issue and possible fix

<details><summary>14 previous messages</summary>

> I’ll treat this as a forensic comparison first, not a rewrite: trace the recovery-data pipeline end to end, compare original versus new behavior, then produce a prioritized Markdown report with evidence and small, staged fix options. I’m using the graph/codebase skill for repository-wide relationships, the DCS gRPC client skill for the Rust streaming path, and the DCS scripting skill for the Lua/server boundary.
>
> <details><summary>Ran 8 commands</summary>
>
> - Ran `rg -n -i "DCS-gRPC-lso|rust-server|recovery|groove|StreamUnits|0\.9\.0" "C:\Users\thierry\.codex\memories\MEMORY.md"`
>
> <details><summary>Explored 3 files</summary>
>
> - Read `C:/Users/thierry/.agents/skills/graphify/SKILL.md`
> - Read `C:/Users/thierry/.codex/skills/dcs_grpc_client/SKILL.md`
> - Read `C:/Users/thierry/.codex/skills/dcs_scripting/SKILL.md`
>
> </details>
>
> - Ran `(Get-Content -LiteralPath 'C:\Users\thierry\.agents\skills\graphify\SKILL.md').Count`
>
> - Ran `Get-ChildItem -LiteralPath 'C:\Users\thierry\.codex\skills\dcs_scripting\references' -File | Select-Object -ExpandProperty Name`
>
> - Ran `$p='C:\Users\thierry\.codex\memories\MEMORY.md'; $l=Get-Content -LiteralPath $p; for($i=123;$i -le 187;$i++){ '{0}:{1}' -f ($i+1),$l[$i] }; for($i=528;$i -le 709;$i++){ '{0}:{1}' -f ($i+1),$l[$i] }`
>
> - Ran `(Get-Content -LiteralPath 'C:\Users\thierry\.codex\skills\dcs_grpc_client\references\dcs_grpc_api.md').Count`
>
> - Ran `Get-Content -LiteralPath 'C:\Users\thierry\.agents\skills\graphify\SKILL.md' | Select-Object -First 220`
>
> - Ran `Get-Content -LiteralPath 'C:\Users\thierry\.agents\skills\graphify\SKILL.md' | Select-Object -Skip 220 -First 220`
>
> - Ran `Get-Content -LiteralPath 'C:\Users\thierry\.agents\skills\graphify\SKILL.md' | Select-Object -Skip 440`
>
> </details>
>
> The prior notes flag two known failure classes that may still matter here—final-approach samples being joined across separate pattern runs, and hook state being treated as “ever up” instead of “recently up near touchdown.” I’ll verify both against these four snapshots; I won’t assume those older diagnoses still match yesterday’s code.
>
> <details><summary>Explored a file, ran 5 commands</summary>
>
> - Ran `$dirs = @('DCS-gRPC-lso-new','DCS-gRPC-lso-origin','rust-server-new','rust-server-origin'); Get-ChildItem -Force | Select-Object Mode,Name; foreach($d in $dirs){ Write-Output "[$d]"; if(Test-Path -LiteralPath $d){ $all=(rg --files --hidden -g '!target/**' -g '!.git/**' $d | Measure-Object).Count; $rust=(rg --files $d -g '*.rs' -g '!target/**' | Measure-Object).Count; $lua=(rg --files $d -g '*.lua' -g '!target/**' | Measure-Object).Count; $md=(rg --files $d -g '*.md' -g '!target/**' | Measure-Object).Count; Write-Output "files=$all rust=$rust lua=$lua md=$md"; git -C $d status --short --branch; git -C $d log -1 --date=iso-strict --format='HEAD=%H%nDATE=%ad%nSUBJECT=%s'; if(Test-Path -LiteralPath "$d\graphify-out\graph.json"){ Write-Output 'graphify=yes' } else { Write-Output 'graphify=no' }; if(Test-Path -LiteralPath "$d\AGENTS.md"){ Write-Output 'AGENTS.md=yes' } } else { Write-Output 'MISSING' } }`
>
> <details><summary>Explored a file</summary>
>
> - Read `C:/Users/thierry/.agents/skills/graphify/references/query.md`
>
> </details>
>
> - Ran `foreach($d in @('DCS-gRPC-lso-new','DCS-gRPC-lso-origin','rust-server-new','rust-server-origin')){ Write-Output "[$d]"; Get-ChildItem -LiteralPath $d -Force | Select-Object Mode,Name }`
>
> - Ran `foreach($d in @('DCS-gRPC-lso-new','DCS-gRPC-lso-origin','rust-server-new','rust-server-origin')){ Write-Output "[$d]"; Get-ChildItem -LiteralPath $d -Filter AGENTS.md -Recurse -Force -ErrorAction SilentlyContinue | Select-Object -ExpandProperty FullName }`
>
> - Ran `$py=Get-Content -LiteralPath 'graphify-out\.graphify_python'; Write-Output "python=$py"; & $py -c "import json,re; from pathlib import Path; d=json.loads(Path('graphify-out/graph.json').read_text(encoding='utf-8')); v=set(); [(v.add(p.lower())) for n in d['nodes'] for c in re.findall(r'[^\W\d_]+',n.get('label','') or '',re.UNICODE) for p in (re.findall(r'[A-Z]+(?=[A-Z][a-z])|[A-Z]?[a-z]+|[A-Z]+',c) or [c]) if 3<=len(p)<=30]; Path('graphify-out/.vocab.txt').write_text('\n'.join(sorted(v)),encoding='utf-8'); print(f'nodes={len(d[\"nodes\"])} vocab={len(v)}')"; rg -n -i "recover|record|track|groove|stream|unit|datum|sample|position|mission" graphify-out\.vocab.txt`
>
> - Ran `$py=Get-Content -LiteralPath 'graphify-out\.graphify_python'; Write-Output "python=$py"; & $py -c "import json,re; from pathlib import Path; d=json.loads(Path('graphify-out/graph.json').read_text(encoding='utf-8')); v=set(); [(v.add(p.lower())) for n in d['nodes'] for c in re.findall(r'[^\W\d_]+',n.get('label','') or '',re.UNICODE) for p in (re.findall(r'[A-Z]+(?=[A-Z][a-z])|[A-Z]?[a-z]+|[A-Z]+',c) or [c]) if 3<=len(p)<=30]; Path('graphify-out/.vocab.txt').write_text('\n'.join(sorted(v)),encoding='utf-8'); print(f'nodes={len(d[\"nodes\"])} vocab={len(v)}')"; rg -n -i "stream|unit|position|coalition|interval|timer|schedule|grpc|lua|mission" graphify-out\.vocab.txt`
>
> </details>
>
> The existing LSO graph vocabulary supports this audited query expansion: `[recovery, recoveries, record, datums, groove, track, stream, unit, position, mission]`. The server graph needs a small interpreter repair before it can be queried; its graph data itself is present.
>
> <details><summary>Ran 41 commands</summary>
>
> - Ran `Get-Command graphify -ErrorAction SilentlyContinue | Select-Object -ExpandProperty Source; uv tool dir; Get-Command python -ErrorAction SilentlyContinue | Select-Object -ExpandProperty Source`
> - Ran `graphify query "recovery recoveries record datums groove track stream unit position mission" --dfs --budget 3000`
> - Ran `Get-Content -LiteralPath 'graphify-out\GRAPH_REPORT.md' -First 80`
> - Ran `py -0p; where.exe python; where.exe python3; Get-ChildItem -LiteralPath 'C:\Users\thierry\AppData\Roaming\uv\tools' -Directory -ErrorAction SilentlyContinue | Select-Object -ExpandProperty FullName; Get-ChildItem -LiteralPath 'C:\Users\thierry\.local\share\uv\tools' -Directory -ErrorAction SilentlyContinue | Select-Object -ExpandProperty FullName`
> - Ran `Write-Output '[LSO source diff]'; git diff --no-index --stat -- 'DCS-gRPC-lso-origin/src' 'DCS-gRPC-lso-new/src'; Write-Output '[LSO tests diff]'; git diff --no-index --stat -- 'DCS-gRPC-lso-origin/tests' 'DCS-gRPC-lso-new/tests'; Write-Output '[LSO manifests]'; git diff --no-index --stat -- 'DCS-gRPC-lso-origin/Cargo.toml' 'DCS-gRPC-lso-new/Cargo.toml'; git diff --no-index --stat -- 'DCS-gRPC-lso-origin/Cargo.lock' 'DCS-gRPC-lso-new/Cargo.lock'; Write-Output '[server source diff]'; git diff --no-index --stat -- 'rust-server-origin/src' 'rust-server-new/src'; Write-Output '[server lua diff]'; git diff --no-index --stat -- 'rust-server-origin/lua' 'rust-server-new/lua'; Write-Output '[server protos diff]'; git diff --no-index --stat -- 'rust-server-origin/protos' 'rust-server-new/protos'; Write-Output '[server other code diff]'; git diff --no-index --stat -- 'rust-server-origin/stubs' 'rust-server-new/stubs'; git diff --no-index --stat -- 'rust-server-origin/repl' 'rust-server-new/repl'; git diff --no-index --stat -- 'rust-server-origin/srs' 'rust-server-new/srs'; git diff --no-index --stat -- 'rust-server-origin/tts' 'rust-server-new/tts'; Write-Output '[server manifests]'; git diff --no-index --stat -- 'rust-server-origin/Cargo.toml' 'rust-server-new/Cargo.toml'; git diff --no-index --stat -- 'rust-server-origin/Cargo.lock' 'rust-server-new/Cargo.lock'`
> - Ran `rg -n -i -C 6 "StreamUnits|StreamUnitsRequest|StreamUnitsResponse|GetTransform|GetPosition|GetVelocity|GetUnit|Eval|GetDrawArgumentValue" 'C:\Users\thierry\.codex\skills\dcs_grpc_client\references\dcs_grpc_api.md'`
> - Ran `rg -n "struct |enum |fn |async fn |stream_units|interval|sleep|tick|Track::|Datum|groove|extrapolat|interpolat|predict|previous|last_|time|position|velocity|hook" src/tasks/record_recovery.rs src/track.rs src/client/unit_client.rs src/tasks/detect_recovery_attempt.rs src/telemetry.rs src/metrics.rs`
> - Ran `rg -n "struct |enum |fn |async fn |stream_units|interval|sleep|tick|Track::|Datum|groove|extrapolat|interpolat|predict|previous|last_|time|position|velocity|hook" src/tasks/record_recovery.rs src/track.rs src/client/unit_client.rs src/tasks/detect_recovery_attempt.rs`
> - Ran `rg -n "stream_units|StreamUnits|interval|tick|scheduleFunction|schedule_function|timer|getUnits|getUnit|getPosition|getVelocity|transform|rate|frequency|duration|yield|tokio::time|broadcast|mpsc|oneshot" src lua protos/dcs/mission/v0/mission.proto`
> - Ran `foreach($base in @('DCS-gRPC-lso-origin','DCS-gRPC-lso-new')){ Write-Output "[$base]"; foreach($f in @('src/tasks/record_recovery.rs','src/track.rs','src/tasks/detect_recovery_attempt.rs','src/client/unit_client.rs','src/telemetry.rs','src/metrics.rs')){ $p=Join-Path $base $f; if(Test-Path -LiteralPath $p){ $lines=(Get-Content -LiteralPath $p).Count; $bytes=(Get-Item -LiteralPath $p).Length; Write-Output "$f lines=$lines bytes=$bytes" } } }`
> - Ran `$p='src/tasks/record_recovery.rs'; $l=Get-Content -LiteralPath $p; for($i=0;$i -lt [Math]::Min(300,$l.Count);$i++){ '{0,4}: {1}' -f ($i+1),$l[$i] }`
> - Ran `$p='src/tasks/record_recovery.rs'; $l=Get-Content -LiteralPath $p; for($i=300;$i -lt $l.Count;$i++){ '{0,4}: {1}' -f ($i+1),$l[$i] }`
> - Ran `$p='src/track.rs'; $l=Get-Content -LiteralPath $p; for($i=0;$i -lt $l.Count;$i++){ '{0,4}: {1}' -f ($i+1),$l[$i] }; $p='src/tasks/detect_recovery_attempt.rs'; $l=Get-Content -LiteralPath $p; for($i=0;$i -lt $l.Count;$i++){ '{0,4}: {1}' -f ($i+1),$l[$i] }; $p='src/client/unit_client.rs'; $l=Get-Content -LiteralPath $p; for($i=0;$i -lt $l.Count;$i++){ '{0,4}: {1}' -f ($i+1),$l[$i] }`
> - Ran `$p='src/utils/interval.rs'; $l=Get-Content $p; for($i=0;$i -lt $l.Count;$i++){ '{0,4}: {1}' -f ($i+1),$l[$i] }; $p='src/transform.rs'; $l=Get-Content $p; for($i=0;$i -lt $l.Count;$i++){ '{0,4}: {1}' -f ($i+1),$l[$i] }`
> - Ran `$p='src/utils/interval.rs'; $l=Get-Content $p; for($i=0;$i -lt $l.Count;$i++){ '{0,4}: {1}' -f ($i+1),$l[$i] }; $p='src/transform.rs'; $l=Get-Content $p; for($i=0;$i -lt $l.Count;$i++){ '{0,4}: {1}' -f ($i+1),$l[$i] }`
> - Ran `rg -n "^(pub\s+)?(const|static|struct|enum|async fn|fn)|^impl " src/tasks/record_recovery.rs src/track.rs src/telemetry.rs src/metrics.rs src/client/unit_client.rs src/tasks/detect_recovery_attempt.rs`
> - Ran `rg -n "^(pub\s+)?(const|static|struct|enum|async fn|fn)|^impl " src/stream.rs src/rpc/mission.rs src/rpc/unit.rs`
> - Ran `rg -n "^(pub\s+)?(const|static|struct|enum|async fn|fn)|^impl " src/stream.rs src/rpc/mission.rs src/rpc/unit.rs`
> - Ran `$p='src/tasks/record_recovery.rs'; $l=Get-Content $p; for($i=0;$i -lt 380;$i++){ '{0,4}: {1}' -f ($i+1),$l[$i] }`
> - Ran `$p='src/tasks/record_recovery.rs'; $l=Get-Content $p; for($i=380;$i -lt 780;$i++){ '{0,4}: {1}' -f ($i+1),$l[$i] }`
> - Ran `$p='src/tasks/record_recovery.rs'; $l=Get-Content $p; for($i=780;$i -lt 1211;$i++){ '{0,4}: {1}' -f ($i+1),$l[$i] }`
> - Ran `$p='src/tasks/record_recovery.rs'; $l=Get-Content $p; for($i=338;$i -lt 560;$i++){ '{0,4}: {1}' -f ($i+1),$l[$i] }`
> - Ran `$p='src/tasks/record_recovery.rs'; $l=Get-Content $p; for($i=560;$i -lt 800;$i++){ '{0,4}: {1}' -f ($i+1),$l[$i] }`
> - Ran `$p='src/telemetry.rs'; $l=Get-Content $p; for($i=0;$i -lt $l.Count;$i++){ '{0,4}: {1}' -f ($i+1),$l[$i] }`
> - Ran `$p='src/client/unit_client.rs'; $l=Get-Content $p; for($i=0;$i -lt $l.Count;$i++){ '{0,4}: {1}' -f ($i+1),$l[$i] }; $p='src/tasks/detect_recovery_attempt.rs'; $l=Get-Content $p; for($i=0;$i -lt $l.Count;$i++){ '{0,4}: {1}' -f ($i+1),$l[$i] }`
> - Ran `$p='src/client/mod.rs'; $l=Get-Content $p; for($i=0;$i -lt $l.Count;$i++){ '{0,4}: {1}' -f ($i+1),$l[$i] }`
> - Ran `rg -n -C 5 "HookSampling|hook_sampling|frequency_hz|timeout|RPC_DEADLINE|throughput|grpc|endpoint|channel" src README.md docs Cargo.toml`
> - Ran `rg -n -C 5 "throughputLimit|callsPerTick|interval|skipFrames|max_calls|requests|queue|poll|timeout" lua src README.md STATUS.md CHANGELOG.md`
> - Ran `git -c core.autocrlf=false diff --no-index --unified=80 -- '..\rust-server-origin\src\stream.rs' 'src\stream.rs'`
> - Ran `git -c core.autocrlf=false diff --no-index --unified=60 -- '..\rust-server-origin\lua\DCS-gRPC\grpc.lua' 'lua\DCS-gRPC\grpc.lua'; git -c core.autocrlf=false diff --no-index --unified=60 -- '..\rust-server-origin\lua\DCS-gRPC\methods\unit.lua' 'lua\DCS-gRPC\methods\unit.lua'`
> - Ran `git -c core.autocrlf=false diff --no-index --unified=30 -- '..\rust-server-origin\src\rpc\unit.rs' 'src\rpc\unit.rs'; git -c core.autocrlf=false diff --no-index --unified=30 -- '..\rust-server-origin\src\rpc\mission.rs' 'src\rpc\mission.rs'; git -c core.autocrlf=false diff --no-index --unified=30 -- '..\rust-server-origin\src\rpc.rs' 'src\rpc.rs'`
> - Ran `rg -n -C 5 "detect_recovery_attempt|record_recovery|tokio::spawn|JoinSet|Semaphore|mpsc|capacity|TaskParams|for .*carrier|for .*plane|carriers|planes|recover" src/commands/run.rs src/tasks/mod.rs`
> - Ran `rg -n -C 5 "detect_recovery_attempt|record_recovery|tokio::spawn|JoinSet|Semaphore|mpsc|capacity|TaskParams|for .*carrier|for .*plane|carriers|planes|recover" src/commands/run.rs src/tasks/mod.rs`
> - Ran `git -c core.autocrlf=false diff --no-index --unified=20 -- '..\rust-server-origin\src\srs.rs' 'src\srs.rs'`
> - Ran `rg -n -C 8 "groove|in_groove|entered|quarter|gate|alignment|telemetry_quality|sample_gap|invalid_reason|next_sample|fn next\(|fn landed|wire|hook|TouchAndGo|Waveoff|Bolter|finish\(" src/track.rs`
> - Ran `$p='src/track.rs'; $l=Get-Content $p; for($i=500;$i -lt 760;$i++){ '{0,4}: {1}' -f ($i+1),$l[$i] }`
> - Ran `$p='src/track.rs'; $l=Get-Content $p; for($i=760;$i -lt 1040;$i++){ '{0,4}: {1}' -f ($i+1),$l[$i] }`
> - Ran `$p='src/track.rs'; $l=Get-Content $p; for($i=1040;$i -lt 1330;$i++){ '{0,4}: {1}' -f ($i+1),$l[$i] }`
> - Ran `$p='src/track.rs'; $l=Get-Content $p; for($i=1330;$i -lt 1610;$i++){ '{0,4}: {1}' -f ($i+1),$l[$i] }`
> - Ran `$p='src/track.rs'; $l=Get-Content $p; for($i=818;$i -lt 1030;$i++){ '{0,4}: {1}' -f ($i+1),$l[$i] }`
> - Ran `$p='src/track.rs'; $l=Get-Content $p; for($i=1030;$i -lt 1192;$i++){ '{0,4}: {1}' -f ($i+1),$l[$i] }`
>
> </details>
>
> The first high-confidence finding is architectural: recovery recording still does two unary `GetTransform` calls every 100 ms; it does not consume `StreamUnits`. The new LSO then optionally adds a separate 4 Hz hook RPC. I’m now testing whether the added alignment, gate, and supervision logic preserves cadence under load, and whether server 0.9.0 changed the actual transform path.
>
> <details><summary>Explored 5 files, ran 19 commands</summary>
>
> - Ran `cargo test --locked --no-fail-fast`
>
> - Ran `cargo test --locked --workspace --no-fail-fast`
>
> - Ran `cargo fmt -- --check`
>
> - Ran `cargo fmt --all -- --check`
>
> - Ran `Get-Content -LiteralPath CHANGELOG.md -First 180; Get-Content -LiteralPath STATUS.md -First 240; Get-ChildItem -LiteralPath src,lua,protos -Recurse -File | Sort-Object LastWriteTime -Descending | Select-Object -First 30 LastWriteTime,Length,FullName`
>
> - Ran `Get-Content -LiteralPath CHANGES.md -Raw; Get-ChildItem -LiteralPath src -Recurse -File | Sort-Object LastWriteTime -Descending | Select-Object -First 30 LastWriteTime,Length,FullName`
>
> - Ran `Get-Content -LiteralPath Cargo.toml -Raw; rg -n "name = \"dcs-grpc|version = \"0\.9" Cargo.lock | Select-Object -First 40`
>
> - Ran `Get-Content -LiteralPath Cargo.toml -Raw; rg -n "name = \"dcs-grpc|version = \"0\." Cargo.lock | Select-Object -First 40`
>
> - Ran `$p='src/grading.rs'; $l=Get-Content $p; for($i=0;$i -lt 260;$i++){ '{0,4}: {1}' -f ($i+1),$l[$i] }; for($i=520;$i -lt 590;$i++){ '{0,4}: {1}' -f ($i+1),$l[$i] }`
>
> - Ran `cargo test --locked --no-fail-fast`
>
> - Ran `cargo clippy --locked --all-targets -- -D warnings`
>
> - Ran `cargo clippy --locked --workspace --all-targets -- -D warnings`
>
> - Ran `Get-Command luacheck -ErrorAction SilentlyContinue | Select-Object -ExpandProperty Source`
>
> - Ran `rg -n "GRPC\.error\s*=|function GRPC\.error|errorInternal|errorUnimplemented|errorNotFound|success\s*=" lua/DCS-gRPC/grpc.lua lua/DCS-gRPC/methods`
>
> - Ran `rg -n "GetTransform|GetDrawArgumentValue|GetRequest|GetResponse|UnitService|StreamUnitsRequest|poll_rate|max_backoff" protos/dcs/unit/v0/unit.proto protos/dcs/mission/v0/mission.proto lua/DCS-gRPC/methods/unit.lua src/rpc/unit.rs`
>
> - Ran `$p='lua/DCS-gRPC/grpc.lua'; $l=Get-Content $p; for($i=40;$i -lt 125;$i++){ '{0,4}: {1}' -f ($i+1),$l[$i] }; for($i=180;$i -lt 285;$i++){ '{0,4}: {1}' -f ($i+1),$l[$i] }`
>
> - Ran `$p='lua/DCS-gRPC/grpc.lua'; $l=Get-Content $p; for($i=115;$i -lt 150;$i++){ '{0,4}: {1}' -f ($i+1),$l[$i] }; rg -n "GRPC\.errorInternal" lua`
>
> <details><summary>Explored 5 files</summary>
>
> - Read `C:/Users/thierry/.codex/skills/dcs_scripting/references/func_getPosition.md`
> - Read `C:/Users/thierry/.codex/skills/dcs_scripting/references/func_getDrawArgumentValue.md`
> - Read `C:/Users/thierry/.codex/skills/dcs_scripting/references/func_getVelocity.md`
> - Read `C:/Users/thierry/.codex/skills/dcs_scripting/references/func_getTime.md`
> - Read `C:/Users/thierry/.codex/skills/dcs_scripting/references/func_scheduleFunction.md`
>
> </details>
>
> - Ran `Get-ChildItem -LiteralPath trap_records -File | Sort-Object LastWriteTime | Select-Object LastWriteTime,Length,Name; Get-ChildItem -LiteralPath 'trap sample' -File -Recurse | Select-Object LastWriteTime,Length,FullName`
>
> - Ran `$f=Get-ChildItem -LiteralPath trap_records -Filter '*.json' | Sort-Object LastWriteTime -Descending | Select-Object -First 1; Write-Output $f.FullName; $j=Get-Content -LiteralPath $f.FullName -Raw | ConvertFrom-Json; $j.PSObject.Properties.Name; Write-Output ('datums=' + $j.datums.Count); $j.telemetry_quality | Format-List *; $j.gate_deviations | ConvertTo-Json -Depth 8; $j.datums | Select-Object -First 3 | ConvertTo-Json -Depth 5`
>
> </details>
>
> Context compaction
>
> Source: automatic
> Status: completed
>
> I’ve finished the source-level pipeline trace. The key finding is already clear: the original recorder was genuine polling with no trajectory extrapolation, while the new recorder explicitly synthesizes position for timestamp-skewed samples and also applies carrier smoothing inside grading geometry. I’m now quantifying the gaps in the saved schema-v3 recoveries and closing the test/lint evidence before writing the report.
>
> <details><summary>Ran 17 commands</summary>
>
> - Ran `$files = Get-ChildItem -LiteralPath '.\DCS-gRPC-lso-new\trap_records' -Filter '*.json' -File; $rows = foreach ($file in $files) { try { $record = Get-Content -LiteralPath $file.FullName -Raw | ConvertFrom-Json; if ($record.schema_version -eq 3) { [pscustomobject]@{ File=$file.Name; Datums=@($record.datums).Count; MaxGapMs=$record.telemetry_quality.max_sample_gap_ms; MaxSkewMs=$record.telemetry_quality.max_skew_ms; Warnings=$record.telemetry_quality.warning_samples; Invalid=$record.telemetry_quality.invalid_samples; Completeness=$record.telemetry_quality.completeness; Grade=$record.grading.final_grade.code } } } catch {} }; "schema3_count=$(@($rows).Count)"; $rows | Sort-Object File | Format-Table -AutoSize`
> - Ran `$files = Get-ChildItem -LiteralPath '.\DCS-gRPC-lso-new\trap_records' -Filter '*.json' -File | Sort-Object LastWriteTime -Descending | Select-Object -First 3; foreach ($file in $files) { $record = Get-Content -LiteralPath $file.FullName -Raw | ConvertFrom-Json; [pscustomobject]@{File=$file.Name; Keys=($record.PSObject.Properties.Name -join ','); Schema=$record.schema_version; Version=$record.version} | Format-List }`
> - Ran `$files = Get-ChildItem -LiteralPath '.\DCS-gRPC-lso-new\trap_records' -Filter '*.json' -File; $rows = foreach ($file in $files) { try { $record = Get-Content -LiteralPath $file.FullName -Raw | ConvertFrom-Json; if ($record.schema_version -eq 2 -and $record.telemetry_quality) { $methods = @($record.datums | ForEach-Object { $_.telemetry.alignment_method } | Where-Object { $_ } | Group-Object | ForEach-Object { "$($_.Name):$($_.Count)" }) -join ';'; [pscustomobject]@{ File=$file.Name; Datums=@($record.datums).Count; MaxGap=[math]::Round([double]$record.telemetry_quality.max_sample_gap_ms,1); MaxSkew=[math]::Round([double]$record.telemetry_quality.max_skew_ms,1); Warnings=[int]$record.telemetry_quality.warning_samples; Invalid=[int]$record.telemetry_quality.invalid_samples; Complete=$record.telemetry_quality.completeness; Grade=$record.grading; Methods=$methods } } } catch {} }; "schema2_with_quality=$(@($rows).Count)"; $rows | Sort-Object File | Format-Table -AutoSize`
> - Ran `$file = Get-ChildItem -LiteralPath '.\DCS-gRPC-lso-new\trap_records' -Filter '*.json' -File | Sort-Object LastWriteTime -Descending | Select-Object -First 1; $record = Get-Content -LiteralPath $file.FullName -Raw | ConvertFrom-Json; $record.telemetry_quality | ConvertTo-Json -Depth 5; $record.datums[0] | ConvertTo-Json -Depth 5; $record.gate_deviations | ConvertTo-Json -Depth 5`
> - Ran `$files = Get-ChildItem -LiteralPath '.\DCS-gRPC-lso-new\trap_records' -Filter '*.json' -File; $records = foreach ($file in $files) { try { $r = Get-Content -LiteralPath $file.FullName -Raw | ConvertFrom-Json; if ($r.schema_version -eq 2 -and $r.telemetry_quality) { $r } } catch {} }; $all = @($records | ForEach-Object { $_.datums }); $groove = @($all | Where-Object { $_.x -ge -200 -and $_.x -le 1389 }); function Summarize($name, $items) { $gaps = @($items | ForEach-Object { [double]$_.sample_gap_ms } | Sort-Object); $n=$gaps.Count; $p50=$gaps[[math]::Floor(($n-1)*0.50)]; $p90=$gaps[[math]::Floor(($n-1)*0.90)]; $p95=$gaps[[math]::Floor(($n-1)*0.95)]; $p99=$gaps[[math]::Floor(($n-1)*0.99)]; [pscustomobject]@{Scope=$name; Samples=$n; P50=[math]::Round($p50,1); P90=[math]::Round($p90,1); P95=[math]::Round($p95,1); P99=[math]::Round($p99,1); Over150=@($gaps|Where-Object {$_ -gt 150}).Count; Over300=@($gaps|Where-Object {$_ -gt 300}).Count; Over1000=@($gaps|Where-Object {$_ -gt 1000}).Count; Invalid=@($items|Where-Object {-not $_.telemetry_valid}).Count} }; Summarize 'all' $all; Summarize 'groove_x' $groove; 'alignment_all'; $all | Group-Object alignment | Sort-Object Name | Select-Object Name,Count | Format-Table -AutoSize; 'gate_quality'; $records | ForEach-Object { $_.gate_deviations.three_quarter_quality.status; $_.gate_deviations.half_quality.status; $_.gate_deviations.quarter_quality.status } | Group-Object | Select-Object Name,Count | Format-Table -AutoSize`
> - Ran `$files = Get-ChildItem -LiteralPath '.\DCS-gRPC-lso-new\trap_records' -Filter '*.json' -File; $gaps = foreach ($file in $files) { try { $r = Get-Content -LiteralPath $file.FullName -Raw | ConvertFrom-Json; if ($r.schema_version -eq 2 -and $r.telemetry_quality) { $r.datums | ForEach-Object { [math]::Round(([double]$_.sample_gap_ms)/50)*50 } } } catch {} }; $gaps | Group-Object | Sort-Object { [double]$_.Name } | Select-Object Name,Count | Format-Table -AutoSize`
> - Ran `$files = Get-ChildItem -LiteralPath '.\DCS-gRPC-lso-new\trap_records' -Filter '*.json' -File; $pairs = [System.Collections.Generic.List[object]]::new(); foreach ($file in $files) { try { $r = Get-Content -LiteralPath $file.FullName -Raw | ConvertFrom-Json; if ($r.schema_version -ne 2 -or -not $r.telemetry_quality) { continue }; $previous=$null; foreach ($d in $r.datums) { if ($null -ne $previous) { $wall=[double]$d.plane_received_unix_ms-[double]$previous.plane_received_unix_ms; $pairs.Add([pscustomobject]@{Dcs=[double]$d.sample_gap_ms; Wall=$wall}) }; $previous=$d } } catch {} }; foreach($condition in @('all','dcs_over_300','dcs_over_900')) { if($condition -eq 'all'){$set=@($pairs)} elseif($condition -eq 'dcs_over_300'){$set=@($pairs|Where-Object {$_.Dcs -gt 300})} else {$set=@($pairs|Where-Object {$_.Dcs -gt 900})}; $walls=@($set.Wall|Sort-Object); $dcs=@($set.Dcs|Sort-Object); $n=$set.Count; [pscustomobject]@{Scope=$condition;N=$n;WallP50=[math]::Round($walls[[math]::Floor(($n-1)*.5)],1);WallP90=[math]::Round($walls[[math]::Floor(($n-1)*.9)],1);DcsP50=[math]::Round($dcs[[math]::Floor(($n-1)*.5)],1);DcsP90=[math]::Round($dcs[[math]::Floor(($n-1)*.9)],1)} }`
> - Ran `rg -n "MissedTickBehavior|interval\(|try_join|align\(|drain_hook_samples|associated_time_dcs|TelemetrySample|next_with_sample|hook" .\DCS-gRPC-lso-new\src\tasks\record_recovery.rs .\DCS-gRPC-lso-origin\src\tasks\record_recovery.rs .\DCS-gRPC-lso-new\src\telemetry.rs; rg -n "GROOVE|groove_entry|groove_time|at_three_quarter|GATE|EMA|FILTER|filtered|alpha|stale_gate|MAX_GATE|telemetry_valid|UnconfirmedArrest|TouchAndGo" .\DCS-gRPC-lso-new\src\track.rs .\DCS-gRPC-lso-new\src\grading.rs; rg -n "StreamUnits|poll_rate|max_backoff|get_transform|try_join_all|throughputLimit|callsPerTick|requestLoop|errorInternal|version =" .\rust-server-new\src .\rust-server-new\lua .\rust-server-new\protos .\rust-server-new\Cargo.toml .\DCS-gRPC-lso-new\Cargo.toml`
> - Ran `rg -n "function GRPC\.error|GRPC\.error[A-Z]" .\rust-server-new\lua\DCS-gRPC\grpc.lua .\rust-server-origin\lua\DCS-gRPC\grpc.lua .\rust-server-new\lua\DCS-gRPC\methods; rg -n "getUnitTransform|rawTransform|timer\.getTime|getPosition|getVelocity|GetTransformRequest|get_transform\(" .\rust-server-new\lua .\rust-server-new\src\rpc .\DCS-gRPC-lso-new\src\unit_client.rs .\DCS-gRPC-lso-origin\src\unit_client.rs`
> - Ran `$targets = @(@{Path='.\DCS-gRPC-lso-origin\src\tasks\record_recovery.rs';Start=50;End=170},@{Path='.\DCS-gRPC-lso-origin\src\track.rs';Start=55;End=135},@{Path='.\DCS-gRPC-lso-new\src\telemetry.rs';Start=100;End=275},@{Path='.\DCS-gRPC-lso-new\src\tasks\record_recovery.rs';Start=340;End=485}); foreach($t in $targets){"FILE $($t.Path)"; $lines=Get-Content -LiteralPath $t.Path; for($i=$t.Start;$i -le $t.End;$i++){ '{0,5}: {1}' -f $i,$lines[$i-1] }}`
> - Ran `rg -n "MissedTickBehavior|set_missed_tick_behavior|pub fn interval" .\DCS-gRPC-lso-new\src .\DCS-gRPC-lso-origin\src; $targets = @(@{Path='.\DCS-gRPC-lso-new\src\track.rs';Start=570;End=935},@{Path='.\DCS-gRPC-lso-new\src\track.rs';Start=1085;End=1160},@{Path='.\DCS-gRPC-lso-new\src\grading.rs';Start=160;End=220},@{Path='.\DCS-gRPC-lso-new\src\grading.rs';Start=535;End=590}); foreach($t in $targets){"FILE $($t.Path)"; $lines=Get-Content -LiteralPath $t.Path; for($i=$t.Start;$i -le $t.End;$i++){ '{0,5}: {1}' -f $i,$lines[$i-1] }}`
> - Ran `$targets = @(@{Path='.\DCS-gRPC-lso-origin\src\tasks\detector.rs';Start=1;End=120},@{Path='.\DCS-gRPC-lso-new\src\tasks\detector.rs';Start=1;End=120},@{Path='.\rust-server-new\src\stream.rs';Start=25;End=110},@{Path='.\rust-server-new\src\stream.rs';Start=195;End=320},@{Path='.\rust-server-new\lua\DCS-gRPC\grpc.lua';Start=195;End=235}); foreach($t in $targets){"FILE $($t.Path)"; $lines=Get-Content -LiteralPath $t.Path; $end=[math]::Min($t.End,$lines.Count); for($i=$t.Start;$i -le $end;$i++){ '{0,5}: {1}' -f $i,$lines[$i-1] }}`
> - Ran `rg -n "1\.5|3\.5|500|1100|detector|compatible|interval\(Duration::from_secs\(2\)|dot\(" .\DCS-gRPC-lso-origin\src\tasks .\DCS-gRPC-lso-new\src\tasks`
> - Ran `git diff --no-index --unified=0 -- .\rust-server-origin\src\stream.rs .\rust-server-new\src\stream.rs; git diff --no-index --unified=0 -- .\rust-server-origin\lua\DCS-gRPC\grpc.lua .\rust-server-new\lua\DCS-gRPC\grpc.lua; git diff --no-index --unified=0 -- .\rust-server-origin\lua\DCS-gRPC\methods\unit.lua .\rust-server-new\lua\DCS-gRPC\methods\unit.lua`
> - Ran `rg -n "dcs-grpc-stubs|tag =|0\.9\.0|5bd6d6e|0\.9\.1|0\.8\.1" .\DCS-gRPC-lso-new\Cargo.toml .\DCS-gRPC-lso-new\Cargo.lock .\rust-server-new\Cargo.toml .\rust-server-new\CHANGELOG.md .\rust-server-origin\Cargo.toml; rg -n "message StreamUnitsRequest|poll_rate|low rate|replay|from_replay|DIRECT_SKEW_MS|MAX_EXTRAPOLATION_MS|SAMPLE_GAP" .\rust-server-new\protos\dcs\mission\v0\mission.proto .\DCS-gRPC-lso-new\src\telemetry.rs`
> - Ran `$lso = git diff --no-index --numstat -- .\DCS-gRPC-lso-origin .\DCS-gRPC-lso-new 2>$null; $server = git diff --no-index --numstat -- .\rust-server-origin .\rust-server-new 2>$null; function Summarize($name,$rows){$parsed=@($rows | Where-Object {$_ -match '^\d+\s+\d+\s+'} | ForEach-Object { $p=$_ -split '\s+',3; [pscustomobject]@{Add=[int]$p[0];Del=[int]$p[1];Path=$p[2]} }); [pscustomobject]@{Pair=$name;ChangedFiles=$parsed.Count;Added=($parsed|Measure-Object Add -Sum).Sum;Deleted=($parsed|Measure-Object Del -Sum).Sum}; $parsed | Sort-Object Add -Descending | Select-Object -First 12 | Format-Table -AutoSize}; Summarize 'LSO' $lso; Summarize 'server' $server`
> - Ran `rg -n "schema_version|SCHEMA|HookSampling|frequency_hz|timeout_ms|mode.*Independent|independent" .\DCS-gRPC-lso-new\src .\DCS-gRPC-lso-new\README.md .\DCS-gRPC-lso-new\docs; rg -n "get_observed_transform|get_transform_for|timeout\(|RpcKind" .\DCS-gRPC-lso-new\src`
>
> </details>
>
> The saved evidence is worse than a small occasional miss: across the 15 telemetry-quality reports in this snapshot, the nominal 100 ms loop has a 111 ms median but about 21% of samples exceed 300 ms; the 90th percentile is about 800 ms. The same distribution appears in the groove, so the problem is real response latency, not just chart rendering. Those files are schema v2 while current source writes v3, so I’ll label them reproduction evidence—not proof that the current head has been live-validated.
>
> <details><summary>Created a file, edited a file, ran 9 commands</summary>
>
> - Ran `cargo test --locked --workspace --no-fail-fast`
> - Ran `$lines=Get-Content -LiteralPath '.\DCS-gRPC-lso-new\src\telemetry.rs'; for($i=1;$i -le 95;$i++){ '{0,5}: {1}' -f $i,$lines[$i-1] }; $lines=Get-Content -LiteralPath '.\DCS-gRPC-lso-new\src\tasks\record_recovery.rs'; for($i=155;$i -le 182;$i++){ '{0,5}: {1}' -f $i,$lines[$i-1] }`
> - Ran `$roots=@('.\DCS-gRPC-lso-origin','.\DCS-gRPC-lso-new','.\rust-server-origin','.\rust-server-new'); foreach($root in $roots){$files=rg --files $root | Where-Object {$_ -notmatch '\\(target|graphify-out|trap_records|acmi|media|logs)\\'}; $code=@($files|Where-Object {$_ -match '\.(rs|lua|proto|toml|md|ps1|yml|yaml|json)$'}); [pscustomobject]@{Root=$root;InScopeFiles=$code.Count;Rust=@($code|Where-Object {$_ -match '\.rs$'}).Count;Lua=@($code|Where-Object {$_ -match '\.lua$'}).Count;Proto=@($code|Where-Object {$_ -match '\.proto$'}).Count;Docs=@($code|Where-Object {$_ -match '\.md$'}).Count}}`
> - Ran `$targets=@('.\DCS-gRPC-lso-origin\src\tasks\detect_recovery_attempt.rs','.\DCS-gRPC-lso-new\src\tasks\detect_recovery_attempt.rs'); foreach($path in $targets){"FILE $path"; $lines=Get-Content -LiteralPath $path; for($i=45;$i -le [math]::Min(105,$lines.Count);$i++){ '{0,5}: {1}' -f $i,$lines[$i-1] }}`
> - Wrote `./DCS_GRPC_LSO_RECOVERY_ANALYSIS.md` (+320 -0)
> - Ran `Get-Item -LiteralPath '.\DCS_GRPC_LSO_RECOVERY_ANALYSIS.md' | Select-Object FullName,Length,LastWriteTime; rg -n "^#|P0|100 passed|schema v2|0\.9\.1|errorInternal" .\DCS_GRPC_LSO_RECOVERY_ANALYSIS.md`
> - Ran ````git status --short; git diff --check -- .\DCS_GRPC_LSO_RECOVERY_ANALYSIS.md; $text=Get-Content -LiteralPath '.\DCS_GRPC_LSO_RECOVERY_ANALYSIS.md' -Raw; [pscustomobject]@{CodeFences=([regex]::Matches($text,'```')).Count; Tables=([regex]::Matches($text,'\| ---')).Count; Lines=($text -split "`n").Count}````
> - Ran `$path='C:\Users\thierry\.codex\memories\MEMORY.md'; $lines=Get-Content -LiteralPath $path; foreach($range in @(@(124,150),@(157,180),@(696,709))){"RANGE $($range[0])-$($range[1])"; for($i=$range[0];$i -le $range[1];$i++){ '{0,5}: {1}' -f $i,$lines[$i-1] }}`
> - Ran `$path='C:\Users\thierry\.codex\memories\MEMORY.md'; $lines=Get-Content -LiteralPath $path; for($i=630;$i -le 695;$i++){ '{0,5}: {1}' -f $i,$lines[$i-1] }`
> - Ran `$path='C:\Users\thierry\.codex\memories\MEMORY.md'; $lines=Get-Content -LiteralPath $path; for($i=600;$i -le 633;$i++){ '{0,5}: {1}' -f $i,$lines[$i-1] }`
> - Wrote `./DCS_GRPC_LSO_RECOVERY_ANALYSIS.md` (+1 -0)
>
> </details>
>
> <details><summary>Plan</summary>
>
> The forensic comparison, saved-record analysis, validation runs, and Markdown report are complete. No production code was changed.
>
> - [x] Load relevant memory and the graph/DCS skills; establish analysis constraints
> - [x] Inventory all four snapshots and compare versioned source, Lua, proto, docs, and tests
> - [x] Trace original and new recovery telemetry pipelines end to end
> - [x] Analyze groove grading, saved gap evidence, and gRPC server hot-path risks
> - [x] Run focused formatting, lint, and test validation on origin/new snapshots
> - [x] Write the durable analysis and incremental repair recommendations
>
> </details>
</details>

Analysis completed and documented in [DCS_GRPC_LSO_RECOVERY_ANALYSIS.md](./DCS_GRPC_LSO_RECOVERY_ANALYSIS.md).

Main conclusions:

- Original LSO used genuine nominal 10 Hz polling. It performed no interpolation, extrapolation, or smoothing—but slow RPCs created unfilled gaps.
- New saved telemetry confirms the problem: about 21% of groove samples exceed 300 ms; groove P90 is approximately 809 ms.
- New extrapolation addresses carrier/aircraft timestamp skew, not whole-loop stalls—the observed failure mode.
- Carrier EMA currently influences grading geometry. Irregular sampling therefore changes gates, groove entry, and outcome calculations.
- Groove time is recorded but completely ignored by grading. Current grading uses only three distance snapshots and does not derive continuous LSO corrections or trends.
- Live and replay alignment behavior can produce different geometry.
- `rust-server-new` is version 0.9.1. Its transform hot path is effectively unchanged from 0.8.1, so no server-version regression was identified.
- The server is not optimized or validated for guaranteed 10 Hz recovery telemetry. `StreamUnits` is unsuitable because it uses whole-second polling and fans out requests across mission units.
- A server Lua defect was found: several new methods call undefined `GRPC.errorInternal`.
- The current LSO suite has one real policy contradiction: 100 tests pass and the touch-and-go grading test fails.

The document includes an incremental repair sequence. The recommended first architectural correction is one narrow atomic recovery RPC returning carrier, aircraft, common DCS timestamp, and optional hook state from one Lua callback. No production code was changed.

> can you explain what means schema v2 and v3 in this comment
> The saved evidence is worse than a small occasional miss: across the 15 telemetry-quality reports in this snapshot, the nominal 100 ms loop has a 111 ms median but about 21% of samples exceed 300 ms; the 90th percentile is about 800 ms. The same distribution appears in the groove, so the problem is real response latency, not just chart rendering. Those files are schema v2 while current source writes v3, so I’ll label them reproduction evidence—not proof that the current head has been live-validated.

<details><summary>2 previous messages</summary>

> I’ll clarify this against the actual JSON contract: “schema” here means the saved recovery-report format, not the gRPC protocol or telemetry sampling algorithm. I’m checking which fields v3 added so the distinction is precise.
>
> <details><summary>Ran 2 commands</summary>
>
> - Ran `$targets=@(@{Path='.\DCS-gRPC-lso-new\docs\DATA_CONTRACTS.md';Start=1;End=120},@{Path='.\DCS-gRPC-lso-new\src\tasks\record_recovery.rs';Start=30;End=115},@{Path='.\DCS-gRPC-lso-new\src\tasks\record_recovery.rs';Start=830;End=950}); foreach($t in $targets){"FILE $($t.Path)"; $lines=Get-Content -LiteralPath $t.Path; for($i=$t.Start;$i -le [math]::Min($t.End,$lines.Count);$i++){ '{0,5}: {1}' -f $i,$lines[$i-1] }}`
> - Ran `rg -n "pub struct TelemetryQuality|pub struct HookObservation|pub struct WireEstimateEvidence|max_scoring|scoring_invalid|health|hook_samples|samples:" .\DCS-gRPC-lso-new\src\track.rs; $file=Get-ChildItem -LiteralPath '.\DCS-gRPC-lso-new\trap_records' -Filter '*.json' -File | Sort-Object LastWriteTime -Descending | Select-Object -First 1; $v2=Get-Content -LiteralPath $file.FullName -Raw | ConvertFrom-Json; [pscustomobject]@{File=$file.Name;TopLevel=($v2.PSObject.Properties.Name -join ', ');Telemetry=($v2.telemetry_quality.PSObject.Properties.Name -join ', ');Hook=($v2.hook_observation.PSObject.Properties.Name -join ', ')} | Format-List`
>
> </details>
</details>

“Schema v2/v3” is the version of the saved JSON recovery-report format. It is not the gRPC version and does not describe the sampling frequency.

For example, an older record begins with:

```json
{
  "schema_version": 2,
  "pilot_name": "...",
  "telemetry_quality": {
    "max_sample_gap_ms": 1069.2,
    "warning_samples": 213
  }
}
```

The current code always writes:

```json
"schema_version": 3
```

Schema v3 preserves the existing v2 fields but adds more diagnostic evidence, including:

- recording start and completion timestamps;
- touchdown DCS timestamp;
- LSO version, commit, and DCS-gRPC version;
- explicit grading availability;
- live telemetry health and separate pattern/groove invalid-sample counts;
- maximum gap specifically within the scoring segment;
- detailed timestamped hook samples;
- wire-crossing evidence, confidence, and rejection reason.

Why the distinction matters:

- The 15 examined files were produced by an older build that wrote schema v2.
- Their recorded gaps are genuine evidence that the system experienced 300–1,000 ms delays.
- However, the source currently in `DCS-gRPC-lso-new` writes schema v3 and contains additional changes.
- There is no schema-v3 live recording in the folder proving how this exact current source behaves inside DCS.

Therefore:

> The v2 records prove that the latency problem existed and reproduce its characteristics, but we still need a new schema-v3 live recovery to confirm whether the latest code improved, preserved, or worsened it.

The number itself does not improve telemetry. It mainly tells analysis tools which fields are available and approximately which generation of the application created the file.

Also, these are separate version systems:

- JSON `schema_version: 2/3`: recovery report format.
- SQLite migrations 1–5: database structure.
- DCS-gRPC 0.9.0/0.9.1: server/API software version.

> don't we have nv3 here D:\\\_DCS\DCS\_gRPC\_analyse\DCS-gRPC-lso-new\trap\_records

<details><summary>2 previous messages</summary>

> You may be right—the earlier count only inspected JSON files directly in `trap_records`, not necessarily every nested location or newly added file. I’ll rescan the directory recursively and report the exact schema counts and paths.
>
> <details><summary>Ran 2 commands</summary>
>
> - Ran `$root='.\DCS-gRPC-lso-new\trap_records'; $jsonFiles=Get-ChildItem -LiteralPath $root -Filter '*.json' -File -Recurse; $rows=foreach($file in $jsonFiles){try{$record=Get-Content -LiteralPath $file.FullName -Raw | ConvertFrom-Json; [pscustomobject]@{Schema=[string]$record.schema_version; Path=$file.FullName; Modified=$file.LastWriteTime}}catch{[pscustomobject]@{Schema='parse_error';Path=$file.FullName;Modified=$file.LastWriteTime}}}; 'COUNTS'; $rows | Group-Object Schema | Sort-Object Name | Select-Object Name,Count | Format-Table -AutoSize; 'SCHEMA_V3_FILES'; $rows | Where-Object {$_.Schema -eq '3'} | Sort-Object Modified -Descending | Format-Table -AutoSize; 'LATEST_FILES'; $rows | Sort-Object Modified -Descending | Select-Object -First 10 | Format-Table -AutoSize`
> - Ran `rg -n --glob '*.json' '"schema_version"\s*:\s*3' .\DCS-gRPC-lso-new\trap_records; $legacy=Get-ChildItem -LiteralPath '.\DCS-gRPC-lso-new\trap_records' -Filter '*.json' -File -Recurse | ForEach-Object {try{$r=Get-Content -LiteralPath $_.FullName -Raw|ConvertFrom-Json;if($null -eq $r.schema_version){$_}}catch{}} | Sort-Object LastWriteTime -Descending | Select-Object -First 3; 'LATEST_LEGACY_WITHOUT_SCHEMA'; $legacy | Select-Object Name,LastWriteTime,FullName | Format-List`
>
> </details>
</details>

I rescanned `DCS-gRPC-lso-new\trap_records` recursively. There are:

- **0 schema-v3 JSON files**
- **15 schema-v2 files**
- **219 older files without any `schema_version` field**

I also searched explicitly for `"schema_version": 3` and found no match.

The current source is configured to write schema v3, but no recovery generated by that current build exists in this directory. It may have been written to another output directory if the application was launched with a different `-o` path.

> can you recheck again i have added the last one form today

<details><summary>4 previous messages</summary>

> I’ll rescan the directory recursively, identify the newly added file by modification time, and verify its declared schema plus telemetry/gate contents.
>
> <details><summary>Ran a command</summary>
>
> - Ran `$root='.\DCS-gRPC-lso-new\trap_records'; $jsonFiles=Get-ChildItem -LiteralPath $root -Filter '*.json' -File -Recurse; $rows=foreach($file in $jsonFiles){try{$record=Get-Content -LiteralPath $file.FullName -Raw | ConvertFrom-Json; [pscustomobject]@{Schema=if($null -eq $record.schema_version){'legacy'}else{[string]$record.schema_version}; Path=$file.FullName; Name=$file.Name; Modified=$file.LastWriteTime; Record=$record}}catch{[pscustomobject]@{Schema='parse_error';Path=$file.FullName;Name=$file.Name;Modified=$file.LastWriteTime;Record=$null}}}; 'COUNTS'; $rows | Group-Object Schema | Sort-Object Name | Select-Object Name,Count | Format-Table -AutoSize; 'LATEST'; $rows | Sort-Object Modified -Descending | Select-Object -First 5 Schema,Modified,Name,Path | Format-List; 'V3_DETAILS'; foreach($row in ($rows | Where-Object {$_.Schema -eq '3'} | Sort-Object Modified -Descending)){ $record=$row.Record; [pscustomobject]@{Name=$row.Name; Modified=$row.Modified; Datums=@($record.datums).Count; Completeness=$record.telemetry_quality.completeness; Health=$record.telemetry_quality.health; HealthReason=$record.telemetry_quality.health_reason; MaxGapMs=$record.telemetry_quality.max_sample_gap_ms; MaxScoringGapMs=$record.telemetry_quality.max_scoring_sample_gap_ms; MaxSkewMs=$record.telemetry_quality.max_skew_ms; Warnings=$record.telemetry_quality.warning_samples; Invalid=$record.telemetry_quality.invalid_samples; ScoringInvalid=$record.telemetry_quality.scoring_invalid_samples; GradingAvailability=$record.grading_availability; PassGrade=$record.pass_grade; Outcome=$record.outcome; LsoVersion=$record.lso_version; LsoCommit=$record.lso_commit; DcsGrpcVersion=$record.dcs_grpc_version} | Format-List; 'GATES'; $record.gate_deviations | ConvertTo-Json -Depth 6 }`
>
> </details>
>
> The rescan now finds three schema-v3 reports, so my previous statement is no longer current. I’m aggregating those three separately; their first-level fields already show that the latest build still has roughly 0.95–1.04 second scoring gaps and invalid gates.
>
> <details><summary>Ran 7 commands</summary>
>
> - Ran `$files=Get-ChildItem -LiteralPath '.\DCS-gRPC-lso-new\trap_records' -Filter '*.json' -File -Recurse; $records=foreach($file in $files){try{$r=Get-Content -LiteralPath $file.FullName -Raw|ConvertFrom-Json;if($r.schema_version -eq 3){[pscustomobject]@{File=$file;Data=$r}}}catch{}}; $all=@($records|ForEach-Object{$_.Data.datums}); $groove=@($all|Where-Object{$_.x -ge -200 -and $_.x -le 1389}); function Summary($name,$items){$gaps=@($items|ForEach-Object{[double]$_.sample_gap_ms}|Sort-Object);$n=$gaps.Count;if($n -eq 0){return};[pscustomobject]@{Scope=$name;Samples=$n;P50=[math]::Round($gaps[[math]::Floor(($n-1)*.5)],1);P90=[math]::Round($gaps[[math]::Floor(($n-1)*.9)],1);P95=[math]::Round($gaps[[math]::Floor(($n-1)*.95)],1);P99=[math]::Round($gaps[[math]::Floor(($n-1)*.99)],1);Over300=@($gaps|Where-Object{$_ -gt 300}).Count;Over1000=@($gaps|Where-Object{$_ -gt 1000}).Count;Invalid=@($items|Where-Object{-not $_.telemetry_valid}).Count}}; 'GAP_SUMMARY'; Summary 'all_v3' $all | Format-List; Summary 'groove_v3' $groove | Format-List; 'ALIGNMENTS'; $all|Group-Object alignment|Sort-Object Name|Select-Object Name,Count|Format-Table -AutoSize; 'GATE_QUALITY'; $records|ForEach-Object{$_.Data.gate_deviations.three_quarter_quality.status;$_.Data.gate_deviations.half_quality.status;$_.Data.gate_deviations.quarter_quality.status}|Group-Object|Select-Object Name,Count|Format-Table -AutoSize; 'BUFFER_FIELDS'; foreach($row in $records){[pscustomobject]@{Name=$row.File.Name;Datums=@($row.Data.datums).Count;Dropped=$row.Data.telemetry_quality.dropped_samples;Reasons=($row.Data.telemetry_quality.reasons -join ',');PatternInvalid=$row.Data.telemetry_quality.pattern_invalid_samples;ScoringInvalid=$row.Data.telemetry_quality.scoring_invalid_samples}} | Format-Table -AutoSize`
> - Ran `$files=Get-ChildItem -LiteralPath '.\DCS-gRPC-lso-new\trap_records' -Filter '*.json' -File -Recurse; $records=foreach($file in $files){try{$r=Get-Content -LiteralPath $file.FullName -Raw|ConvertFrom-Json;if($r.schema_version -eq 3){[pscustomobject]@{File=$file;Data=$r}}}catch{}}; $all=@($records|ForEach-Object{$_.Data.datums}); $groove=@($all|Where-Object{$_.x -ge -200 -and $_.x -le 1389}); function Summary($name,$items){$gaps=@($items|ForEach-Object{[double]$_.sample_gap_ms}|Sort-Object);$n=$gaps.Count;if($n -eq 0){return};[pscustomobject]@{Scope=$name;Samples=$n;P50=[math]::Round($gaps[[math]::Floor(($n-1)*.5)],1);P90=[math]::Round($gaps[[math]::Floor(($n-1)*.9)],1);P95=[math]::Round($gaps[[math]::Floor(($n-1)*.95)],1);P99=[math]::Round($gaps[[math]::Floor(($n-1)*.99)],1);Over300=@($gaps|Where-Object{$_ -gt 300}).Count;Over1000=@($gaps|Where-Object{$_ -gt 1000}).Count;Invalid=@($items|Where-Object{-not $_.telemetry_valid}).Count}}; 'GAP_SUMMARY'; Summary 'all_v3' $all | Format-List; Summary 'groove_v3' $groove | Format-List; 'ALIGNMENTS'; $all|Group-Object alignment|Sort-Object Name|Select-Object Name,Count|Format-Table -AutoSize; 'GATE_QUALITY'; $records|ForEach-Object{$_.Data.gate_deviations.three_quarter_quality.status;$_.Data.gate_deviations.half_quality.status;$_.Data.gate_deviations.quarter_quality.status}|Group-Object|Select-Object Name,Count|Format-Table -AutoSize; 'BUFFER_FIELDS'; $bufferRows=foreach($row in $records){[pscustomobject]@{Name=$row.File.Name;Datums=@($row.Data.datums).Count;Dropped=$row.Data.telemetry_quality.dropped_samples;Reasons=($row.Data.telemetry_quality.reasons -join ',');PatternInvalid=$row.Data.telemetry_quality.pattern_invalid_samples;ScoringInvalid=$row.Data.telemetry_quality.scoring_invalid_samples}}; $bufferRows | Format-Table -AutoSize`
> - Ran `rg -n "MAX_TRACK_SAMPLES|BufferLimit|dropped_samples" .\DCS-gRPC-lso-new\src\track.rs .\DCS-gRPC-lso-new\docs; $files=Get-ChildItem -LiteralPath '.\DCS-gRPC-lso-new\trap_records' -Filter '*.json' -File -Recurse; foreach($file in $files){try{$r=Get-Content -LiteralPath $file.FullName -Raw|ConvertFrom-Json;if($r.schema_version -eq 3){[pscustomobject]@{Name=$file.Name;Completeness=$r.telemetry_quality.completeness;Dropped=$r.telemetry_quality.dropped_samples;Reasons=($r.telemetry_quality.reasons -join '|');PatternInvalid=$r.telemetry_quality.pattern_invalid_samples;ScoringInvalid=$r.telemetry_quality.scoring_invalid_samples;MaxGap=[math]::Round([double]$r.telemetry_quality.max_sample_gap_ms,1);MaxScoringGap=[math]::Round([double]$r.telemetry_quality.max_scoring_sample_gap_ms,1)}}}catch{}} | Format-List`
> - Ran `rg -n "MAX_TRACK_SAMPLES|BufferLimit|dropped_samples" .\DCS-gRPC-lso-new\src\track.rs .\DCS-gRPC-lso-new\docs; $files=Get-ChildItem -LiteralPath '.\DCS-gRPC-lso-new\trap_records' -Filter '*.json' -File -Recurse; $rows=foreach($file in $files){try{$r=Get-Content -LiteralPath $file.FullName -Raw|ConvertFrom-Json;if($r.schema_version -eq 3){[pscustomobject]@{Name=$file.Name;Completeness=$r.telemetry_quality.completeness;Dropped=$r.telemetry_quality.dropped_samples;Reasons=($r.telemetry_quality.reasons -join '|');PatternInvalid=$r.telemetry_quality.pattern_invalid_samples;ScoringInvalid=$r.telemetry_quality.scoring_invalid_samples;MaxGap=[math]::Round([double]$r.telemetry_quality.max_sample_gap_ms,1);MaxScoringGap=[math]::Round([double]$r.telemetry_quality.max_scoring_sample_gap_ms,1)}}}catch{}}; $rows | Format-List`
> - Ran `$lines=Get-Content -LiteralPath '.\DCS-gRPC-lso-new\src\track.rs'; foreach($range in @(@(1315,1345),@(1405,1435),@(180,215))){"RANGE $($range[0])-$($range[1])";for($i=$range[0];$i -le $range[1];$i++){ '{0,5}: {1}' -f $i,$lines[$i-1] }}`
> - Ran `rg -n "MAX_HOOK_EVIDENCE|MAX_EVENT_EVIDENCE" .\DCS-gRPC-lso-new\src\track.rs; $files=Get-ChildItem -LiteralPath '.\DCS-gRPC-lso-new\trap_records' -Filter '*.json' -File -Recurse; $rows=foreach($file in $files){try{$r=Get-Content -LiteralPath $file.FullName -Raw|ConvertFrom-Json;if($r.schema_version -eq 3){[pscustomobject]@{Name=$file.Name;HookTimeline=@($r.hook_observation.timeline).Count;Events=@($r.events).Count;HookSuccess=$r.hook_observation.successful_samples;HookTimeout=$r.hook_observation.timeout_samples;HookError=$r.hook_observation.error_samples;HookStale=$r.hook_observation.stale_samples;Dropped=$r.telemetry_quality.dropped_samples}}}catch{}}; $rows|Format-Table -AutoSize`
> - Ran `$files=Get-ChildItem -LiteralPath '.\DCS-gRPC-lso-new\trap_records' -Filter '*.json' -File -Recurse; foreach($file in $files){try{$r=Get-Content -LiteralPath $file.FullName -Raw|ConvertFrom-Json;if($r.schema_version -eq 3){[pscustomobject]@{Name=$file.Name;Timeline=@($r.hook_observation.timeline).Count;Successful=[int]$r.hook_observation.successful_samples;Timeout=[int]$r.hook_observation.timeout_samples;Error=[int]$r.hook_observation.error_samples;Stale=[int]$r.hook_observation.stale_samples;TotalStatuses=([int]$r.hook_observation.successful_samples+[int]$r.hook_observation.timeout_samples+[int]$r.hook_observation.error_samples+[int]$r.hook_observation.stale_samples);Dropped=[int]$r.telemetry_quality.dropped_samples;Completeness=$r.telemetry_quality.completeness}|Format-List}}catch{}}`
>
> </details>
</details>

You are correct. After rescanning recursively, `trap_records` now contains:

- **3 schema-v3 reports**
- **23 schema-v2 reports**
- **237 legacy reports** without `schema_version`

The three v3 reports are the newest files, generated with DCS-gRPC 0.9.1.

## What the v3 reports prove

Across the three v3 records:

| Scope | Samples | Median gap | P90 gap | Over 300 ms |
|---|---:|---:|---:|---:|
| Complete recordings | 2,586 | 101.5 ms | 542.7 ms | 491 — 19.0% |
| Groove window | 769 | 99.1 ms | 567.7 ms | 152 — 19.8% |

Additional findings:

- Maximum scoring gaps are **949–966 ms**.
- Alignment is 2,565 `direct`, 21 `invalid`, and **zero extrapolated**.
- Only **1 of 9 grading gates** is valid.
- All three passes are technically unavailable and graded `Incomplete`.

Therefore, schema v3 confirms the central finding: the latest recorder still experiences substantial whole-loop delays during the groove. The extrapolation logic is not addressing these delays because carrier and aircraft normally arrive with matching timestamps—the complete RPC pair is simply late.

## Newly discovered v3 problem

All three reports have:

```json
"completeness": "buffer_limit"
```

This comes from the hook-evidence timeline reaching its fixed limit of 512 entries:

| Report | Hook observations | Retained | Dropped |
|---|---:|---:|---:|
| Nazgul91 | 762 | 512 | 250 |
| ERGON pass 1 | 771 | 512 | 259 |
| ERGON pass 2 | 623 | 512 | 111 |

When this diagnostic hook buffer fills, the code marks the **entire recovery technically incomplete**, which forces:

```text
grading_availability = unavailable_technical
pass_grade = Incomplete
```

This is a separate major bug. A diagnostic hook-history overflow should not automatically invalidate otherwise usable flight telemetry. Additionally, the buffer retains the first 512 hook samples and discards later ones—the exact near-deck samples that are most valuable.

The appropriate focused fix would be a bounded rolling buffer that preserves the newest/final-window hook evidence, while tracking older discarded diagnostic samples without invalidating the entire pass.

So the report needs two corrections:

1. Schema-v3 live evidence is now available.
2. That evidence confirms the cadence problem and exposes the hook-buffer grading failure.

> can you updated the amalysis document with last telemetry evidenc schema v3

<details><summary>4 previous messages</summary>

> I’ll update the existing report, replacing the obsolete “no v3 evidence” statement with a separate schema-v3 analysis, preserving v2 as historical comparison, and adding the newly proven hook-buffer failure as its own prioritized issue.
>
> <details><summary>Edited a file, ran 3 commands</summary>
>
> - Ran `$files=Get-ChildItem -LiteralPath '.\DCS-gRPC-lso-new\trap_records' -Filter '*.json' -File -Recurse; $records=foreach($file in $files){try{$data=Get-Content -LiteralPath $file.FullName -Raw|ConvertFrom-Json;if($data.schema_version -in @(2,3) -and $data.telemetry_quality){[pscustomobject]@{File=$file;Data=$data;Schema=[int]$data.schema_version}}}catch{}}; function MakeSummary($schema,$scopeName,$items){$gaps=@($items|ForEach-Object{[double]$_.sample_gap_ms}|Sort-Object);$n=$gaps.Count;if($n -eq 0){return};[pscustomobject]@{Schema=$schema;Scope=$scopeName;Samples=$n;P50=[math]::Round($gaps[[math]::Floor(($n-1)*.5)],1);P90=[math]::Round($gaps[[math]::Floor(($n-1)*.9)],1);P95=[math]::Round($gaps[[math]::Floor(($n-1)*.95)],1);P99=[math]::Round($gaps[[math]::Floor(($n-1)*.99)],1);Over300=@($gaps|Where-Object{$_ -gt 300}).Count;Over1000=@($gaps|Where-Object{$_ -gt 1000}).Count;Invalid=@($items|Where-Object{-not $_.telemetry_valid}).Count}}; foreach($schema in @(2,3)){$set=@($records|Where-Object{$_.Schema -eq $schema});$all=@($set|ForEach-Object{$_.Data.datums});$groove=@($all|Where-Object{$_.x -ge -200 -and $_.x -le 1389});"SCHEMA $schema records=$($set.Count)";MakeSummary $schema 'all' $all|Format-List;MakeSummary $schema 'groove' $groove|Format-List;'alignment';$all|Group-Object alignment|Sort-Object Name|Select-Object Name,Count|Format-Table -AutoSize;'gates';$set|ForEach-Object{$_.Data.gate_deviations.three_quarter_quality.status;$_.Data.gate_deviations.half_quality.status;$_.Data.gate_deviations.quarter_quality.status}|Group-Object|Select-Object Name,Count|Format-Table -AutoSize}`
> - Ran `$path='.\DCS_GRPC_LSO_RECOVERY_ANALYSIS.md'; $lines=Get-Content -LiteralPath $path; foreach($range in @(@(1,25),@(90,135),@(230,255),@(300,325))){"RANGE $($range[0])-$($range[1])";for($i=$range[0];$i -le [math]::Min($range[1],$lines.Count);$i++){ '{0,5}: {1}' -f $i,$lines[$i-1] }}`
> - Ran `$files=Get-ChildItem -LiteralPath '.\DCS-gRPC-lso-new\trap_records' -Filter '*.json' -File -Recurse; foreach($schema in @(2,3)){$pairs=[System.Collections.Generic.List[object]]::new();foreach($file in $files){try{$r=Get-Content -LiteralPath $file.FullName -Raw|ConvertFrom-Json;if($r.schema_version -ne $schema){continue};$prev=$null;foreach($d in $r.datums){if($null-ne$prev){$pairs.Add([pscustomobject]@{Dcs=[double]$d.sample_gap_ms;Wall=([double]$d.plane_received_unix_ms-[double]$prev.plane_received_unix_ms)})};$prev=$d}}catch{}};$set=@($pairs|Where-Object{$_.Dcs -gt 300});$wall=@($set.Wall|Sort-Object);$dcs=@($set.Dcs|Sort-Object);$n=$set.Count;[pscustomobject]@{Schema=$schema;N=$n;WallMedian=[math]::Round($wall[[math]::Floor(($n-1)*.5)],1);DcsMedian=[math]::Round($dcs[[math]::Floor(($n-1)*.5)],1);WallP90=[math]::Round($wall[[math]::Floor(($n-1)*.9)],1);DcsP90=[math]::Round($dcs[[math]::Floor(($n-1)*.9)],1)}}`
> - Wrote `./DCS_GRPC_LSO_RECOVERY_ANALYSIS.md` (+45 -11)
>
> </details>
>
> The report is updated. The v3 data confirms the cadence diagnosis and adds a separate critical failure: every v3 pass was forced incomplete by the 512-entry hook-history cap. I’m doing a consistency pass now to ensure the old “no v3 evidence” wording and obsolete counts are completely removed.
>
> <details><summary>Ran a command</summary>
>
> - Ran ````$path='.\DCS_GRPC_LSO_RECOVERY_ANALYSIS.md'; 'OBSOLETE_TEXT_CHECK'; rg -n "15 saved|no schema-v3|captured evidence is v2|11,055|10,980|10 of 45|Live DCS test.*Not available" $path; 'UPDATED_SECTIONS'; rg -n "four grading-critical|Schema-v3 evidence|Schema-v2 historical|Hook-evidence overflow|2,586|512-entry|Persisted schema-v3" $path; $text=Get-Content -LiteralPath $path -Raw; [pscustomobject]@{Path=(Resolve-Path $path).Path;Bytes=(Get-Item $path).Length;Lines=($text -split "`n").Count;CodeFences=([regex]::Matches($text,'```')).Count} | Format-List````
>
> </details>
</details>

Updated [DCS_GRPC_LSO_RECOVERY_ANALYSIS.md](./DCS_GRPC_LSO_RECOVERY_ANALYSIS.md).

Changes include:

- Analysis of all three schema-v3 reports.
- Updated schema-v2 totals for historical comparison.
- V3 cadence statistics for complete records and groove samples.
- Confirmation that approximately 20% of v3 groove samples exceed 300 ms.
- Gate-quality results: only 1 of 9 v3 gates is valid.
- New P0 finding covering the 512-entry hook-evidence overflow.
- Explanation of why this overflow forces every v3 recovery to `Incomplete`.
- Recommended rolling-buffer and grading-availability correction.
- Updated validation limitations: v3 evidence exists, but `lso_commit` is `unknown`.

No production code was modified.

> for this part&#x20;
> The new server snapshot is version **0.9.1**, not 0.9.0. Its `GetTransform`/mission-queue hot path and its `StreamUnits` implementation are behaviorally unchanged from the 0.8.1 origin snapshot. Therefore, this comparison does not support blaming the gaps on a new 0.9.x transform implementation. It also does not support calling 0.9.1 optimized for this recovery workload: there is no atomic recovery snapshot, latency/load benchmark, or test of the DCS Lua queue under contention. &#x20;
>
> No production fix was made in this analysis &#x20;
> can you provide possible fix in the document

<details><summary>2 previous messages</summary>

> I’ll place a concrete server-side remedy directly after that conclusion, so the document explains both why 0.9.1 is not proven to be the regression and how to improve the recovery workload without redesigning the whole server.
>
> <details><summary>Edited a file, ran a command</summary>
>
> - Wrote `./DCS_GRPC_LSO_RECOVERY_ANALYSIS.md` (+12 -0)
> - Ran ````$path='.\DCS_GRPC_LSO_RECOVERY_ANALYSIS.md'; $lines=Get-Content -LiteralPath $path; for($i=15;$i -le 45;$i++){ '{0,5}: {1}' -f $i,$lines[$i-1] }; $text=Get-Content -LiteralPath $path -Raw; [pscustomobject]@{Lines=($text -split "`n").Count;CodeFences=([regex]::Matches($text,'```')).Count;AtomicFixMentions=([regex]::Matches($text,'GetRecoverySnapshot')).Count} | Format-List````
>
> </details>
</details>

Updated [DCS_GRPC_LSO_RECOVERY_ANALYSIS.md](./DCS_GRPC_LSO_RECOVERY_ANALYSIS.md).

The section now includes a concrete server-side proposal:

- Add one atomic `GetRecoverySnapshot` RPC.
- Read carrier, aircraft, common DCS time, and optional hook value in one Lua callback.
- Reduce CATOBAR load from approximately 24 to 10 mission requests per second.
- Add queue-wait and Lua-execution metrics.
- Protect recovery requests from `StreamUnits` bursts.
- Skip late ticks instead of accumulating requests or inventing positions.
- Validate against explicit latency and fairness acceptance criteria.

It also defines the limited implementation scope across protobuf, Rust, Lua, stubs, and the LSO client. No production code was changed.

> ok i think this is main update to do , that will potentially impact all other potential fix ?

Yes. The atomic `GetRecoverySnapshot` is the foundational telemetry update, so it should happen before tuning most groove or grading behavior.

| Existing issue | Effect of atomic snapshot |
|---|---|
| Carrier/aircraft timestamp skew | Eliminated by using one DCS timestamp |
| Position extrapolation | Probably becomes unnecessary for live recovery |
| Separate hook timing | Eliminated if hook value is included |
| Server queue load | Reduced from ~24 to 10 requests/second |
| Missing/stale gates | Likely improved, but must be measured |
| Carrier EMA affecting grades | Not fixed |
| Hook 512-entry overflow | Not fixed |
| Groove time ignored | Not fixed |
| Three-point-only grading | Not fixed |
| Touch-and-go rule contradiction | Not fixed |
| DCS frame stalls | Not eliminated, but accurately exposed |

Recommended order:

1. **Fix the hook buffer first.**
   This is small and independent. Currently every schema-v3 recovery becomes technically incomplete, which would prevent clean validation of the snapshot change.

2. **Add regression tests and latency instrumentation.**
   Capture queue wait, Lua execution, total RPC latency, missed ticks, and common timestamp.

3. **Implement the atomic snapshot as one isolated vertical change.**
   Additive protobuf RPC, Rust handler, Lua handler, stubs update, and one LSO client method. Existing clients and generic APIs remain unchanged.

4. **Collect new live recoveries.**
   Compare current two-RPC mode against atomic mode using the same mission and clients.

5. **Then address grading geometry.**
   Remove EMA from scoring, quarantine unhealthy samples, and verify gate capture with the improved data.

6. **Only afterward tune groove realism.**
   Continuous corrections, groove time, AoA, thresholds, and notation should be calibrated using the stabilized telemetry.

So yes: it potentially changes the evidence behind many later fixes. Implementing grading or gate tuning before stabilizing acquisition risks tuning the system around telemetry defects. However, it will not automatically solve the independent hook-buffer, grading-policy, or continuous-groove problems.

> can you add this last analysis in the doc , may be at the end as annex

<details><summary>2 previous messages</summary>

> I’ll add it as a final annex with an impact matrix and the recommended implementation order, keeping it separate from the forensic findings so it can guide the repair work step by step.
>
> <details><summary>Edited a file, ran a command</summary>
>
> - Wrote `./DCS_GRPC_LSO_RECOVERY_ANALYSIS.md` (+65 -0)
> - Ran ````$path='.\DCS_GRPC_LSO_RECOVERY_ANALYSIS.md'; rg -n "^## Annex A|^### Recommended|^#### A[1-6]|Change-management boundary|atomic snapshot is the primary" $path; $text=Get-Content -LiteralPath $path -Raw; [pscustomobject]@{Path=(Resolve-Path $path).Path;Bytes=(Get-Item $path).Length;Lines=($text -split "`n").Count;CodeFences=([regex]::Matches($text,'```')).Count} | Format-List````
>
> </details>
</details>

Added “Annex A — Dependency and implementation order for the atomic snapshot” to [DCS_GRPC_LSO_RECOVERY_ANALYSIS.md](./DCS_GRPC_LSO_RECOVERY_ANALYSIS.md).

The annex includes:

- An impact matrix for every identified issue.
- What the atomic snapshot fixes, improves, or does not affect.
- Six incremental implementation stages with acceptance criteria.
- The hook-buffer fix as a prerequisite.
- Controlled A/B live-validation requirements.
- A change-management boundary keeping telemetry, geometry, and grading changes in separate commits.

No production code was modified.
