# Analyze recovery gate failures

> can you analyzes all data and logs in C:\Users\thierry\Documents\GitHub\sevenfifty777\DCS-gRPC-lso\trap\_records\recovery\_snapshot
> they are 2 recovery test i made today
> can you give reason of insufficientgates

<details><summary>13 previous messages</summary>

> I’ll trace both recovery snapshots against the current gate-selection and grading code, then separate what the telemetry proves from what the implementation inferred. I’m using the graphify skill to navigate the codebase relationships, and I’ll consult the prior recovery-analysis notes because `InsufficientGates` has depended on acquisition timing and retained hook evidence before.
>
> <details><summary>Ran 3 commands</summary>
>
> - Ran ``$skill = 'C:\Users\thierry\.agents\skills\graphify\SKILL.md'; Get-Content -LiteralPath $skill -Raw; Write-Output "`n--- MEMORY HITS ---"; rg -n -i "InsufficientGates|gate|recovery snapshot|GetRecoverySnapshot|schema_version|hook evidence|record_recovery" 'C:\Users\thierry\.codex\memories\MEMORY.md'``
> - Ran `$g = 'C:\Users\thierry\.agents\skills\graphify\SKILL.md'; (Get-Content -LiteralPath $g).Count; Test-Path '.\graphify-out\graph.json'; Write-Output '--- SNAPSHOT FILES ---'; rg --files '.\trap_records\recovery_snapshot'; Write-Output '--- GIT/GRAPH STATE ---'; git status --short; if (Test-Path '.\graphify-out\graph.json') { Get-Item '.\graphify-out\graph.json' | Select-Object FullName,Length,LastWriteTime }`
> - Ran `$skill = 'C:\Users\thierry\.agents\skills\graphify\SKILL.md'; Get-Content -LiteralPath $skill | Select-Object -First 260; Write-Output '--- SKILL 261-520 ---'; Get-Content -LiteralPath $skill | Select-Object -Skip 260 -First 260; Write-Output '--- SKILL 521-755 ---'; Get-Content -LiteralPath $skill | Select-Object -Skip 520; Write-Output '--- QUERY REFERENCE ---'; Get-Content -LiteralPath 'C:\Users\thierry\.agents\skills\graphify\references\query.md' -Raw; Write-Output '--- MEMORY 100-132 ---'; $m='C:\Users\thierry\.codex\memories\MEMORY.md'; $lines=Get-Content -LiteralPath $m; for($i=99;$i -lt 132;$i++){ '{0}:{1}' -f ($i+1),$lines[$i] }`
>
> </details>
>
> I found exactly two JSON records, two paired trapsheet images, two pattern images, two compressed ACMI logs, and one SQLite database. The repository graph is older than today’s tests, so I’ll use it only to locate the grading path and verify every conclusion against the current source and today’s raw artifacts.
>
> <details><summary>Ran a command</summary>
>
> - Ran `$py = Get-Content -LiteralPath '.\graphify-out\.graphify_python' -Raw; & $py -m graphify reflect --if-stale; if (Test-Path '.\graphify-out\reflections\LESSONS.md') { Get-Content -LiteralPath '.\graphify-out\reflections\LESSONS.md' -Raw }; @'
> import json,re
> from pathlib import Path
> data=json.loads(Path('graphify-out/graph.json').read_text(encoding='utf-8'))
> vocab=set()
> for n in data['nodes']:
>     for c in re.findall(r'[^\W\d_]+', n.get('label','') or '', re.UNICODE):
>         parts=re.findall(r'[A-Z]+(?=[A-Z][a-z])|[A-Z]?[a-z]+|[A-Z]+',c) or [c]
>         for p in parts:
>             t=p.lower()
>             if 3 <= len(t) <= 30: vocab.add(t)
> Path('graphify-out/.vocab.txt').write_text('\n'.join(sorted(vocab)),encoding='utf-8')
> print('vocab:',len(vocab),'tokens')
> '@ | & $py -; Write-Output '--- MATCHING VOCAB ---'; rg -n -i "gate|insufficient|grade|recovery|sample|pass|track|datum|groove" '.\graphify-out\.vocab.txt'`
>
> </details>
>
> The graph’s saved interpreter path is stale, so its refresh helper cannot run. The graph itself is readable; I’ll use the documented fallback for its traversal and continue with current-source inspection. Query expansion from the graph vocabulary is: `[gate, gates, grade, grades, recovery, sample, pass, track, datum, datums, groove]`.
>
> <details><summary>Ran 10 commands</summary>
>
> - Ran `$g='C:\Users\thierry\.agents\skills\graphify\SKILL.md'; Get-Content -LiteralPath $g | Select-Object -Skip 240 -First 260; Write-Output '--- 501-END ---'; Get-Content -LiteralPath $g | Select-Object -Skip 500; Write-Output '--- GRAPH TOOLS ---'; Get-Command graphify,python,python3 -ErrorAction SilentlyContinue | Select-Object Name,Source; Write-Output '--- GRAPH METADATA ---'; Get-Content -LiteralPath '.\graphify-out\graph.json' -TotalCount 20; Write-Output '--- MEMORY ROLLOUT ---'; Get-Content -LiteralPath 'C:\Users\thierry\.codex\memories\rollout_summaries\2026-08-31T22-53-27-B61I-dcs_lso_telemetry_analysis_and_atomic_snapshot_plan.md' -Raw`
> - Ran `graphify query 'gate gates grade grades recovery sample pass track datum datums groove' --budget 3000`
> - Ran `$d = Get-Content -LiteralPath '.\graphify-out\graph.json' -Raw | ConvertFrom-Json; $terms = @('gate','gates','grade','grades','recovery','sample','pass','track','datum','datums','groove'); $matches = foreach($n in $d.nodes){ $label=[string]$n.label; $score=0; foreach($t in $terms){ if($label.ToLowerInvariant().Contains($t)){$score++} }; if($score -gt 0){[pscustomobject]@{score=$score;id=$n.id;label=$label;source_file=$n.source_file;source_location=$n.source_location}} }; $matches | Sort-Object score -Descending | Select-Object -First 25 | Format-Table -AutoSize -Wrap; Write-Output '--- LINK KEYS ---'; $d.links | Select-Object -First 3 | ConvertTo-Json -Depth 6`
> - Ran `rg -n -i -C 8 "InsufficientGates|insufficient_gates|gate_samples|grade_from_gates|GateDatum|gate" src tests docs Cargo.toml`
> - Ran `rg -n -i -C 12 "InsufficientGates" src docs tests; rg -n -C 8 "all_valid\(|valid_gate|GateStatus::|GateQuality|capture_gate|finalize_gate|at_three_quarter_nm|at_half_nm|at_quarter_nm" src/track.rs src/tasks/record_recovery.rs src/grading.rs`
> - Ran `$files = Get-ChildItem -LiteralPath '.\trap_records\recovery_snapshot' -Filter '*.json' | Sort-Object Name; foreach($f in $files){ $j=Get-Content -LiteralPath $f.FullName -Raw | ConvertFrom-Json; Write-Output "--- $($f.Name) ($($f.Length) bytes) ---"; $j.PSObject.Properties.Name -join ', '; foreach($p in $j.PSObject.Properties){ $v=$p.Value; $kind=if($null -eq $v){'null'}elseif($v -is [System.Array]){"array[$($v.Count)]"}elseif($v -is [System.Management.Automation.PSCustomObject]){'object'}else{"$($v.GetType().Name): $v"}; Write-Output "$($p.Name): $kind" } }`
> - Ran `$files = Get-ChildItem -LiteralPath '.\trap_records\recovery_snapshot' -Filter '*.json' | Sort-Object Name; foreach($f in $files){ $j=Get-Content -LiteralPath $f.FullName -Raw | ConvertFrom-Json; Write-Output "--- $($f.Name) ---"; [pscustomobject]@{grading=$j.grading;gate_deviations=$j.gate_deviations;telemetry_quality=$j.telemetry_quality;wire_estimation=$j.wire_estimation;hook_observation=$j.hook_observation;spot_zone=$j.spot_zone;events=$j.events} | ConvertTo-Json -Depth 12 }`
> - Ran `$files = Get-ChildItem -LiteralPath '.\trap_records\recovery_snapshot' -Filter '*.json' | Sort-Object Name; foreach($f in $files){ $j=Get-Content -LiteralPath $f.FullName -Raw | ConvertFrom-Json; $h=$j.hook_observation; $w=$j.wire_estimation; [pscustomobject]@{file=$f.Name;g3q=$j.gate_deviations.three_quarter_quality.status;g3q_reason=$j.gate_deviations.three_quarter_quality.reason;g_half=$j.gate_deviations.half_quality.status;g_half_reason=$j.gate_deviations.half_quality.reason;g_quarter=$j.gate_deviations.quarter_quality.status;g_quarter_reason=$j.gate_deviations.quarter_quality.reason;completeness=$j.telemetry_quality.completeness;health=$j.telemetry_quality.health;max_gap_ms=$j.telemetry_quality.max_sample_gap_ms;max_scoring_gap_ms=$j.telemetry_quality.max_scoring_sample_gap_ms;warning_samples=$j.telemetry_quality.warning_samples;invalid_samples=$j.telemetry_quality.invalid_samples;datums=$j.datums.Count;touchdown=$j.touchdown_time_dcs;wire_dcs=$j.wire_dcs;wire_est=$j.wire_estimated;wire_reason=$w.reason;hook_groove=$h.samples_in_groove;hook_final=$h.samples_in_final_window;hook_compacted=$h.compacted_samples;hook_state=$h.interpreted_state} | Format-List }`
> - Ran `$files=Get-ChildItem -LiteralPath '.\trap_records\recovery_snapshot' -Filter '*.json'|Sort-Object Name; foreach($f in $files){$j=Get-Content -LiteralPath $f.FullName -Raw|ConvertFrom-Json; Write-Output "--- $($f.Name) first/last datum ---"; $j.datums[0] | ConvertTo-Json -Depth 8; $j.datums[-1] | ConvertTo-Json -Depth 8}`
> - Ran `rg -n "GATE_THREE_QUARTER_NM|GATE_HALF_NM|GATE_QUARTER_NM|GATE_BUFFER_WINDOW_S|SAMPLE_GAP_WARNING_MS|MAX_EXTRAPOLATION_MS" src/track.rs src/telemetry.rs; $lines=Get-Content -LiteralPath '.\src\track.rs'; foreach($start in @(780,825,1468,1475,1513,1135)){Write-Output "--- track.rs $($start+1) ---"; for($i=$start;$i -lt [Math]::Min($start+130,$lines.Count);$i++){ '{0}:{1}' -f ($i+1),$lines[$i] }}`
>
> </details>
>
> The immediate reason is now clear in the records: the first test captured zero valid gates; the second captured only the ¾‑NM gate. Every missing gate is explicitly marked `invalid: stale_gate_bracket`. I’m now reconstructing the exact before/after samples at 1,389 m, 926 m, and 463 m to determine whether those stale brackets came from real telemetry gaps or from the gate state machine choosing/resetting samples incorrectly.
>
> <details><summary>Ran 15 commands</summary>
>
> - Ran `$gates=@([pscustomobject]@{name='3/4';x=1389.0},[pscustomobject]@{name='1/2';x=926.0},[pscustomobject]@{name='1/4';x=463.0}); $files=Get-ChildItem -LiteralPath '.\trap_records\recovery_snapshot' -Filter '*.json'|Sort-Object Name; foreach($f in $files){$j=Get-Content -LiteralPath $f.FullName -Raw|ConvertFrom-Json; Write-Output "=== $($f.Name) ==="; $d=@($j.datums); foreach($gate in $gates){Write-Output "-- $($gate.name) gate $($gate.x)m adjacent crossings --"; for($i=1;$i -lt $d.Count;$i++){if([double]$d[$i-1].x -gt $gate.x -and [double]$d[$i].x -le $gate.x){$prevInbound=if($i-ge 2){[double]$d[$i-1].x -lt [double]$d[$i-2].x}else{$false};$curInbound=[double]$d[$i].x -lt [double]$d[$i-1].x;[pscustomobject]@{i=$i;seq0=$d[$i-1].observation_sequence;t0=$d[$i-1].time;x0=[math]::Round($d[$i-1].x,2);seq1=$d[$i].observation_sequence;t1=$d[$i].time;x1=[math]::Round($d[$i].x,2);dcs_gap_ms=[math]::Round((([double]$d[$i].time-[double]$d[$i-1].time)*1000),3);reported_gap0=[math]::Round($d[$i-1].sample_gap_ms,3);reported_gap1=[math]::Round($d[$i].sample_gap_ms,3);rtt0=[math]::Round($d[$i-1].request_round_trip_ms,3);rtt1=[math]::Round($d[$i].request_round_trip_ms,3);prev_inbound=$prevInbound;cur_inbound=$curInbound;valid0=$d[$i-1].telemetry_valid;valid1=$d[$i].telemetry_valid}|Format-List}}}}`
> - Ran `$lines=Get-Content -LiteralPath '.\src\track.rs'; for($i=640;$i -lt 805;$i++){ '{0}:{1}' -f ($i+1),$lines[$i] }; Write-Output '--- telemetry sample gap creation ---'; rg -n -C 10 "sample_gap_ms|source_age_ms|observation_sequence|request_round_trip_ms" src/tasks/record_recovery.rs src/telemetry.rs`
> - Ran `$lines=Get-Content -LiteralPath '.\src\tasks\record_recovery.rs'; for($i=330;$i -lt 530;$i++){ '{0}:{1}' -f ($i+1),$lines[$i] }; Write-Output '--- interval setup ---'; rg -n -C 10 "interval\(|MissedTickBehavior|set_missed_tick_behavior|recovery_snapshot_timeout|AcquisitionMode" src/tasks/record_recovery.rs src/config.rs src/*.rs`
> - Ran `$lines=Get-Content -LiteralPath '.\src\track.rs'; for($i=560;$i -lt 650;$i++){ '{0}:{1}' -f ($i+1),$lines[$i] }; Write-Output '--- interval helper ---'; rg -n -C 15 "fn recovery_interval|recovery_interval" src`
> - Ran `rg -n -C 8 "recovery_snapshot_timeout|RECOVERY.*TIMEOUT|snapshot timeout|timeout_ms" src README.md docs config* .env.example Cargo.toml`
> - Ran `rg -n -C 18 "struct RecoveryClient|impl RecoveryClient|get_snapshot" src/client.rs src/client/*.rs src; rg -n -C 8 "timeout\(" src/client.rs src/client src`
> - Ran `Get-Command sqlite3 -ErrorAction SilentlyContinue | Select-Object Name,Source; sqlite3 '.\trap_records\recovery_snapshot\lso.db' ".tables"; sqlite3 '.\trap_records\recovery_snapshot\lso.db' ".schema"`
> - Ran `Get-Command py,strings -ErrorAction SilentlyContinue | Select-Object Name,Source; rg -n "RecoveryDb|lso.db|all_passes|passes" src/commands src/db.rs README.md docs | Select-Object -First 200; Get-Item '.\trap_records\recovery_snapshot\lso.db' | Select-Object FullName,Length,LastWriteTime`
> - Ran `$names=@('System.Data.SQLite','Microsoft.Data.Sqlite','Mono.Data.Sqlite'); foreach($n in $names){try{Add-Type -AssemblyName $n -ErrorAction Stop; Write-Output "loaded $n"}catch{Write-Output "missing $n"}}; [AppDomain]::CurrentDomain.GetAssemblies() | Where-Object {$_.FullName -match 'Sqlite|SQLite'} | Select-Object FullName,Location`
> - Ran `rg --files target | rg -i "(sqlite3|lso|dcs-grpc-lso)(\.exe)?$|rusqlite.*\.rlib$" | Select-Object -First 100; Get-ChildItem -LiteralPath '.\target\debug' -Filter '*.exe' -ErrorAction SilentlyContinue | Select-Object Name,Length,LastWriteTime`
> - Ran `& '.\target\debug\lso.exe' --help`
> - Ran `rg -a -n -o -i "s1788288229-g19-p5240-c5160-t[0-9]+|insufficient_gates|unavailable_technical|Ghost-72|stale_gate_bracket|LSO: GRADE:[^\x00]{0,100}" '.\trap_records\recovery_snapshot\lso.db'`
> - Ran `$files=Get-ChildItem -LiteralPath '.\trap_records\recovery_snapshot' -Filter '*.zip.acmi'|Sort-Object Name; foreach($f in $files){Write-Output "--- $($f.Name) ---"; tar -tf $f.FullName; tar -xOf $f.FullName | Select-Object -First 25}`
> - Ran `$files=Get-ChildItem -LiteralPath '.\trap_records\recovery_snapshot' -Filter '*.zip.acmi'|Sort-Object Name; foreach($f in $files){$times=@(); tar -xOf $f.FullName | ForEach-Object {if($_ -match '^#([0-9.]+)$'){$times += [double]$matches[1]}}; $gaps=@(); for($i=1;$i -lt $times.Count;$i++){$gaps += ($times[$i]-$times[$i-1])*1000}; $sorted=@($gaps|Sort-Object); $p50=$sorted[[math]::Floor(($sorted.Count-1)*0.5)];$p90=$sorted[[math]::Floor(($sorted.Count-1)*0.9)];$p95=$sorted[[math]::Floor(($sorted.Count-1)*0.95)];[pscustomobject]@{file=$f.Name;frames=$times.Count;first=$times[0];last=$times[-1];duration_s=[math]::Round($times[-1]-$times[0],3);median_gap_ms=[math]::Round($p50,3);p90_gap_ms=[math]::Round($p90,3);p95_gap_ms=[math]::Round($p95,3);max_gap_ms=[math]::Round(($sorted|Select-Object -Last 1),3);gaps_over_300=($gaps|Where-Object{$_ -gt 300}).Count;gaps_over_1000=($gaps|Where-Object{$_ -gt 1000}).Count}|Format-List; Write-Output 'top gaps:'; for($i=1;$i -lt $times.Count;$i++){ $gap=($times[$i]-$times[$i-1])*1000;if($gap -gt 800){[pscustomobject]@{from=$times[$i-1];to=$times[$i];gap_ms=[math]::Round($gap,3)}}}|Select-Object -First 20|Format-Table -AutoSize}`
> - Ran `$files=Get-ChildItem -LiteralPath '.\trap_records\recovery_snapshot' -Filter '*.zip.acmi'|Sort-Object Name; foreach($f in $files){$times=@(); tar -xOf $f.FullName | ForEach-Object {if($_ -match '^#([0-9.]+)$'){$times += [double]$matches[1]}}; $gaps=@(); $top=@(); for($i=1;$i -lt $times.Count;$i++){$gap=($times[$i]-$times[$i-1])*1000;$gaps += $gap;if($gap -gt 800){$top += [pscustomobject]@{from=$times[$i-1];to=$times[$i];gap_ms=[math]::Round($gap,3)}}}; $sorted=@($gaps|Sort-Object); $p50=$sorted[[math]::Floor(($sorted.Count-1)*0.5)];$p90=$sorted[[math]::Floor(($sorted.Count-1)*0.9)];$p95=$sorted[[math]::Floor(($sorted.Count-1)*0.95)];[pscustomobject]@{file=$f.Name;frames=$times.Count;first=$times[0];last=$times[-1];duration_s=[math]::Round($times[-1]-$times[0],3);median_gap_ms=[math]::Round($p50,3);p90_gap_ms=[math]::Round($p90,3);p95_gap_ms=[math]::Round($p95,3);max_gap_ms=[math]::Round(($sorted|Select-Object -Last 1),3);gaps_over_300=($gaps|Where-Object{$_ -gt 300}).Count;gaps_over_1000=($gaps|Where-Object{$_ -gt 1000}).Count}|Format-List; Write-Output 'top gaps:'; $top|Select-Object -First 20|Format-Table -AutoSize}`
>
> </details>
>
> Viewed 4 images
>
> <details><summary>Ran 4 commands</summary>
>
> - Ran `$lines=Get-Content -LiteralPath '.\src\db.rs'; for($i=115;$i -lt 285;$i++){ '{0}:{1}' -f ($i+1),$lines[$i] }`
> - Ran `rg -n -C 12 "gap up to|TRACE PARTIAL|INVALID|failed to detect cable|gate_status|three_quarter_quality" src/draw.rs src`
> - Ran `$f=Get-ChildItem -LiteralPath '.\trap_records\recovery_snapshot' -Filter '*1859830.json'; $j=Get-Content -LiteralPath $f.FullName -Raw|ConvertFrom-Json; $j.gate_deviations.at_three_quarter_nm|Format-List; Write-Output '--- sequence/gap aggregate ---'; $files=Get-ChildItem -LiteralPath '.\trap_records\recovery_snapshot' -Filter '*.json'|Sort-Object Name; foreach($file in $files){$x=Get-Content -LiteralPath $file.FullName -Raw|ConvertFrom-Json;$d=@($x.datums);$seqs=@($d|ForEach-Object{[long]$_.observation_sequence});$missing=0;for($i=1;$i -lt $seqs.Count;$i++){if($seqs[$i]-$seqs[$i-1]-gt 0){$missing += $seqs[$i]-$seqs[$i-1]-1}};$dcsGaps=@();for($i=1;$i -lt $d.Count;$i++){$dcsGaps += ([double]$d[$i].time-[double]$d[$i-1].time)*1000};[pscustomobject]@{file=$file.Name;success_datums=$d.Count;first_seq=$seqs[0];last_seq=$seqs[-1];missing_sequences_between_datums=$missing;missing_pct=[math]::Round(100*$missing/($missing+$d.Count),1);dcs_gaps_over_300=($dcsGaps|Where-Object{$_-gt 300}).Count;dcs_gaps_over_1000=($dcsGaps|Where-Object{$_-gt 1000}).Count;max_dcs_gap_ms=[math]::Round(($dcsGaps|Measure-Object -Maximum).Maximum,1)}|Format-List}`
> - Ran `$cases=@(@{suffix='898090';gates=@(1389.0,926.0,463.0);after=@(800,800,800)},@{suffix='1859830';gates=@(1389.0,926.0,463.0);after=@(1700,1700,1700)}); foreach($case in $cases){$f=Get-ChildItem -LiteralPath '.\trap_records\recovery_snapshot' -Filter "*$($case.suffix).json";$j=Get-Content -LiteralPath $f.FullName -Raw|ConvertFrom-Json;$d=@($j.datums);Write-Output "--- $($f.Name) latest final ---";for($g=0;$g-lt 3;$g++){$gate=$case.gates[$g];$best=$null;for($i=1;$i-lt$d.Count;$i++){if($d[$i].time-gt$case.after[$g]-and$d[$i-1].x-gt$gate-and$d[$i].x-le$gate){$best=[pscustomobject]@{gate_m=$gate;from_seq=$d[$i-1].observation_sequence;to_seq=$d[$i].observation_sequence;missing_attempts=[long]$d[$i].observation_sequence-[long]$d[$i-1].observation_sequence-1;from_dcs=$d[$i-1].time;to_dcs=$d[$i].time;dcs_gap_ms=[math]::Round((([double]$d[$i].time-[double]$d[$i-1].time)*1000),1);wall_gap_ms=[long]$d[$i].plane_received_unix_ms-[long]$d[$i-1].plane_received_unix_ms;successful_rtt_ms=[math]::Round($d[$i].request_round_trip_ms,1);from_x=[math]::Round($d[$i-1].x,1);to_x=[math]::Round($d[$i].x,1)}}};$best|Format-List}}`
>
> </details>
>
> The reconstruction shows this is not a pilot-path or gate-order problem. In both final approaches, the aircraft crossed the affected thresholds during roughly 0.9–1.08 seconds with three missing snapshot attempts between the bracketing samples. The gate contract rejects any bracket over 300 ms, so `InsufficientGates` is the expected safety result. I’m doing one targeted code-level validation and checking why the images misleadingly report only 327–355 ms.
>
> <details><summary>Ran 2 commands</summary>
>
> - Ran `cargo test --locked gate -- --nocapture`
> - Ran `$p='C:\Users\thierry\.codex\memories\rollout_summaries\2026-08-31T22-53-27-B61I-dcs_lso_telemetry_analysis_and_atomic_snapshot_plan.md'; $lines=Get-Content -LiteralPath $p; for($i=20;$i -lt [Math]::Min(55,$lines.Count);$i++){ '{0}:{1}' -f ($i+1),$lines[$i] }; Write-Output '--- CURRENT STATUS ---'; git status --short; git rev-parse --short HEAD; Write-Output '--- KEY FILE LINE COUNTS/CITATIONS ---'; $targets=@('.\src\track.rs','.\src\tasks\record_recovery.rs','.\src\telemetry.rs','.\src\draw.rs'); foreach($t in $targets){Write-Output $t; rg -n "InsufficientGates|stale_gate_bracket|telemetry_aligner.reset|sample_gap_ms = self|max_sample_gap_ms\)|gap up to|SAMPLE_GAP_WARNING_MS|all_valid" $t}`
>
> </details>
>
> <details><summary>Plan</summary>
>
> Both records and every paired artifact were correlated with the current gate and telemetry code. The cause and a secondary diagnostics defect are established.
>
> - [x] Inventory both snapshot records and identify their schemas/results
> - [x] Trace InsufficientGates through current grading and recovery code
> - [x] Correlate every gate and timing signal across both tests
> - [x] Report root cause, evidence, and actionable next checks
>
> </details>
</details>

The `insufficient_gates` result is correct for both tests: snapshot telemetry disappeared exactly while the aircraft crossed the grading gates.

| Test | ¾ NM | ½ NM | ¼ NM |
|---|---:|---:|---:|
| 20:55 | 1,080 ms gap, 3 missing attempts → invalid | 960 ms, 3 missing → invalid | 990 ms, 3 missing → invalid |
| 21:11 | 90 ms → valid | 900 ms, 3 missing → invalid | 1,080 ms, 3 missing → invalid |

The gate limit is 300 ms. All three gates must be present, valid, and chronologically ordered ([track.rs](/C:/Users/thierry/Documents/GitHub/sevenfifty777/DCS-gRPC-lso/src/track.rs:328)). A bracket exceeding 300 ms becomes `stale_gate_bracket` ([track.rs](/C:/Users/thierry/Documents/GitHub/sevenfifty777/DCS-gRPC-lso/src/track.rs:1539)); if any gate is invalid, completion becomes `InsufficientGates` and the grade is forced to `Incomplete`/NC ([track.rs](/C:/Users/thierry/Documents/GitHub/sevenfifty777/DCS-gRPC-lso/src/track.rs:1148)).

### What happened

Both recordings used schema 4 and atomic snapshots, but acquisition was only approximately 5 Hz overall:

- Test 20:55: 385 missing sequence numbers, 27.5% of attempts.
- Test 21:11: 406 missing sequence numbers, 28.3% of attempts.
- The ACMI logs contain 195 and 210 gaps over 300 ms respectively.
- Their maximum actual gaps are 1,080 and 1,110 ms.

Three consecutive missing requests at each affected gate strongly indicate repeated atomic snapshot RPC failures—most likely deadline expirations if the default 250 ms timeout was used. The exact gRPC status cannot be proven because this folder contains no console/runtime log and unsuccessful statuses are not persisted.

The touchdown evidence itself was good: both reports recorded a correlated recovery and DCS `WIRE# 4`. It was the approach telemetry, not the landing outcome, that prevented grading.

### Misleading diagnostics found

The displayed “gap up to 355 ms” and “327 ms” values are too low. On an RPC error, the recorder resets `TelemetryAligner` ([record_recovery.rs](/C:/Users/thierry/Documents/GitHub/sevenfifty777/DCS-gRPC-lso/src/tasks/record_recovery.rs:439)). Consequently, the next successful sample starts without its preceding outage being included in `sample_gap_ms`. This explains why both JSON files claim:

- telemetry health: `green`
- invalid samples: `0`
- maximum scoring gaps: only 170/237 ms

Meanwhile, the gate detector correctly sees the 900–1,080 ms DCS-time brackets. The renderer then prints the underestimated global gap instead of the actual rejected gate bracket ([draw.rs](/C:/Users/thierry/Documents/GitHub/sevenfifty777/DCS-gRPC-lso/src/draw.rs:1120)).

The images’ “TRACE PARTIAL — 4/9 fragments” messages independently confirm these outages.

Relevant records:

- [20:55 JSON](/C:/Users/thierry/Documents/GitHub/sevenfifty777/DCS-gRPC-lso/trap_records/recovery_snapshot/LSO-20260901-205515-Ghost72TT-s1788288229-g19-p5240-c5160-t898090.json)
- [21:11 JSON](/C:/Users/thierry/Documents/GitHub/sevenfifty777/DCS-gRPC-lso/trap_records/recovery_snapshot/LSO-20260901-211107-Ghost72TT-s1788288229-g19-p5240-c5160-t1859830.json)

Targeted validation passed: `cargo test --locked gate -- --nocapture` — 10 passed, 0 failed. No source files were changed.

Earlier recovery notes were used as a starting hypothesis; all figures above were recomputed from today’s two records.

> can you compare f we improved perf compare to yesterday data in legacy folder and also in C:\Users\thierry\Documents\GitHub\sevenfifty777\DCS-gRPC-lso\docs\DCS\_gRPC\_analyse\DCS\_GRPC\_LSO\_RECOVERY\_ANALYSIS.md

<details><summary>13 previous messages</summary>

> I’ll compare yesterday’s legacy captures and today’s atomic captures using the same timestamp-gap methodology, then reconcile the result with the existing analysis document. I’m reusing the graphify navigation workflow because this comparison depends on the current telemetry contracts and the report’s prior conclusions; the final answer will distinguish measured improvement from differences that the two small flight samples cannot prove.
>
> <details><summary>Ran 2 commands</summary>
>
> - Ran `$skill='C:\Users\thierry\.agents\skills\graphify\SKILL.md'; $lines=Get-Content -LiteralPath $skill; Write-Output "skill_lines=$($lines.Count)"; for($start=0;$start-lt$lines.Count;$start+=190){Write-Output "--- SKILL $($start+1)-$([Math]::Min($start+190,$lines.Count)) ---"; $lines|Select-Object -Skip $start -First 190}; Write-Output '--- MEMORY HITS ---'; rg -n -i "legacy|atomic|recovery telemetry|GetRecoverySnapshot|gap|schema-v3|cadence" 'C:\Users\thierry\.codex\memories\MEMORY.md'`
> - Ran `Get-Content -LiteralPath 'C:\Users\thierry\.agents\skills\graphify\references\query.md' -Raw; Write-Output '--- RECORD DIRECTORIES ---'; Get-ChildItem -LiteralPath '.\trap_records' -Directory -Recurse | Select-Object FullName,LastWriteTime; Write-Output '--- LEGACY/RECOVERY FILES ---'; rg --files '.\trap_records' | rg -i "legacy|recovery"; Write-Output '--- REPORT ---'; Get-Item -LiteralPath '.\docs\DCS_gRPC_analyse\DCS_GRPC_LSO_RECOVERY_ANALYSIS.md' | Select-Object FullName,Length,LastWriteTime`
>
> </details>
>
> The legacy folder contains a large historical corpus, but its newest recovery files appear to be dated August 30 rather than August 31. I’ll identify the exact latest schema-v3/legacy-mode records instead of assuming every file in that folder is part of yesterday’s benchmark.
>
> <details><summary>Ran 7 commands</summary>
>
> - Ran `$files=Get-ChildItem -LiteralPath '.\trap_records\Legacy' -Filter '*.json'|Sort-Object LastWriteTime -Descending; $rows=@(); foreach($f in ($files|Select-Object -First 30)){try{$j=Get-Content -LiteralPath $f.FullName -Raw|ConvertFrom-Json;$rows += [pscustomobject]@{file=$f.Name;last_write=$f.LastWriteTime;schema=$j.schema_version;mode=$j.acquisition_mode;recorded=$j.recording_started_at;completed=$j.completed_at;pilot=$j.pilot_name;aircraft=$j.aircraft_type;datums=@($j.datums).Count;completeness=$j.telemetry_quality.completeness;max_gap=$j.telemetry_quality.max_sample_gap_ms;grade=$j.pass_grade}}catch{$rows += [pscustomobject]@{file=$f.Name;last_write=$f.LastWriteTime;schema='parse_error'}}}; $rows|Format-Table -AutoSize -Wrap; Write-Output '--- DATE PREFIX COUNTS (latest) ---'; $files|ForEach-Object{if($_.Name -match '^LSO-(\d{8})-'){$matches[1]}}|Group-Object|Sort-Object Name -Descending|Select-Object -First 12 Name,Count|Format-Table -AutoSize`
> - Ran `$files=Get-ChildItem -LiteralPath '.\trap_records\Legacy' -Filter 'LSO-20260831-*.json'|Sort-Object Name; foreach($f in $files){$j=Get-Content -LiteralPath $f.FullName -Raw|ConvertFrom-Json; [pscustomobject]@{file=$f.Name;schema=$j.schema_version;pilot=$j.pilot_name;aircraft=$j.aircraft_type;carrier=$j.carrier_name;mode=$j.acquisition_mode;datums=@($j.datums).Count;first_time=if(@($j.datums).Count){$j.datums[0].time};last_time=if(@($j.datums).Count){$j.datums[-1].time};grading=($j.grading|ConvertTo-Json -Compress);pass_grade=$j.pass_grade;keys=($j.PSObject.Properties.Name -join ',')}}|Format-List; Write-Output '--- FIRST SCHEMA2 DATUM ---'; $j0=Get-Content -LiteralPath $files[0].FullName -Raw|ConvertFrom-Json;$j0.datums[0]|ConvertTo-Json -Depth 6`
> - Ran `$files=Get-ChildItem -LiteralPath '.\trap_records\Legacy' -Filter 'LSO-20260831-*.json'|Sort-Object Name;$rows=@(); foreach($f in $files){$j=Get-Content -LiteralPath $f.FullName -Raw|ConvertFrom-Json;$rows += [pscustomobject]@{file=$f.Name;schema=$j.schema_version;pilot=$j.pilot_name;aircraft=$j.aircraft_type;carrier=$j.carrier_name;mode=$j.acquisition_mode;datums=@($j.datums).Count;first_time=if(@($j.datums).Count){$j.datums[0].time};last_time=if(@($j.datums).Count){$j.datums[-1].time};grading=($j.grading|ConvertTo-Json -Compress);pass_grade=$j.pass_grade;keys=($j.PSObject.Properties.Name -join ',')}};$rows|Format-List; Write-Output '--- FIRST SCHEMA2 DATUM ---'; $j0=Get-Content -LiteralPath $files[0].FullName -Raw|ConvertFrom-Json;$j0.datums[0]|ConvertTo-Json -Depth 6`
> - Ran `$files=Get-ChildItem -LiteralPath '.\trap_records\Legacy' -Filter 'LSO-20260831-*.json'|Sort-Object Name;$rows=@();foreach($f in $files){$j=Get-Content -LiteralPath $f.FullName -Raw|ConvertFrom-Json;$q=$j.telemetry_quality;$g=$j.gate_deviations;$h=$j.hook_observation;$rows += [pscustomobject]@{file=$f.Name;pilot=$j.pilot_name;aircraft=$j.aircraft_type;datums=@($j.datums).Count;completeness=$q.completeness;health=$q.health;max_gap=[math]::Round([double]$q.max_sample_gap_ms,1);max_scoring=[math]::Round([double]$q.max_scoring_sample_gap_ms,1);warnings=$q.warning_samples;invalid=$q.invalid_samples;dropped=$q.dropped_samples;g3q=$g.three_quarter_quality.status;g3q_reason=$g.three_quarter_quality.reason;gh=$g.half_quality.status;gh_reason=$g.half_quality.reason;gq=$g.quarter_quality.status;gq_reason=$g.quarter_quality.reason;hook_compacted=$h.compacted_samples;hook_timeline=@($h.timeline).Count}};$rows|Format-Table -AutoSize -Wrap`
> - Ran `$files=Get-ChildItem -LiteralPath '.\trap_records\Legacy' -Filter 'LSO-20260831-*.json'|Sort-Object Name;foreach($f in $files){$j=Get-Content -LiteralPath $f.FullName -Raw|ConvertFrom-Json;$q=$j.telemetry_quality;$g=$j.gate_deviations;$h=$j.hook_observation;Write-Output ("{0}|{1}|{2}|n={3}|complete={4}|health={5}|max={6:N1}|scoremax={7:N1}|warn={8}|invalid={9}|drop={10}|gates={11}/{12}/{13}|reasons={14}/{15}/{16}|hook={17}+{18}" -f $f.Name,$j.pilot_name,$j.aircraft_type,@($j.datums).Count,$q.completeness,$q.health,[double]$q.max_sample_gap_ms,[double]$q.max_scoring_sample_gap_ms,$q.warning_samples,$q.invalid_samples,$q.dropped_samples,$g.three_quarter_quality.status,$g.half_quality.status,$g.quarter_quality.status,$g.three_quarter_quality.reason,$g.half_quality.reason,$g.quarter_quality.reason,@($h.timeline).Count,$h.compacted_samples)}`
> - Ran `$p='.\docs\DCS_gRPC_analyse\DCS_GRPC_LSO_RECOVERY_ANALYSIS.md'; (Get-Content -LiteralPath $p).Count; rg -n -i -C 4 "legacy|atomic|GetRecoverySnapshot|median|P90|P95|gap|10 Hz|schema-v3|2,586|491|769|acceptance|baseline|hot-path|timer" $p`
> - Ran `if(Test-Path '.\graphify-out\.vocab.txt'){rg -n -i "^(atomic|legacy|telemetry|recovery|sample|samples|gap|gaps|performance|snapshot|snapshots|transform|transforms|cadence|gate|gates|rpc)$" '.\graphify-out\.vocab.txt'}else{Write-Output 'vocab file missing'}; $d=Get-Content -LiteralPath '.\graphify-out\graph.json' -Raw|ConvertFrom-Json;$terms=@('legacy','telemetry','recovery','sample','samples','gap','gaps','snapshot','transform','transforms','gate','gates');$matches=@();foreach($n in $d.nodes){$label=[string]$n.label;$score=0;foreach($t in $terms){if($label.ToLowerInvariant().Contains($t)){$score++}};if($score-gt 0){$matches += [pscustomobject]@{score=$score;id=$n.id;label=$label;source=$n.source_file;location=$n.source_location}}};$matches|Sort-Object score -Descending|Select-Object -First 20|Format-Table -AutoSize -Wrap`
>
> </details>
>
> we have plenty of data from 31st august
>
> For the repository graph, the auditable vocabulary expansion is `[recovery, transform, gate, gates]`; the older graph does not contain the newly implemented atomic-snapshot terminology. I’m therefore using it only to identify the recovery path and treating the report plus raw records as the evidence source.
>
> <details><summary>Ran a command</summary>
>
> - Ran `function Percentile([double[]]$values,[double]$p){if($values.Count-eq 0){return [double]::NaN};$s=@($values|Sort-Object);$idx=[math]::Floor(($s.Count-1)*$p);return [double]$s[$idx]};function Summarize($name,$files){$all=@();$wall=@();$groove=@();$grooveWall=@();$samples=0;$duration=0.0;$validGates=0;$gateTotal=0;$complete=0;$statuses=@{};$seqMissing=0;$seqOpportunities=0;foreach($f in $files){$j=Get-Content -LiteralPath $f.FullName -Raw|ConvertFrom-Json;$d=@($j.datums);$samples += $d.Count;if($d.Count-gt 1){$duration += [double]$d[-1].time-[double]$d[0].time};for($i=1;$i-lt$d.Count;$i++){$dg=([double]$d[$i].time-[double]$d[$i-1].time)*1000;$wg=[double]$d[$i].plane_received_unix_ms-[double]$d[$i-1].plane_received_unix_ms;$all += $dg;$wall += $wg;if([double]$d[$i-1].x-ge-200-and[double]$d[$i-1].x-le1389-and[double]$d[$i].x-ge-200-and[double]$d[$i].x-le1389){$groove += $dg;$grooveWall += $wg};if($null-ne$d[$i].observation_sequence-and$null-ne$d[$i-1].observation_sequence){$jump=[long]$d[$i].observation_sequence-[long]$d[$i-1].observation_sequence;$seqOpportunities += [math]::Max(1,$jump);if($jump-gt1){$seqMissing += $jump-1}}};$g=$j.gate_deviations;foreach($q in @($g.three_quarter_quality,$g.half_quality,$g.quarter_quality)){if($null-ne$q){$gateTotal++;if($q.status-eq'valid'){$validGates++}}};$status=[string]$j.telemetry_quality.completeness;if([string]::IsNullOrWhiteSpace($status)){$status='unspecified'};if(!$statuses.ContainsKey($status)){$statuses[$status]=0};$statuses[$status]++;if($status-eq'complete'){$complete++}};$statusText=($statuses.GetEnumerator()|Sort-Object Name|ForEach-Object{"$($_.Name)=$($_.Value)"})-join',';[pscustomobject]@{scope=$name;records=$files.Count;samples=$samples;duration_s=[math]::Round($duration,1);effective_hz=[math]::Round($samples/$duration,2);median_dcs_ms=[math]::Round((Percentile $all 0.5),1);p90_dcs_ms=[math]::Round((Percentile $all 0.9),1);p95_dcs_ms=[math]::Round((Percentile $all 0.95),1);p99_dcs_ms=[math]::Round((Percentile $all 0.99),1);max_dcs_ms=[math]::Round((Percentile $all 1.0),1);over300=$(@($all|Where-Object{$_-gt300}).Count);over300_pct=[math]::Round(100*@($all|Where-Object{$_-gt300}).Count/$all.Count,1);over1000=$(@($all|Where-Object{$_-gt1000}).Count);groove_n=$groove.Count;groove_median=[math]::Round((Percentile $groove 0.5),1);groove_p90=[math]::Round((Percentile $groove 0.9),1);groove_p95=[math]::Round((Percentile $groove 0.95),1);groove_over300=$(@($groove|Where-Object{$_-gt300}).Count);groove_over300_pct=[math]::Round(100*@($groove|Where-Object{$_-gt300}).Count/$groove.Count,1);valid_gates="$validGates/$gateTotal";completeness=$statusText;seq_missing=$seqMissing;seq_missing_pct=if($seqOpportunities-gt0){[math]::Round(100*$seqMissing/$seqOpportunities,1)}else{$null}}};$legacy=Get-ChildItem -LiteralPath '.\trap_records\Legacy' -Filter 'LSO-20260831-*.json';$legacyGhost=@($legacy|Where-Object{$_.Name-match'Ghost72TT'});$atomic=Get-ChildItem -LiteralPath '.\trap_records\recovery_snapshot' -Filter '*.json';$out=@();$out += Summarize 'Legacy Aug31 all' @($legacy);$out += Summarize 'Legacy Aug31 Ghost T-45' $legacyGhost;$out += Summarize 'Atomic Sep01 Ghost T-45' @($atomic);$out|Format-List`
>
> </details>
>
> Yes—I found all 15 August 31 schema-v2 records, totaling 11,055 samples, and I’m using all of them as the primary legacy cohort. I’m also keeping a six-record Ghost‑72/T‑45 subset as the closest apples-to-apples comparison with today’s two tests; both views currently point to the same nuanced result: typical cadence improved, but the extreme one-second outage tail became worse.
>
> <details><summary>Ran 8 commands</summary>
>
> - Ran `function P([double[]]$v,[double]$p){$s=@($v|Sort-Object);if(!$s.Count){return 0};$s[[math]::Floor(($s.Count-1)*$p)]};$sets=@(@{name='legacy';files=@(Get-ChildItem -LiteralPath '.\trap_records\Legacy' -Filter 'LSO-20260831-*.json'|Sort-Object Name)},@{name='atomic';files=@(Get-ChildItem -LiteralPath '.\trap_records\recovery_snapshot' -Filter '*.json'|Sort-Object Name)});foreach($set in$sets){Write-Output "=== $($set.name) per record ===";foreach($f in$set.files){$j=Get-Content -LiteralPath $f.FullName -Raw|ConvertFrom-Json;$d=@($j.datums);$dg=@();$wg=@();$sg=@();for($i=1;$i-lt$d.Count;$i++){$dg += ([double]$d[$i].time-[double]$d[$i-1].time)*1000;$wg += [double]$d[$i].plane_received_unix_ms-[double]$d[$i-1].plane_received_unix_ms;$sg += [double]$d[$i].sample_gap_ms};$dur=[double]$d[-1].time-[double]$d[0].time;$valid=@($j.gate_deviations.three_quarter_quality.status,$j.gate_deviations.half_quality.status,$j.gate_deviations.quarter_quality.status|Where-Object{$_-eq'valid'}).Count;Write-Output ("{0}|{1}|{2}|n={3}|Hz={4:N2}|DCS p50/p90/p95/p99/max={5:N0}/{6:N0}/{7:N0}/{8:N0}/{9:N0}|wall={10:N0}/{11:N0}/{12:N0}/{13:N0}/{14:N0}|>300={15:N1}%|>1000={16:N1}%|storedmax={17:N1}|gates={18}/3|{19}" -f $f.Name,$j.pilot_name,$j.aircraft_type,$d.Count,($d.Count/$dur),(P $dg .5),(P $dg .9),(P $dg .95),(P $dg .99),(P $dg 1),(P $wg .5),(P $wg .9),(P $wg .95),(P $wg .99),(P $wg 1),(100*@($dg|Where-Object{$_-gt300}).Count/$dg.Count),(100*@($dg|Where-Object{$_-gt1000}).Count/$dg.Count),([double]$j.telemetry_quality.max_sample_gap_ms),$valid,$j.telemetry_quality.completeness)}}}`
> - Ran `function P([double[]]$v,[double]$p){$s=@($v|Sort-Object);if(!$s.Count){return 0};$s[[math]::Floor(($s.Count-1)*$p)]};$sets=@(@{name='legacy';files=@(Get-ChildItem -LiteralPath '.\trap_records\Legacy' -Filter 'LSO-20260831-*.json'|Sort-Object Name)},@{name='atomic';files=@(Get-ChildItem -LiteralPath '.\trap_records\recovery_snapshot' -Filter '*.json'|Sort-Object Name)});foreach($set in $sets){Write-Output "=== $($set.name) per record ===";foreach($f in $set.files){$j=Get-Content -LiteralPath $f.FullName -Raw|ConvertFrom-Json;$d=@($j.datums);$dg=@();$wg=@();$sg=@();for($i=1;$i-lt$d.Count;$i++){$dg += ([double]$d[$i].time-[double]$d[$i-1].time)*1000;$wg += [double]$d[$i].plane_received_unix_ms-[double]$d[$i-1].plane_received_unix_ms;$sg += [double]$d[$i].sample_gap_ms};$dur=[double]$d[-1].time-[double]$d[0].time;$valid=@($j.gate_deviations.three_quarter_quality.status,$j.gate_deviations.half_quality.status,$j.gate_deviations.quarter_quality.status|Where-Object{$_-eq'valid'}).Count;Write-Output ("{0}|{1}|{2}|n={3}|Hz={4:N2}|DCS p50/p90/p95/p99/max={5:N0}/{6:N0}/{7:N0}/{8:N0}/{9:N0}|wall={10:N0}/{11:N0}/{12:N0}/{13:N0}/{14:N0}|>300={15:N1}%|>1000={16:N1}%|storedmax={17:N1}|gates={18}/3|{19}" -f $f.Name,$j.pilot_name,$j.aircraft_type,$d.Count,($d.Count/$dur),(P $dg .5),(P $dg .9),(P $dg .95),(P $dg .99),(P $dg 1),(P $wg .5),(P $wg .9),(P $wg .95),(P $wg .99),(P $wg 1),(100*@($dg|Where-Object{$_-gt300}).Count/$dg.Count),(100*@($dg|Where-Object{$_-gt1000}).Count/$dg.Count),([double]$j.telemetry_quality.max_sample_gap_ms),$valid,$j.telemetry_quality.completeness)}}`
> - Ran `$all=Get-ChildItem -LiteralPath '.\trap_records\Legacy' -Filter '*.json';$v2=@();foreach($f in$all){try{$j=Get-Content -LiteralPath $f.FullName -Raw|ConvertFrom-Json;if($j.schema_version-eq2){$v2+=$f}}catch{}};Write-Output "v2_count=$($v2.Count)";Write-Output "v2_files:";$v2|Sort-Object Name|ForEach-Object{$_.Name};function P([double[]]$v,[double]$p){$s=@($v|Sort-Object);$s[[math]::Floor(($s.Count-1)*$p)]};foreach($label in @('v2','aug31','atomic')){$files=switch($label){'v2'{$v2};'aug31'{@($v2|Where-Object{$_.Name-match'^LSO-20260831-'})};'atomic'{@(Get-ChildItem -LiteralPath '.\trap_records\recovery_snapshot' -Filter '*.json')}};$raw=@();$stored=@();$wall=@();$grooveRaw=@();$grooveStored=@();$n=0;foreach($f in$files){$j=Get-Content -LiteralPath $f.FullName -Raw|ConvertFrom-Json;$d=@($j.datums);$n+=$d.Count;for($i=1;$i-lt$d.Count;$i++){$r=([double]$d[$i].time-[double]$d[$i-1].time)*1000;$s=[double]$d[$i].sample_gap_ms;$w=[double]$d[$i].plane_received_unix_ms-[double]$d[$i-1].plane_received_unix_ms;$raw+=$r;$stored+=$s;$wall+=$w;if([double]$d[$i-1].x-ge-200-and[double]$d[$i-1].x-le1389-and[double]$d[$i].x-ge-200-and[double]$d[$i].x-le1389){$grooveRaw+=$r;$grooveStored+=$s}}};Write-Output "--- $label records=$($files.Count) samples=$n ---";foreach($m in@(@{n='raw_dcs';v=$raw},@{n='stored';v=$stored},@{n='wall';v=$wall},@{n='groove_raw';v=$grooveRaw},@{n='groove_stored';v=$grooveStored})){$v=@($m.v);Write-Output ("{0}: n={1} p50={2:N1} p90={3:N1} p95={4:N1} p99={5:N1} max={6:N1} >300={7} ({8:N1}%) >1000={9} ({10:N1}%)"-f$m.n,$v.Count,(P$v .5),(P$v .9),(P$v .95),(P$v .99),(P$v 1),@($v|Where-Object{$_-gt300}).Count,(100*@($v|Where-Object{$_-gt300}).Count/$v.Count),@($v|Where-Object{$_-gt1000}).Count,(100*@($v|Where-Object{$_-gt1000}).Count/$v.Count))}}`
> - Ran `$all=Get-ChildItem -LiteralPath '.\trap_records\Legacy' -Filter '*.json';$v2=@();foreach($f in $all){try{$j=Get-Content -LiteralPath $f.FullName -Raw|ConvertFrom-Json;if($j.schema_version-eq2){$v2+=$f}}catch{}};Write-Output "v2_count=$($v2.Count)";Write-Output "v2_files:";$v2|Sort-Object Name|ForEach-Object{$_.Name};function P([double[]]$v,[double]$p){$s=@($v|Sort-Object);$s[[math]::Floor(($s.Count-1)*$p)]};foreach($label in @('v2','aug31','atomic')){$files=switch($label){'v2'{$v2};'aug31'{@($v2|Where-Object{$_.Name-match'^LSO-20260831-'})};'atomic'{@(Get-ChildItem -LiteralPath '.\trap_records\recovery_snapshot' -Filter '*.json')}};$raw=@();$stored=@();$wall=@();$grooveRaw=@();$grooveStored=@();$n=0;foreach($f in $files){$j=Get-Content -LiteralPath $f.FullName -Raw|ConvertFrom-Json;$d=@($j.datums);$n+=$d.Count;for($i=1;$i-lt$d.Count;$i++){$r=([double]$d[$i].time-[double]$d[$i-1].time)*1000;$s=[double]$d[$i].sample_gap_ms;$w=[double]$d[$i].plane_received_unix_ms-[double]$d[$i-1].plane_received_unix_ms;$raw+=$r;$stored+=$s;$wall+=$w;if([double]$d[$i-1].x-ge-200-and[double]$d[$i-1].x-le1389-and[double]$d[$i].x-ge-200-and[double]$d[$i].x-le1389){$grooveRaw+=$r;$grooveStored+=$s}}};Write-Output "--- $label records=$($files.Count) samples=$n ---";foreach($m in @(@{n='raw_dcs';v=$raw},@{n='stored';v=$stored},@{n='wall';v=$wall},@{n='groove_raw';v=$grooveRaw},@{n='groove_stored';v=$grooveStored})){$v=@($m.v);Write-Output ("{0}: n={1} p50={2:N1} p90={3:N1} p95={4:N1} p99={5:N1} max={6:N1} >300={7} ({8:N1}%) >1000={9} ({10:N1}%)"-f$m.n,$v.Count,(P $v .5),(P $v .9),(P $v .95),(P $v .99),(P $v 1),@($v|Where-Object{$_-gt300}).Count,(100*@($v|Where-Object{$_-gt300}).Count/$v.Count),@($v|Where-Object{$_-gt1000}).Count,(100*@($v|Where-Object{$_-gt1000}).Count/$v.Count))}}`
> - Ran `function P([double[]]$v,[double]$p){$s=@($v|Sort-Object);$s[[math]::Floor(($s.Count-1)*$p)]};$files=Get-ChildItem -LiteralPath '.\trap_records\recovery_snapshot' -Filter '*.json';$rtt=@();$jumps=@();$grooveJumps=@();$attempts=0;$success=0;$duration=0;foreach($f in$files){$j=Get-Content -LiteralPath $f.FullName -Raw|ConvertFrom-Json;$d=@($j.datums);$duration += [double]$d[-1].time-[double]$d[0].time;$success += $d.Count;foreach($x in$d){if($null-ne$x.request_round_trip_ms){$rtt += [double]$x.request_round_trip_ms}};for($i=1;$i-lt$d.Count;$i++){$jump=[long]$d[$i].observation_sequence-[long]$d[$i-1].observation_sequence;if($jump-gt1){$jumps += $jump-1;if([double]$d[$i].x-ge-200-and[double]$d[$i].x-le1389){$grooveJumps += $jump-1}};$attempts += [math]::Max(1,$jump)}};Write-Output ("success=$success attempts_between=$attempts missing=$($attempts-($success-$files.Count)) delivered_hz={0:N2} attempted_hz={1:N2}"-f($success/$duration),($attempts/$duration));Write-Output ("RTT ms p50={0:N1} p90={1:N1} p95={2:N1} p99={3:N1} max={4:N1}"-f(P$rtt .5),(P$rtt .9),(P$rtt .95),(P$rtt .99),(P$rtt 1));Write-Output 'missing run lengths:';$jumps|Group-Object|Sort-Object {[int]$_.Name}|Format-Table Name,Count -AutoSize;Write-Output 'groove missing run lengths:';$grooveJumps|Group-Object|Sort-Object {[int]$_.Name}|Format-Table Name,Count -AutoSize`
> - Ran `function P([double[]]$v,[double]$p){$s=@($v|Sort-Object);$s[[math]::Floor(($s.Count-1)*$p)]};$files=Get-ChildItem -LiteralPath '.\trap_records\recovery_snapshot' -Filter '*.json';$rtt=@();$jumps=@();$grooveJumps=@();$attempts=0;$success=0;$duration=0;foreach($f in $files){$j=Get-Content -LiteralPath $f.FullName -Raw|ConvertFrom-Json;$d=@($j.datums);$duration += [double]$d[-1].time-[double]$d[0].time;$success += $d.Count;foreach($x in $d){if($null-ne$x.request_round_trip_ms){$rtt += [double]$x.request_round_trip_ms}};for($i=1;$i-lt$d.Count;$i++){$jump=[long]$d[$i].observation_sequence-[long]$d[$i-1].observation_sequence;if($jump-gt1){$jumps += $jump-1;if([double]$d[$i].x-ge-200-and[double]$d[$i].x-le1389){$grooveJumps += $jump-1}};$attempts += [math]::Max(1,$jump)}};Write-Output ("success=$success attempts_between=$attempts missing=$($attempts-($success-$files.Count)) delivered_hz={0:N2} attempted_hz={1:N2}"-f($success/$duration),($attempts/$duration));Write-Output ("RTT ms p50={0:N1} p90={1:N1} p95={2:N1} p99={3:N1} max={4:N1}"-f(P $rtt .5),(P $rtt .9),(P $rtt .95),(P $rtt .99),(P $rtt 1));Write-Output 'missing run lengths:';$jumps|Group-Object|Sort-Object {[int]$_.Name}|Format-Table Name,Count -AutoSize;Write-Output 'groove missing run lengths:';$grooveJumps|Group-Object|Sort-Object {[int]$_.Name}|Format-Table Name,Count -AutoSize`
> - Ran `$p='.\docs\DCS_gRPC_analyse\DCS_GRPC_LSO_RECOVERY_ANALYSIS.md';$lines=Get-Content -LiteralPath $p;foreach($start in@(0,150,300)){Write-Output "--- REPORT $($start+1)-$([Math]::Min($start+150,$lines.Count)) ---";$lines|Select-Object -Skip $start -First 150}`
> - Ran `$p='.\docs\DCS_gRPC_analyse\DCS_GRPC_LSO_RECOVERY_ANALYSIS.md';$lines=Get-Content -LiteralPath $p;foreach($start in @(0,150,300)){Write-Output "--- REPORT $($start+1)-$([Math]::Min($start+150,$lines.Count)) ---";$lines|Select-Object -Skip $start -First 150}`
>
> </details>
>
> Context compaction
>
> Source: automatic
> Status: completed
>
> I’ve now treated all 15 August 31 schema-v2 recoveries as the main baseline, with the six Ghost-72/T-45 records as the aircraft/pilot-matched control. The result is mixed: typical cadence improved, but the worst stalls and gate reliability did not. I’m tightening the comparison so the “faster” signal isn’t confused with timeouts that were simply discarded.
>
> <details><summary>Ran 2 commands</summary>
>
> - Ran `git status --short; rg -n "3 schema-v3|23 schema-v2|Median|P90|Acceptance|No groove|atomic|24 mission|10 atomic|Gate availability|valid gates|15,651|4,990" "docs/DCS_gRPC_analyse/DCS_GRPC_LSO_RECOVERY_ANALYSIS.md"; Get-ChildItem -LiteralPath "trap_records/Legacy" -Filter "20260831*.json" | Sort-Object Name | Select-Object -ExpandProperty Name; Get-ChildItem -LiteralPath "trap_records/recovery_snapshot" -Filter "*.json" | Sort-Object Name | Select-Object -ExpandProperty Name`
> - Ran `$legacy = Get-ChildItem -LiteralPath "trap_records/Legacy" -Filter "*20260831*.json" | Sort-Object Name; $legacy | ForEach-Object { $j = Get-Content -Raw -LiteralPath $_.FullName | ConvertFrom-Json; [pscustomobject]@{File=$_.Name; Schema=$j.schema_version; Pilot=$j.pilot; Airframe=$j.airframe; Datums=@($j.datums).Count} } | Format-Table -AutoSize`
>
> </details>
>
> <details><summary>Plan</summary>
>
> Completed the comparison using all 15 August 31 schema-v2 records, the six matched Ghost-72/T-45 records, both September 1 atomic tests, and the documented 23-record baseline. Raw DCS/wall timestamps were used to avoid the reset-masked sample_gap_ms diagnostic.
>
> - [x] Inventory the August 31 legacy and September 1 atomic recovery datasets
> - [x] Extract the historical baseline and acceptance thresholds from the analysis report
> - [x] Compute comparable cadence, tail-gap, groove, and gate-validity metrics
> - [x] Assess improvement and explain both InsufficientGates outcomes
>
> </details>
</details>

Yes—but only a partial performance improvement. Normal updates are faster, while the worst stalls became more frequent. Gate reliability did not improve.

### August 31 versus September 1

I analyzed all 15 August 31 schema-v2 records, including the six matched Ghost-72/T-45 recoveries.

| Cohort | Records | Effective rate | Median gap | P90 | P95 | Gaps >300 ms | Gaps >1 s | Valid gates |
|---|---:|---:|---:|---:|---:|---:|---:|---:|
| Aug 31—all aircraft | 15 | 4.52 Hz | 120 ms | 780 ms | 930 ms | 21.5% | 0.6% | 10/45 |
| Aug 31—Ghost/T-45 only | 6 | 4.66 Hz | 120 ms | 750 ms | 960 ms | 20.0% | 1.4% | 3/18 |
| Sep 1—atomic Ghost/T-45 | 2 | 5.05 Hz | 90 ms | 450 ms | 1,020 ms | 18.9% | 5.5% | 1/6 |

Compared with the matched Ghost/T-45 records:

- Delivered frequency improved approximately 8.4%.
- Median gap improved 120 → 90 ms.
- P90 improved 750 → 450 ms.
- Gaps above 300 ms improved only slightly: 20.0% → 18.9%.
- P95 worsened: 960 → 1,020 ms.
- Gaps over one second increased almost fourfold: 1.4% → 5.5%.
- Gate validity is unchanged: 16.7% in both cohorts.

This is a bimodal result: more fast samples, but also more severe holes.

### Why both tests produced `InsufficientGates`

A CATOBAR recovery needs valid ¾-mile, ½-mile, and ¼-mile gates. A gate bracket becomes stale when the samples surrounding the crossing are more than 300 ms apart.

| Atomic test | ¾-mile bracket | ½-mile bracket | ¼-mile bracket | Result |
|---|---:|---:|---:|---|
| [20:55 test](./trap_records/recovery_snapshot/LSO-20260901-205515-Ghost72TT-s1788288229-g19-p5240-c5160-t898090.json) | 1,080 ms ❌ | 960 ms ❌ | 990 ms ❌ | 0/3 gates |
| [21:11 test](./trap_records/recovery_snapshot/LSO-20260901-211107-Ghost72TT-s1788288229-g19-p5240-c5160-t1859830.json) | 90 ms ✅ | 900 ms ❌ | 1,080 ms ❌ | 1/3 gates |

Therefore:

- First recovery: every required gate was rejected.
- Second recovery: only the ¾-mile gate was valid.
- Neither recovery had enough gates for grading.

### Probable underlying reason

The atomic records contain 791 missing sequence intervals—27.9% of attempted intervals. There are 200 missing runs of exactly three consecutive requests.

That pattern is strongly consistent with requests exceeding the current 250 ms atomic deadline and being discarded. Three missed requests plus the next successful observation creates the observed approximately 0.9–1.1 second holes. However, the logs do not preserve the exact RPC error/status, server queue time, or Lua execution time, so they cannot distinguish definitively between:

- DCS mission/Lua stalls,
- server queue congestion,
- transport delay,
- client scheduling delay.

There is also a diagnostic problem: after an acquisition error, the telemetry aligner is reset in [record_recovery.rs](./src/tasks/record_recovery.rs:440). Consequently, persisted `sample_gap_ms` reports maximums of only approximately 327–355 ms, while raw DCS and wall timestamps show gaps up to 1,110 ms. The apparently “green” stored gap statistics are therefore misleading.

### Comparison with the analysis document

The document’s separate historical baseline contains 23 schema-v2 reports rather than the 15 August 31 files currently in `Legacy`. It reports median 105.1 ms, P90 706.2 ms, and 21.5% of gaps above 300 ms ([documented baseline](./docs/DCS_gRPC_analyse/DCS_GRPC_LSO_RECOVERY_ANALYSIS.md:142)).

Against that documented baseline, atomic improved median and P90, but it fails the document’s acceptance criteria of:

- no groove gap above 300 ms,
- P95 ≤150 ms,
- P99 ≤200 ms.

The atomic results are P95 ≈1,020 ms and P99 ≈1,080 ms, far outside those targets ([acceptance criteria](./docs/DCS_gRPC_analyse/DCS_GRPC_LSO_RECOVERY_ANALYSIS.md:416)).

Final conclusion: the atomic implementation improved typical cadence and removed cross-request timestamp skew, but it has not improved operational grading reliability yet. The severe tail stalls remain—and in these two tests became worse—so this is not yet a successful performance validation.

> can you check the commit b84697239ce91d1d2d01ee604da47a0f72334360
> which was the version before we try to improve lso application then we work on recovery snapshot based on f962498109b78eac40c16af14962a888024f17fe

<details><summary>10 previous messages</summary>

> I’ll verify both commits directly and reconstruct the actual change boundary: `b846972` as the pre-improvement LSO version, then `f962498` as the recovery-snapshot base. I’m using the graphify workflow for code-history navigation, but Git ancestry and the source at each commit will be the authoritative evidence.
>
> <details><summary>Ran 3 commands</summary>
>
> - Ran `$skill='C:\Users\thierry\.agents\skills\graphify\SKILL.md'; Get-Content -Raw -LiteralPath $skill; if (Test-Path -LiteralPath '.\AGENTS.md') { Get-Content -Raw -LiteralPath '.\AGENTS.md' }; rg -n "b846972|f962498|recovery snapshot|atomic snapshot|GetRecoverySnapshot" "C:\Users\thierry\.codex\memories\MEMORY.md"`
> - Ran `Get-Content -Raw -LiteralPath 'C:\Users\thierry\.agents\skills\graphify\references\query.md'; if (Test-Path -LiteralPath 'graphify-out\graph.json') { Get-Item -LiteralPath 'graphify-out\graph.json' | Select-Object FullName,Length,LastWriteTime; Get-Content -Raw -LiteralPath 'graphify-out\.graphify_python' -ErrorAction SilentlyContinue }`
> - Ran `$data = Get-Content -Raw -LiteralPath 'graphify-out\graph.json' | ConvertFrom-Json; $tokens = [System.Collections.Generic.HashSet[string]]::new([System.StringComparer]::OrdinalIgnoreCase); foreach ($n in $data.nodes) { foreach ($m in [regex]::Matches([string]$n.label, '[A-Za-z]{3,30}')) { [void]$tokens.Add($m.Value.ToLowerInvariant()) } }; $tokens | Where-Object { $_ -match 'recover|snapshot|transform|record|telemetry|hook|gate|grpc|datum|sample|track' } | Sort-Object`
>
> </details>
>
> Graph query expanded to the repository graph’s vocabulary: `[recovery, record, transform, hook, gate, datums, grpc, track]`. The graph predates both commits, so I’ll use it only to locate the acquisition/grading path; commit trees and diffs will determine what actually changed.
>
> <details><summary>Ran 2 commands</summary>
>
> - Ran `$py=(Get-Content -Raw -LiteralPath 'graphify-out\.graphify_python').Trim(); $code=@'
> import json
> from pathlib import Path
> from networkx.readwrite import json_graph
> p=Path('graphify-out/graph.json')
> data=json.loads(p.read_text(encoding='utf-8'))
> G=json_graph.node_link_graph(data, edges='links')
> terms='recovery record transform hook gate datums grpc track'.split()
> sc=[]
> for nid,d in G.nodes(data=True):
>     label=(d.get('label') or '').lower()
>     score=sum(t in label for t in terms)
>     if score: sc.append((score,str(d.get('label',nid)),nid))
> for score,label,nid in sorted(sc, reverse=True)[:12]:
>     d=G.nodes[nid]
>     print(f'NODE {label} score={score} src={d.get("source_file","")} loc={d.get("source_location","")}')
>     for nb in list(G.neighbors(nid))[:8]:
>         nd=G.nodes[nb]
>         raw=G[nid][nb]
>         edge=next(iter(raw.values()),{}) if G.is_multigraph() else raw
>         print(f'  --{edge.get("relation","")}--> {nd.get("label",nb)} [{edge.get("confidence","")}] src={nd.get("source_file","")}')
> '@; & $py -c $code`
> - Ran `$a='b84697239ce91d1d2d01ee604da47a0f72334360'; $b='f962498109b78eac40c16af14962a888024f17fe'; git status --short; git show -s --date=iso-strict --format='COMMIT %H%nPARENTS %P%nAUTHOR %an <%ae>%nDATE %ad%nSUBJECT %s%nBODY%n%b' $a; git show -s --date=iso-strict --format='COMMIT %H%nPARENTS %P%nAUTHOR %an <%ae>%nDATE %ad%nSUBJECT %s%nBODY%n%b' $b; $mb=git merge-base $a $b; "MERGE_BASE $mb"; git merge-base --is-ancestor $a $b; "A_IS_ANCESTOR_OF_B exit=$LASTEXITCODE"; git log --oneline --decorate --ancestry-path "$a..$b"; git diff --stat $a $b; git diff --name-status $a $b; git branch --all --contains $a; git branch --all --contains $b`
>
> </details>
>
> The ancestry check confirms `b846972` is exactly the merge base and a direct ancestor of `f962498`. The boundary is larger than telemetry alone—about 6,000 added lines across 39 files—so I’m separating three generations: pre-improvement (`b846972`), post-August-31 LSO improvements (`f962498`), and the later snapshot-branch atomic acquisition changes.
>
> <details><summary>Ran 10 commands</summary>
>
> - Ran `$data=Get-Content -Raw -LiteralPath 'graphify-out\graph.json' | ConvertFrom-Json; $terms=@('recovery','record','transform','hook','gate','datums','grpc','track'); $scores=@{}; foreach($n in $data.nodes){$label=[string]$n.label;$s=0;foreach($t in $terms){if($label.ToLowerInvariant().Contains($t)){$s++}};if($s -gt 0){$scores[[string]$n.id]=[pscustomobject]@{Score=$s;Label=$label;Source=$n.source_file;Location=$n.source_location}}}; $top=$scores.GetEnumerator() | Sort-Object {$_.Value.Score} -Descending | Select-Object -First 12; foreach($e in $top){"NODE $($e.Value.Label) score=$($e.Value.Score) src=$($e.Value.Source) loc=$($e.Value.Location)"; foreach($link in $data.links | Where-Object { [string]$_.source -eq $e.Key -or [string]$_.target -eq $e.Key } | Select-Object -First 8){$other=if([string]$link.source -eq $e.Key){[string]$link.target}else{[string]$link.source};$o=$scores[$other];if(-not $o){$node=$data.nodes|Where-Object{[string]$_.id -eq $other}|Select-Object -First 1;$ol=[string]$node.label}else{$ol=$o.Label};"  --$($link.relation)--> $ol [$($link.confidence)]"}}`
> - Ran `$base='f962498109b78eac40c16af14962a888024f17fe'; $head=git rev-parse HEAD; "HEAD $head"; git show -s --date=iso-strict --format='COMMIT %H%nPARENTS %P%nDATE %ad%nSUBJECT %s%nBODY%n%b' HEAD; $mb=git merge-base $base HEAD; "MERGE_BASE_WITH_HEAD $mb"; git merge-base --is-ancestor $base HEAD; "F962_IS_ANCESTOR_OF_HEAD exit=$LASTEXITCODE"; git log --oneline --decorate --ancestry-path "$base..HEAD"; git diff --stat $base HEAD; git diff --name-status $base HEAD`
> - Ran `$commits=@('b84697239ce91d1d2d01ee604da47a0f72334360','f962498109b78eac40c16af14962a888024f17fe','HEAD'); foreach($c in $commits){"COMMIT_SCAN $c"; git grep -n -E 'GetRecoverySnapshot|get_recovery_snapshot|TelemetryAligner|sample_gap_ms|sample_seq|acquisition_mode|MissedTickBehavior|interval\(|get_transform|hook_bypass|draw_argument' $c -- src ':!src/tests.rs'}`
> - Ran `$files=Get-ChildItem -LiteralPath 'trap_records\Legacy' -Filter '*20260831*.json' | Sort-Object Name; foreach($f in $files | Select-Object -First 1){$j=Get-Content -Raw -LiteralPath $f.FullName|ConvertFrom-Json; "LEGACY_FILE $($f.Name)"; $j|ConvertTo-Json -Depth 4 -Compress}; $atomic=Get-ChildItem -LiteralPath 'trap_records\recovery_snapshot' -Filter '*.json'|Sort-Object Name; foreach($f in $atomic){$j=Get-Content -Raw -LiteralPath $f.FullName|ConvertFrom-Json; "ATOMIC_META $($f.Name)"; [pscustomobject]@{schema_version=$j.schema_version;lso_commit=$j.lso_commit;server_version=$j.server_version;server_protocol=$j.server_protocol;acquisition_mode=$j.acquisition_mode;sample_count=@($j.datums).Count}|Format-List}`
> - Ran `$commits=@('b84697239ce91d1d2d01ee604da47a0f72334360','f962498109b78eac40c16af14962a888024f17fe','HEAD'); foreach($c in $commits){"COMMIT_SCAN $c"; git grep -n -E 'GetRecoverySnapshot|get_recovery_snapshot|TelemetryAligner|sample_gap_ms|sample_seq|acquisition_mode|MissedTickBehavior|interval\(|get_transform' $c -- 'src/tasks/record_recovery.rs' 'src/commands/run.rs' 'src/telemetry.rs' 'src/client/recovery_client.rs' 2>$null}`
> - Ran `$base='f962498109b78eac40c16af14962a888024f17fe'; git log --reverse --date=iso-strict --format='COMMIT %H%nDATE %ad%nSUBJECT %s%n' "$base..HEAD"; foreach($c in (git rev-list --reverse "$base..HEAD")){git show --stat --oneline --no-renames $c -- 'src' 'Cargo.toml' 'Cargo.lock' 'docs/DCS-gRPC-0.9.1/Scripts/DCS-gRPC/methods/recovery.lua' 'docs/DCS-gRPC-0.9.1/Docs/DCS-gRPC/protos/dcs/recovery/v0/recovery.proto'}`
> - Ran `$sets=@(@{Name='Legacy';Path='trap_records\Legacy';Filter='*20260831*.json'},@{Name='Atomic';Path='trap_records\recovery_snapshot';Filter='*.json'}); foreach($s in $sets){"SET $($s.Name)"; foreach($f in (Get-ChildItem -LiteralPath $s.Path -Filter $s.Filter|Sort-Object Name)){ $j=Get-Content -Raw -LiteralPath $f.FullName|ConvertFrom-Json; $names=$j.PSObject.Properties.Name; [pscustomobject]@{File=$f.Name;Schema=$j.schema_version;LsoVersion=$j.lso_version;LsoCommit=$j.lso_commit;ServerVersion=$j.dcs_grpc_version;ServerProtocol=$j.dcs_grpc_protocol;Acquisition=$j.acquisition_mode;Samples=@($j.datums).Count;TopFields=($names -join ',')} | Format-List }}`
> - Ran `$commits=@('b84697239ce91d1d2d01ee604da47a0f72334360','47d885b','53f69f5','7f6b22d','f962498109b78eac40c16af14962a888024f17fe','f0b7762','HEAD'); foreach($c in $commits){"SCHEMA $c"; git grep -n -E 'schema_version|lso_version|lso_commit|dcs_grpc_version|acquisition_mode' $c -- 'src/tasks/record_recovery.rs' 'src/data.rs' 2>$null}; "HISTORY"; git log --all --reverse --date=iso-strict --format='%H %ad %s' -S 'schema_version' -- 'src/tasks/record_recovery.rs' 'src/data.rs'; git log --all --reverse --date=iso-strict --format='%H %ad %s' -S 'TelemetryAligner' -- 'src/tasks/record_recovery.rs' 'src/telemetry.rs'`
> - Ran `$a='b84697239ce91d1d2d01ee604da47a0f72334360'; $b='f962498109b78eac40c16af14962a888024f17fe'; git diff --numstat $a $b -- src; git diff --dirstat=files,0 $a $b -- src; git diff --shortstat $a $b -- src; git diff --stat $a $b -- 'src/tasks/record_recovery.rs' 'src/telemetry.rs' 'src/track.rs' 'src/grading.rs' 'src/data.rs' 'src/commands/run.rs'; git log --reverse --date=iso-strict --format='COMMIT %h %ad %s' "$a..$b" -- 'src/tasks/record_recovery.rs' 'src/telemetry.rs' 'src/track.rs' 'src/grading.rs' 'src/data.rs' 'src/commands/run.rs'`
> - Ran ``$commits=@('b84697239ce91d1d2d01ee604da47a0f72334360','f962498109b78eac40c16af14962a888024f17fe','HEAD'); foreach($c in $commits){"ACQUISITION $c"; $lines=git show "$c`:src/tasks/record_recovery.rs"; for($i=0;$i -lt $lines.Count;$i++){if($lines[$i] -match 'get_transform|join!|try_join!|TelemetryAligner|acquisition_mode|get_recovery_snapshot|recovery_interval'){ $lo=[Math]::Max(0,$i-3);$hi=[Math]::Min($lines.Count-1,$i+8);for($k=$lo;$k -le $hi;$k++){"{0,5}: {1}" -f ($k+1),$lines[$k]};"..."}}}``
>
> </details>
>
> A key correction is emerging: the August 31 files are not recordings from `b846972`. Their schema fingerprints the intermediate `53f69f5` generation (`schema_version: 2`), created after the first LSO telemetry/grading work. `f962498` emits schema 3, while the snapshot branch emits schema 4. I’m locating the clean pre-improvement data window now so we don’t attribute the August 31 results to the wrong code.
>
> <details><summary>Ran 9 commands</summary>
>
> - Ran `$rows=foreach($f in Get-ChildItem -LiteralPath 'trap_records\Legacy' -Filter '*.json'){try{$j=Get-Content -Raw -LiteralPath $f.FullName|ConvertFrom-Json;$date=if($f.Name -match 'LSO-(\d{8})-'){$Matches[1]}else{'unknown'};[pscustomobject]@{Date=$date;Schema=if($null -eq $j.schema_version){'none'}else{[string]$j.schema_version};Samples=@($j.datums).Count;File=$f.Name}}catch{[pscustomobject]@{Date='parse-error';Schema='';Samples=0;File=$f.Name}}}; $rows|Group-Object Date,Schema|Sort-Object Name|ForEach-Object{[pscustomobject]@{Date=($_.Name -split ', ')[0];Schema=($_.Name -split ', ')[1];Records=$_.Count;Samples=($_.Group|Measure-Object Samples -Sum).Sum;First=($_.Group|Sort-Object File|Select-Object -First 1).File;Last=($_.Group|Sort-Object File|Select-Object -Last 1).File}}|Format-Table -AutoSize; $candidate=$rows|Where-Object{$_.Date -eq '20260830' -and $_.Schema -eq 'none'}|Select-Object -First 1; if($candidate){$j=Get-Content -Raw -LiteralPath (Join-Path 'trap_records\Legacy' $candidate.File)|ConvertFrom-Json; "CANDIDATE $($candidate.File)"; "TOP_FIELDS $($j.PSObject.Properties.Name -join ',')"; "GATES"; $j.gate_deviations|ConvertTo-Json -Depth 5}`
> - Ran ``$script=@'
> const fs=require('fs'),path=require('path');
> function load(dir,pred){return fs.readdirSync(dir).filter(n=>n.endsWith('.json')).sort().map(n=>({n,j:JSON.parse(fs.readFileSync(path.join(dir,n),'utf8'))})).filter(x=>pred(x.n,x.j));}
> function pct(a,p){if(!a.length)return null;const s=[...a].sort((x,y)=>x-y),i=(s.length-1)*p,lo=Math.floor(i),hi=Math.ceil(i);return s[lo]+(s[hi]-s[lo])*(i-lo);}
> function stats(name,rows){let gaps=[],groove=[],samples=0,duration=0,present=0,valid=0,total=0;for(const {j} of rows){const d=j.datums||[];samples+=d.length;if(d.length>1)duration+=d[d.length-1].time-d[0].time;for(let i=1;i<d.length;i++){const g=(d[i].time-d[i-1].time)*1000;if(g>=0&&Number.isFinite(g)){gaps.push(g);if(d[i-1].x>=-200&&d[i-1].x<=1389&&d[i].x>=-200&&d[i].x<=1389)groove.push(g);}}const gd=j.gate_deviations||{};for(const key of ['at_three_quarter_nm','at_half_nm','at_quarter_nm']){total++;const x=gd[key];if(x){present++;if(x.valid===true)valid++;}}}
> const out={name,records:rows.length,samples,duration_s:+duration.toFixed(1),hz:+(samples/duration).toFixed(2),p50:+pct(gaps,.5).toFixed(1),p90:+pct(gaps,.9).toFixed(1),p95:+pct(gaps,.95).toFixed(1),p99:+pct(gaps,.99).toFixed(1),max:+Math.max(...gaps).toFixed(1),over300:+(100*gaps.filter(x=>x>300).length/gaps.length).toFixed(1),over1000:+(100*gaps.filter(x=>x>1000).length/gaps.length).toFixed(1),groove_n:groove.length,groove_p90:+pct(groove,.9).toFixed(1),groove_p95:+pct(groove,.95).toFixed(1),groove_over300:+(100*groove.filter(x=>x>300).length/groove.length).toFixed(1),gate_present:`${present}/${total}`,gate_valid:`${valid}/${total}`};console.log(JSON.stringify(out));}
> const legacy='trap_records/Legacy',atomic='trap_records/recovery_snapshot';
> stats('b846 clean window Aug30',load(legacy,(n,j)=>n.includes('20260830')&&j.schema_version==null));
> stats('intermediate schema2 Aug31',load(legacy,(n,j)=>n.includes('20260831')&&j.schema_version===2));
> stats('snapshot schema4 Sep1',load(atomic,(n,j)=>j.schema_version===4));
> '@; node -e $script; Get-ChildItem -LiteralPath 'trap_records\Legacy' -Filter '*20260830*.json'|Sort-Object Name|Select-Object -ExpandProperty Name``
> - Ran ``$specs=@(@{C='b84697239ce91d1d2d01ee604da47a0f72334360';Start=120;Count=115},@{C='f962498109b78eac40c16af14962a888024f17fe';Start=240;Count=190},@{C='HEAD';Start=250;Count=230}); foreach($s in $specs){"SOURCE $($s.C) record_recovery.rs lines $($s.Start+1)-$($s.Start+$s.Count)";$lines=git show "$($s.C)`:src/tasks/record_recovery.rs";$lines|Select-Object -Skip $s.Start -First $s.Count|ForEach-Object -Begin{$n=$s.Start+1}-Process{"{0,5}: {1}" -f $n,$_ ;$n++}}``
> - Ran `git show -s --format='f0b7762 parents: %P' f0b7762; git show -s --format='53f69f5 parents: %P' 53f69f5; git show -s --format='7f6b22d parents: %P' 7f6b22d; git show -s --format='f962498 parents: %P' f962498; git diff --shortstat f962498 f0b7762; git diff --shortstat b846972 53f69f5; git diff --shortstat 53f69f5 7f6b22d; git show -s --date=iso-strict --format='%h %ad %s' 53f69f5 7f6b22d f0b7762`
> - Ran `$script=@'
> const fs=require('fs'),path=require('path');function rows(dir,pred){return fs.readdirSync(dir).filter(n=>n.endsWith('.json')).map(n=>({n,j:JSON.parse(fs.readFileSync(path.join(dir,n),'utf8'))})).filter(x=>pred(x.n,x.j));}function count(name,r){let a=[];for(const {j} of r){for(let i=1;i<j.datums.length;i++){let g=(j.datums[i].time-j.datums[i-1].time)*1000;if(g>=0)a.push(g)}}console.log(name,{pairs:a.length,over300:a.filter(x=>x>300).length,over1000:a.filter(x=>x>1000).length,max:Math.max(...a)});}count('b846-window',rows('trap_records/Legacy',(n,j)=>n.includes('20260830')&&j.schema_version==null));count('schema2-Aug31',rows('trap_records/Legacy',(n,j)=>n.includes('20260831')&&j.schema_version===2));count('snapshot',rows('trap_records/recovery_snapshot',(n,j)=>j.schema_version===4));
> '@; node -e $script; git status --short`
> - Ran `$commits=@('b846972','53f69f5','f962498','HEAD'); foreach($c in $commits){"GATES $c"; git grep -n -E 'stale_gate_bracket|GateDatum|gate.*valid|InsufficientGates|at_three_quarter_nm|MAX_GATE|GATE.*MS|interpolat' $c -- 'src/track.rs' 'src/grading.rs' 'src/tasks/record_recovery.rs' 2>$null | Select-Object -First 100}`
> - Ran ``$specs=@(@{C='b846972';Start=120;Count=42},@{C='53f69f5';Start=190;Count=105},@{C='HEAD';Start=250;Count=110}); foreach($s in $specs){"TRACK $($s.C)";$lines=git show "$($s.C)`:src/track.rs";$lines|Select-Object -Skip $s.Start -First $s.Count|ForEach-Object -Begin{$n=$s.Start+1}-Process{"{0,5}: {1}" -f $n,$_ ;$n++}}``
> - Ran ``$commits=@('b846972','53f69f5','7f6b22d','f962498','f0b7762','7654930','HEAD'); foreach($c in $commits){$cargo=git show "$c`:Cargo.toml"; $pkg=($cargo|Select-String '^version\s*=\s*"'|Select-Object -First 1).Line; "VERSION $c $pkg"}; git diff --unified=1 b846972 f962498 -- Cargo.toml | Select-Object -First 120``
> - Ran `$p='C:\Users\thierry\.codex\memories\MEMORY.md'; $lines=Get-Content -LiteralPath $p; for($i=95;$i -le 123;$i++){if($i -lt $lines.Count){'{0}: {1}' -f ($i+1),$lines[$i]}}`
>
> </details>
>
> The commit lineage is now clear. The September 1 snapshot work is based exactly on `f962498`: its implementation commit `f0b7762` has `f962498` as its sole parent. The important caveat is that the August 31 schema-v2 recordings came from the earlier `53f69f5` feature generation, so they measure the first improvement attempt—not the pristine `b846972` baseline.
>
> <details><summary>Plan</summary>
>
> Verified commit ancestry and fingerprinted the recovery JSON schemas against source history. Recomputed the clean b846972-era window separately from the August 31 schema-v2 and September 1 schema-v4 atomic cohorts.
>
> - [x] Verify both commit objects, dates, messages, and ancestry
> - [x] Identify the exact LSO acquisition and grading changes between the commits
> - [x] Map each test-data cohort to the code version that produced it
> - [x] Revise the performance conclusion using the correct commit boundary
>
> </details>
</details>

Yes. The commit lineage is correct, but it changes how we must label the datasets.

## Verified commit generations

| Generation | Commit | Recovery behavior |
|---|---|---|
| Pre-improvement | `b846972` — Aug 29 | Two concurrent `GetTransform` calls, separate hook RPC, no telemetry-quality schema, no stale-gate rejection |
| First improvement | `53f69f5` — Aug 31 00:14 | Schema 2, `TelemetryAligner`, gate-quality validation, `InsufficientGates` |
| Post-test integration | `7f6b22d` / merge `f962498` — Aug 31 | Schema 3, metrics/runtime identity, watchdog and hook-sampling improvements; still uses two transform requests |
| Recovery snapshot | `f0b7762` — Sep 1 | Direct child of `f962498`; adds `GetRecoverySnapshot`, atomic mode and schema 4 |
| Tested snapshot build | `7654930` and later | LSO version 0.3.0, DCS-gRPC 0.9.1 |

Git confirms:

- `b846972` is the merge base and an ancestor of `f962498`.
- `f0b7762` has exactly `f962498` as its parent.
- Therefore, the recovery-snapshot implementation was indeed based directly on `f962498`.

The current atomic/legacy selection is visible in [record_recovery.rs](./src/tasks/record_recovery.rs:265), with the atomic request path at [line 401](./src/tasks/record_recovery.rs:401).

## Important dataset correction

The 15 August 31 files are not from `b846972`.

They:

- contain `schema_version: 2`;
- were recorded between 15:54 and 21:03;
- precede `7f6b22d` at 21:53 and `f962498` at 23:51;
- match the `53f69f5` schema-2 generation.

Their exact executable SHA was not recorded, but they clearly represent the first improvement attempt—not the pristine pre-improvement version.

The best clean `b846972`-era window is the three August 30 Ghost-72/T-45 records: after the `b846972` merge and before `53f69f5`.

## Corrected performance comparison

Raw adjacent DCS timestamps:

| Cohort | Recoveries | Samples | Effective rate | Median | P90 | P95 | >300 ms | >1 second |
|---|---:|---:|---:|---:|---:|---:|---:|---:|
| Aug 30, `b846972` era | 3 | 2,665 | 4.48 Hz | 120 ms | 900 ms | 930 ms | 22.0% | 3/2,661 |
| Aug 31, schema-2 improvements | 15 | 11,055 | 4.52 Hz | 120 ms | 780 ms | 930 ms | 21.5% | 63/11,030 |
| Sep 1, atomic snapshot | 2 | 2,043 | 5.05 Hz | 90 ms | 450 ms | 1,020 ms | 18.9% | 113/2,041 |

Atomic versus the clean `b846972`-era window:

- Effective delivery improved approximately 12.7%.
- Median improved 120 → 90 ms.
- P90 improved 900 → 450 ms.
- Gaps above 300 ms improved 22.0% → 18.9%.
- P95 worsened 930 → 1,020 ms.
- More-than-one-second gaps worsened from 0.1% to 5.5%.

So the conclusion remains: atomic acquisition improves normal/typical cadence substantially, but the extreme stalls became much worse.

## Gate results are not directly comparable with `b846972`

The three August 30 records contain all nine gate values, but `b846972` had no gate-quality status, no 300 ms bracket limit, and no `InsufficientGates`. It could therefore produce a grade from stale samples.

The quality enforcement was introduced by the schema-2 generation. Current code rejects stale brackets in [track.rs](./src/track.rs:1542).

Therefore:

- We cannot conclude that `b846972` had better gate acquisition simply because it populated 9/9 gates.
- It was less strict and could silently accept unreliable evidence.
- The fair gate comparison is August 31 schema 2 versus September 1 schema 4.
- In the matched Ghost/T-45 records, both have 16.7% valid gates: 3/18 versus 1/6.

Final assessment: the recovery snapshot is correctly based on `f962498`, and it improves typical acquisition speed. It has not yet improved reliable grading evidence because timeout/missing-request clusters still create approximately one-second gate holes. The previous comparison remains valid as “August 31 first-improvement code versus atomic snapshot,” but it should not be described as “`b846972` versus atomic.”

The records cannot prove the exact binaries because the August 31 files have no commit field and the September 1 files report `lso_commit: unknown`.

> so what wass the main difference with `b846972`  , the goal is to balance quality gate with DCS and grpc perf
> if we cannot improve gaps , we cannot get grading
> being to strict with those perf lead to unusable application
> so what are your recommendation ?
> i am preparing mission with only carrier , no foothold busy mission to see if DCS is the cause
> until then i need you to get a improvement plan
> also i dint see if hook up or down was detected? how the application determine the argument to check
> if this is alos 1 point that can cause performance degradation we should get rid off it, and pakr asde the feature for touchngo
> don't modify any code yet

<details><summary>7 previous messages</summary>

> I’ll produce a no-code improvement plan focused on usable grading under real DCS limits. I’m using the graphify workflow to trace the current grading/acquisition dependencies, plus the DCS scripting guidance because hook state is read through the mission Lua environment. I’ll specifically separate the hook’s extra RPC cost in `b846972` from its embedded cost in the atomic snapshot.
>
> <details><summary>Ran 3 commands</summary>
>
> - Ran `Get-Content -Raw -LiteralPath 'C:\Users\thierry\.agents\skills\graphify\SKILL.md'; Get-Content -Raw -LiteralPath 'C:\Users\thierry\.agents\skills\dcs_scripting\SKILL.md'`
> - Ran `rg --files 'C:\Users\thierry\.agents\skills\dcs_scripting\references' | rg -i 'unit|draw|object'; Get-Content -Raw -LiteralPath 'C:\Users\thierry\.agents\skills\graphify\references\query.md'`
> - Ran `Get-Content -Raw -LiteralPath 'C:\Users\thierry\.agents\skills\dcs_scripting\references\func_getDrawArgumentValue.md'; Get-Content -Raw -LiteralPath 'C:\Users\thierry\.agents\skills\dcs_scripting\references\class_Unit.md'; $data=Get-Content -Raw -LiteralPath 'graphify-out\graph.json'|ConvertFrom-Json;$tokens=[System.Collections.Generic.HashSet[string]]::new([System.StringComparer]::OrdinalIgnoreCase);foreach($n in $data.nodes){foreach($m in [regex]::Matches([string]$n.label,'[A-Za-z]{3,30}')){[void]$tokens.Add($m.Value.ToLowerInvariant())}};$tokens|Where-Object{$_ -match 'hook|recover|gate|track|transform|draw|grade|telemetry|sample|touch|bolter'}|Sort-Object`
>
> </details>
>
> Graph query expanded to: `[recovery, transform, hook, hookclient, draw, gate, grade, track, bolter]`. I’ll use the graph as navigation only because it predates the snapshot branch; the current Rust and vendored Lua implementation remain authoritative.
>
> <details><summary>Ran 3 commands</summary>
>
> - Ran `rg -n -i "draw_argument|hook_state|hook_sample|hook_observation|hook_was|hook.*up|hook.*down|polarity|intentional.*bolter|touchandgo|touch_and_go|TouchAndGo|recovery_snapshot_timeout|recovery_telemetry|HookSampling" src docs/DCS-gRPC-0.9.1/Scripts/DCS-gRPC/methods/recovery.lua docs/DCS-gRPC-0.9.1/Docs/DCS-gRPC/protos/dcs/recovery/v0/recovery.proto README.md docs/ADMIN_GUIDE.md docs/BENCHMARK_PROTOCOL.md`
> - Ran `$data=Get-Content -Raw -LiteralPath 'graphify-out\graph.json'|ConvertFrom-Json;$terms=@('recovery','transform','hook','hookclient','draw','gate','grade','track','bolter');$scores=@{};foreach($n in $data.nodes){$label=[string]$n.label;$s=0;foreach($t in $terms){if($label.ToLowerInvariant().Contains($t)){$s++}};if($s){$scores[[string]$n.id]=[pscustomobject]@{Score=$s;Label=$label;Source=$n.source_file;Location=$n.source_location}}};$top=$scores.GetEnumerator()|Sort-Object {$_.Value.Score} -Descending|Select-Object -First 10;foreach($e in $top){"NODE $($e.Value.Label) score=$($e.Value.Score) src=$($e.Value.Source) loc=$($e.Value.Location)";foreach($link in $data.links|Where-Object{[string]$_.source -eq $e.Key -or [string]$_.target -eq $e.Key}|Select-Object -First 8){$other=if([string]$link.source -eq $e.Key){[string]$link.target}else{[string]$link.source};$node=$data.nodes|Where-Object{[string]$_.id -eq $other}|Select-Object -First 1;"  --$($link.relation)--> $($node.label) [$($link.confidence)] src=$($node.source_file)"}}`
> - Ran `$files=Get-ChildItem -LiteralPath 'trap_records\recovery_snapshot' -Filter '*.json'|Sort-Object Name;foreach($f in $files){$j=Get-Content -Raw -LiteralPath $f.FullName|ConvertFrom-Json;"FILE $($f.Name)";"OUTCOME $($j.outcome) CAUSE $($j.cause) GRADING $($j.grading) PASS $($j.pass_grade)";"HOOK_OBSERVATION";$j.hook_observation|ConvertTo-Json -Depth 8;"WIRE_ESTIMATION";$j.wire_estimation|ConvertTo-Json -Depth 8;"DATUM_FIELDS $((@($j.datums)[0].PSObject.Properties.Name)-join ',')";$statuses=@($j.datums|ForEach-Object{$_.hook_sample_status}|Where-Object{$_})|Group-Object|Sort-Object Name;"HOOK_STATUSES";$statuses|Select-Object Name,Count|Format-Table -AutoSize}`
>
> </details>
>
> The hook evidence is present in both atomic tests, but the application did not classify it as up or down for the T-45. The raw argument changed between roughly `1` and `0`, yet `interpreted_state` remained `unknown` because the code only has calibrated polarity for a specifically recognized aircraft. That means the T-45 currently pays the observation cost without receiving touch-and-go classification value.
>
> <details><summary>Ran 7 commands</summary>
>
> - Ran `$paths=@('src/track.rs','src/tasks/record_recovery.rs','src/client/recovery_client.rs','src/commands/run.rs','src/tasks/mod.rs','docs/DCS-gRPC-0.9.1/Scripts/DCS-gRPC/methods/recovery.lua','docs/DCS-gRPC-0.9.1/Docs/DCS-gRPC/protos/dcs/recovery/v0/recovery.proto');$ranges=@(@('src/track.rs',470,225),@('src/track.rs',1280,135),@('src/tasks/record_recovery.rs',115,80),@('src/tasks/record_recovery.rs',475,95),@('src/client/recovery_client.rs',1,115),@('src/commands/run.rs',55,45),@('src/commands/run.rs',285,32),@('src/tasks/mod.rs',1,65),@('docs/DCS-gRPC-0.9.1/Scripts/DCS-gRPC/methods/recovery.lua',1,70),@('docs/DCS-gRPC-0.9.1/Docs/DCS-gRPC/protos/dcs/recovery/v0/recovery.proto',1,65));foreach($r in $ranges){$p=$r[0];$start=[int]$r[1];$count=[int]$r[2];"FILE $p lines $start-$($start+$count-1)";$lines=Get-Content -LiteralPath $p;$lines|Select-Object -Skip ($start-1) -First $count|ForEach-Object -Begin{$n=$start}-Process{"{0,5}: {1}" -f $n,$_ ;$n++}}`
> - Ran `$commits=@('b846972','f962498','HEAD');foreach($c in $commits){"HOOK_RPC $c";git grep -n -E 'get_draw_argument_value|sample_hook|HookSamplingMode|draw_argument|aircraft_draw_argument' $c -- 'src/tasks/record_recovery.rs' 'src/client/unit_client.rs' 'src/tasks/mod.rs' 'src/commands/run.rs' 2>$null}`
> - Ran `$files=Get-ChildItem -LiteralPath 'trap_records\recovery_snapshot' -Filter '*.json'|Sort-Object Name;foreach($f in $files){$j=Get-Content -Raw -LiteralPath $f.FullName|ConvertFrom-Json;$h=$j.hook_observation;[pscustomobject]@{File=$f.Name;Aircraft=$j.aircraft_type;Samples=$h.successful_samples;Timeouts=$h.timeout_samples;Errors=$h.error_samples;Stale=$h.stale_samples;Groove=$h.samples_in_groove;FinalWindow=$h.samples_in_final_window;Min=$h.min_raw;Max=$h.max_raw;Final=$h.final_raw;State=$h.interpreted_state;Polarity=$h.polarity;Outcome=$j.outcome;Cause=$j.cause}|Format-List}`
> - Ran `$ranges=@(@('src/tasks/record_recovery.rs',475,95),@('src/track.rs',640,85),@('src/track.rs',1180,115),@('src/data.rs',250,205),@('src/tasks/record_recovery.rs',900,145));foreach($r in $ranges){$p=$r[0];$start=[int]$r[1];$count=[int]$r[2];"FILE $p lines $start-$($start+$count-1)";$lines=Get-Content -LiteralPath $p;$lines|Select-Object -Skip ($start-1) -First $count|ForEach-Object -Begin{$n=$start}-Process{"{0,5}: {1}" -f $n,$_ ;$n++}}`
> - Ran `rg -n "SAMPLE_GAP_WARNING_MS|SAMPLE_GAP_INCOMPLETE_MS|SKEW|SOURCE_AGE|stale_gate_bracket|GateStatus::" src/telemetry.rs src/track.rs; $ranges=@(@('src/telemetry.rs',1,235),@('src/track.rs',1510,85),@('src/track.rs',1125,45));foreach($r in $ranges){$p=$r[0];$start=[int]$r[1];$count=[int]$r[2];"FILE $p lines $start-$($start+$count-1)";$lines=Get-Content -LiteralPath $p;$lines|Select-Object -Skip ($start-1) -First $count|ForEach-Object -Begin{$n=$start}-Process{"{0,5}: {1}" -f $n,$_ ;$n++}}`
> - Ran `$lines=git show 'b846972:src/track.rs';$lines|Select-Object -Skip 450 -First 70|ForEach-Object -Begin{$n=451}-Process{"{0,5}: {1}" -f $n,$_ ;$n++}; rg -n "hook|GetRecoverySnapshot|SAMPLE_GAP_WARNING_MS|timeout" "C:\Users\thierry\.codex\memories\MEMORY.md"; git status --short`
> - Ran `$p='C:\Users\thierry\.codex\memories\MEMORY.md';$lines=Get-Content -LiteralPath $p;foreach($range in @(@(0,25),@(365,389))){for($i=$range[0];$i -le $range[1];$i++){if($i -lt $lines.Count){'{0}: {1}' -f ($i+1),$lines[$i]}}}; git status --short`
>
> </details>
>
> <details><summary>Plan</summary>
>
> Completed a read-only trace of the historical and current acquisition paths, DCS draw-argument call, hook calibration/classification, and gate rejection rules. The plan separates instrumentation, optional hook removal, acquisition pacing, and degraded-but-usable grading.
>
> - [x] Trace acquisition, hook sampling, and grading differences from b846972 to the snapshot branch
> - [x] Determine how draw argument 25 and hook up/down interpretation work
> - [x] Assess whether hook observation can materially cause the measured stalls
> - [x] Produce a staged no-code improvement and low-load mission test plan
>
> </details>
</details>

Your concern is correct: the application currently treats a quality warning as a hard grading failure. That is too strict for observed DCS performance.

However, merely changing 300 ms to 1,000 ms will not fully solve the two tests. We need both acquisition improvement and a degraded-but-usable grading mode.

## Main difference from `b846972`

| Area | `b846972` | Current atomic version |
|---|---|---|
| Acquisition | Two `GetTransform` RPCs plus a separate hook RPC | One `GetRecoverySnapshot` RPC |
| Approximate CATOBAR load | Up to 30 mission requests/sec | 10 mission requests/sec |
| Timestamp | Carrier and aircraft could have different timestamps | One common DCS timestamp |
| Gate capture | First inbound sample below each distance | Interpolation between two bracketing samples |
| Gap validation | None | Rejects gate bracket above 300 ms |
| Missing telemetry | Could silently grade poor evidence | Reports `InsufficientGates` |
| Hook logic | Argument 25 read every tick; sticky “ever up” state | Embedded argument read; calibrated recent state, but only for F/A-18C |

`b846972` appeared more usable because it graded through telemetry gaps. It could capture a ¾-mile gate using a sample already significantly inside ¾ mile, without reporting how old or distant that sample was.

The current interpolation is better, but its policy is binary:

- ≤300 ms: accepted.
- >300 ms: rejected.
- One rejected gate makes the complete pass `Incomplete`.

The code already defines 300 ms as a warning and 1,000 ms as incomplete in [telemetry.rs](./src/telemetry.rs:5), but gate capture reuses the 300 ms warning as a hard failure in [track.rs](./src/track.rs:1539). That is the policy imbalance.

## Hook detection

The application does not discover the hook argument dynamically. It hardcodes external-model draw argument `25`:

- All arrested-recovery aircraft request argument 25.
- V/STOL aircraft do not request it.
- In atomic mode, argument 25 is included in the existing snapshot request at [record_recovery.rs](./src/tasks/record_recovery.rs:264).
- Lua calls `aircraft:getDrawArgumentValue(25)` inside the same callback at [recovery.lua](./docs/DCS-gRPC-0.9.1/Scripts/DCS-gRPC/methods/recovery.lua:32).

DCS draw arguments are aircraft-model-specific. An invalid argument returns zero rather than an explicit error, according to the [DCS scripting reference](~/.agents/skills/dcs_scripting/references/func_getDrawArgumentValue.md:1). Therefore, hardcoding 25 for every arrested aircraft is not a robust capability definition.

For the two T-45 tests:

- Argument values were successfully collected.
- Both recorded values between approximately 0 and 1.
- There were zero reported hook timeouts or errors.
- Nevertheless, `interpreted_state` remained `unknown`.
- `polarity` was `unknown_pending_live_validation`.

That happens because current classification only interprets hook state for the exact aircraft name `F/A-18C Hornet`, using:

- raw ≤0.2: hook up;
- raw ≥0.8: hook down;
- at least three stable samples over 0.4 seconds for up;
- at least two stable samples over 0.2 seconds for down.

See [track.rs](./src/track.rs:1357).

So yes: for the T-45, hook state is currently collected but provides no classification benefit.

### Hook performance impact

In `b846972`, hook observation was a separate RPC and could materially increase queue load.

In atomic mode, it is not another RPC. It is one additional DCS function call inside the existing Lua callback. Removing it will reduce a little Lua work, but it will not reduce atomic request count from 10/sec.

Therefore:

- It is sensible to disable hook-state observation for the T-45.
- It is unlikely by itself to eliminate the approximately one-second stalls.
- An A/B test with hook disabled is still worthwhile.

The static aircraft hook geometry used to estimate wire crossings is separate from draw argument 25. It requires no extra RPC and should remain. We can park only the hook-up/touch-and-go classification feature.

## Recommended improvement plan

### Phase 1 — Empty-mission baseline

Use your carrier-only mission with:

- the same CVN-72 and T-45;
- the current binary and 250 ms deadline;
- no Foothold scripts;
- no dashboard or streaming clients;
- preferably 6–10 comparable approaches.

Measure:

- successful versus attempted sequences;
- P50/P90/P95/P99 raw DCS gaps;
- gaps above 300 ms and 1,000 ms;
- consecutive missing-request runs;
- each gate’s actual bracket duration;
- RPC status and timing from the logs.

Interpretation:

- If the one-second holes remain, Foothold is not the primary cause.
- If they largely disappear, mission-script contention or mission callback load is a major contributor.
- If RTT remains low but DCS timestamps jump, DCS simulation/callback cadence is the problem.
- If RTT rises before every gap, the gRPC/IPC/Lua queue path is implicated.

### Phase 2 — Fix observability first

Before changing grading rules:

1. Preserve every attempted snapshot sequence, including failures.
2. Persist the exact gRPC status: deadline, cancellation, unavailable, queue-full, etc.
3. Record scheduled tick, request start, server queue wait, Lua execution time and response time.
4. Stop resetting the aligner in a way that hides the real interval after a failure.
5. Embed the actual LSO and server commit hashes.
6. Record whether the hook argument was requested.

Acceptance: every missing interval can be assigned to client scheduling, transport, server queue or DCS Lua execution.

### Phase 3 — Park hook-state classification

Add an explicit capability policy later:

- `off`: never request a hook draw argument.
- `auto`: request it only for aircraft with a validated argument and polarity.
- `diagnostic`: collect raw values without affecting outcome.

Initial capability table:

- T-45: off.
- F-14: off until calibrated.
- F/A-18C: optional/validated.
- AV-8B: not requested, unchanged.

While disabled:

- Ordinary recovered/bolter grading continues.
- Wire crossing geometry remains.
- Touch-and-go/qualification-bolter classification is parked.
- Do not interpret an unavailable hook as down.

### Phase 4 — Stabilize acquisition pacing

The current combination is problematic:

- scheduled every 100 ms;
- only one request can complete at a time;
- request timeout is 250 ms;
- missed ticks are skipped.

A single slow request can consume multiple sampling periods and produce the repeated three-sequence holes we observed.

Test this matrix before selecting defaults:

| Rate | Deadline | Purpose |
|---:|---:|---|
| 10 Hz | 250 ms | Current reference |
| 10 Hz | 500 ms | Determine whether responses arrive rather than time out |
| 5 Hz | 500 ms | Likely sustainable operating point |
| 5 Hz | 750 ms | Diagnose slow but recoverable DCS callbacks |

Keep only one request in flight and ensure cancelled requests are removed from the server queue. Do not allow timed-out work to execute later and create backlog.

My expected practical default is likely 5 Hz with a measured deadline around 500 ms, but the empty-mission results should decide that.

### Phase 5 — Introduce graded telemetry quality

Replace the binary gate policy with three levels:

- **High quality:** bracket ≤300 ms.
- **Degraded:** 300–1,000 ms, but only when both samples are valid, inbound, monotonic, correctly lined up and within a safe spatial span.
- **Invalid:** gap >1,000 ms, invalid geometry, timestamp reversal or excessive spatial uncertainty.

Then separate grade availability:

- Three high-quality gates: official grade.
- Three gates with one or more degraded: provisional grade with a telemetry warning.
- Two usable gates: provisional two-gate grade, clearly marked as estimated.
- Fewer than two usable gates: `Incomplete`.

A degraded grade should remain visible on the trapsheet and greenie board. Whether it receives leaderboard points can be a separate policy.

This makes the application usable without pretending that a one-second interpolation has the same confidence as a 100 ms observation.

### Why threshold relaxation alone is insufficient

The six atomic gate brackets were:

- First pass: 1,080 / 960 / 990 ms.
- Second pass: 90 / 900 / 1,080 ms.

At a 1,000 ms threshold, each pass would still have only two gates. Therefore, both would remain incomplete if three gates are mandatory.

A usable fallback needs either:

- a provisional two-gate grade, or
- a quantified low-confidence reconstruction of the missing gate.

I recommend the provisional two-gate grade first because it is simpler and more honest.

## Recommended implementation order

1. Run and analyze the empty-mission tests.
2. Add complete timing/failure instrumentation.
3. Disable hook observation for unsupported aircraft, beginning with T-45.
4. Test 5 Hz and longer deadlines.
5. Replay all existing records with high/degraded/invalid gate policies.
6. Add provisional grading without altering official-grade rules.
7. Validate under both empty and Foothold mission load.

No code or repository files were modified.

> first can you explain what is the purpose of --recovery-snapshot-timeout-ms 250
> what impact when increase/decrease?

<details><summary>4 previous messages</summary>

> I’ll trace the timeout through the CLI, gRPC request, scheduler, and error path so the explanation matches the current snapshot branch exactly. The key distinction is that this is a client deadline for one atomic request—not a direct setting for DCS sampling speed.
>
> <details><summary>Ran 2 commands</summary>
>
> - Ran `Get-Content -Raw -LiteralPath 'C:\Users\thierry\.agents\skills\graphify\SKILL.md'; Get-Content -Raw -LiteralPath 'C:\Users\thierry\.agents\skills\graphify\references\query.md'; rg -n "recovery_snapshot_timeout|recovery-snapshot-timeout|request_with_timeout|DeadlineExceeded|recovery_interval|MissedTickBehavior|may_fallback_to_legacy|telemetry_aligner.reset" src README.md docs`
> - Ran `$data=Get-Content -Raw -LiteralPath 'graphify-out\graph.json'|ConvertFrom-Json;$tokens=[System.Collections.Generic.HashSet[string]]::new([System.StringComparer]::OrdinalIgnoreCase);foreach($n in $data.nodes){foreach($m in [regex]::Matches([string]$n.label,'[A-Za-z]{3,30}')){[void]$tokens.Add($m.Value.ToLowerInvariant())}};$tokens|Where-Object{$_ -match 'recover|request|timeout|interval|snapshot|deadline|grpc|telemetry|transform'}|Sort-Object`
>
> </details>
>
> Graph query expanded to: `[recovery, interval, grpc, transform, handlerequest]`. The graph has no snapshot-timeout vocabulary because it predates that feature, so I’m relying on the current request and error paths for the timing details.
>
> <details><summary>Ran 3 commands</summary>
>
> - Ran `$data=Get-Content -Raw -LiteralPath 'graphify-out\graph.json'|ConvertFrom-Json;$terms=@('recovery','interval','grpc','transform','handlerequest');$scores=@{};foreach($n in $data.nodes){$label=[string]$n.label;$score=0;foreach($t in $terms){if($label.ToLowerInvariant().Contains($t)){$score++}};if($score){$scores[[string]$n.id]=[pscustomobject]@{Score=$score;Label=$label;Source=$n.source_file;Location=$n.source_location}}};$scores.GetEnumerator()|Sort-Object {$_.Value.Score} -Descending|Select-Object -First 8|ForEach-Object{"NODE $($_.Value.Label) score=$($_.Value.Score) src=$($_.Value.Source) loc=$($_.Value.Location)"}`
> - Ran `$ranges=@(@('src/commands/run.rs',65,18),@('src/client/mod.rs',1,36),@('src/client/recovery_client.rs',34,70),@('src/tasks/record_recovery.rs',255,50),@('src/tasks/record_recovery.rs',375,82),@('src/utils/interval.rs',1,28));foreach($r in $ranges){$p=$r[0];$start=[int]$r[1];$count=[int]$r[2];"FILE $p lines $start-$($start+$count-1)";$lines=Get-Content -LiteralPath $p;$lines|Select-Object -Skip ($start-1) -First $count|ForEach-Object -Begin{$n=$start}-Process{"{0,5}: {1}" -f $n,$_ ;$n++}}`
> - Ran `rg -n "snapshot.*timeout|100-299|250 ms|MissedTickBehavior::Skip|DeadlineExceeded" 'C:\Users\thierry\.codex\memories\MEMORY.md'`
>
> </details>
</details>

`--recovery-snapshot-timeout-ms 250` sets the maximum time the LSO client will wait for one atomic `GetRecoverySnapshot` response.

It does not control DCS simulation speed or directly set the sampling frequency. Sampling is scheduled every 100 ms; the timeout limits how long each request may block that sampling loop.

## What happens at 250 ms

For each scheduled observation:

1. LSO sends one atomic snapshot request.
2. The server queues and executes the DCS Lua callback.
3. The callback reads carrier, aircraft and optional hook state.
4. If the complete response returns within 250 ms, the datum is recorded.
5. If it takes longer, the client receives `DeadlineExceeded`.
6. No datum is recorded for that sequence.
7. The aligner is reset and the recorder continues with a later tick.

The option is defined as 100–299 ms in [run.rs](./src/commands/run.rs:74), applied to the gRPC request in [recovery_client.rs](./src/client/recovery_client.rs:34), and handled as a missing observation in [record_recovery.rs](./src/tasks/record_recovery.rs:433).

### Timing example

With 100 ms ticks and a 250 ms timeout:

```text
0 ms      request starts
100 ms    tick missed because request still running
200 ms    tick missed
250 ms    request times out
300 ms    next request can start
```

Therefore, one timeout can already create an approximately 300 ms hole. Three consecutive timeouts can produce the observed approximately 900–1,100 ms gap.

Missed ticks are deliberately skipped rather than replayed in a burst, as shown in [interval.rs](./src/utils/interval.rs:15).

## Impact of decreasing the timeout

For example, changing 250 → 150 ms:

Advantages:

- A slow request blocks the recorder for less time.
- The client can move to a later request sooner.
- It reduces exposure to a completely stuck request.

Disadvantages:

- More responses are discarded even though they might have completed successfully.
- More missing sequences and gate holes.
- Greater probability of `InsufficientGates`.

Your successful atomic calls already had approximately:

- P90: 110 ms
- P95: 143 ms
- maximum: 250 ms

A 100–150 ms timeout would therefore discard a significant number of calls that currently succeed. I do not recommend decreasing it.

## Impact of increasing the timeout

For example, changing 250 → 299 ms:

Advantages:

- Responses completing between 250 and 299 ms may be retained.
- Fewer deadline errors.
- Potentially fewer missing sequences.

Disadvantages:

- The sole in-flight request blocks longer.
- More 100 ms ticks are skipped while waiting.
- A response can succeed but still produce a gate bracket over 300 ms.
- Event-stream processing inside the same recorder loop can also be delayed.
- It cannot rescue calls taking 900–1,100 ms.

So a higher timeout trades “discard the slow request” for “wait longer and possibly receive it.” It does not make the server or DCS faster.

## Why the maximum is currently 299 ms

The code assumes a snapshot received after 300 ms is already too stale for a valid gate. That is why the CLI prevents values above 299 ms.

But this couples two different policies:

- **RPC deadline:** how long the client should wait.
- **Gate quality:** whether the received evidence is precise enough for official grading.

Those should eventually be separated. A 500 ms response might still be useful as degraded evidence even if it is not high-quality official evidence.

The current executable cannot accept 500 ms; the allowed range is only 100–299 ms. My earlier 500 ms suggestion was for a later controlled code experiment, not an available current CLI setting.

## One additional effect in `auto` mode

At the start of every recovery, the application sends an initial snapshot request to determine whether atomic mode is supported.

- `UNIMPLEMENTED` causes fallback to legacy mode.
- A timeout does not cause fallback.
- A timeout during that initial probe can abort starting that recovery recording.

That initial request uses the same 250 ms setting at [record_recovery.rs](./src/tasks/record_recovery.rs:265).

## Recommendation for your empty-mission test

Keep 250 ms for the first carrier-only tests. That preserves an exact comparison with the two existing atomic recoveries.

After establishing that baseline, 299 ms can be tested diagnostically. I would not expect the additional 49 ms to solve the one-second holes, but it can reveal whether there is a population of requests completing just above 250 ms.

In short:

- Lower than 250 ms: likely worse for grading.
- 250 ms: current balanced but aggressive deadline.
- 299 ms: may save a few borderline responses.
- Above 300 ms: requires a code and grading-policy change.
- The approximately one-second holes indicate a deeper queue/DCS scheduling problem, not simply a poorly chosen 250 ms value.

> what need to be modifed to accept 500ms? just for curiosity

<details><summary>4 previous messages</summary>

> I’ll distinguish the tiny change needed for the CLI to wait 500 ms from the larger change needed for a 500 ms response to remain gradable. Those are separate: allowing the number does not currently relax the 300 ms gate rule.
>
> <details><summary>Ran a command</summary>
>
> - Ran `Get-Content -Raw -LiteralPath 'C:\Users\thierry\.agents\skills\graphify\SKILL.md'; Get-Content -Raw -LiteralPath 'C:\Users\thierry\.agents\skills\graphify\references\query.md'`
>
> </details>
>
> Graph query expanded to: `[recovery, interval, grpc, transform]`. The graph predates the timeout option, so it only confirms the surrounding recovery loop; current source defines the exact edit surface.
>
> <details><summary>Ran 3 commands</summary>
>
> - Ran `$data=Get-Content -Raw -LiteralPath 'graphify-out\graph.json'|ConvertFrom-Json;$terms=@('recovery','interval','grpc','transform');foreach($n in $data.nodes){$label=[string]$n.label;$score=0;foreach($t in $terms){if($label.ToLowerInvariant().Contains($t)){$score++}};if($score){[pscustomobject]@{Score=$score;Label=$label;Source=$n.source_file;Location=$n.source_location}}}|Sort-Object Score -Descending|Select-Object -First 8|Format-Table -AutoSize`
> - Ran `rg -n "recovery_snapshot_timeout_ms|recovery-snapshot-timeout-ms|SAMPLE_GAP_WARNING_MS|SAMPLE_GAP_INCOMPLETE_MS|stale_gate_bracket|all_valid\(|GateStatus|request_with_timeout" src README.md docs/ADMIN_GUIDE.md docs/BENCHMARK_PROTOCOL.md; git status --short`
> - Ran `$data=Get-Content -Raw -LiteralPath 'graphify-out\graph.json'|ConvertFrom-Json;$terms=@('recovery','interval','grpc','transform');$rows=@();foreach($n in $data.nodes){$label=[string]$n.label;$score=0;foreach($t in $terms){if($label.ToLowerInvariant().Contains($t)){$score++}};if($score -gt 0){$rows += [pscustomobject]@{Score=$score;Label=$label;Source=$n.source_file;Location=$n.source_location}}};$rows|Sort-Object Score -Descending|Select-Object -First 8|Format-Table -AutoSize`
>
> </details>
</details>

To make the CLI accept `--recovery-snapshot-timeout-ms 500`, the essential functional change is only in [run.rs](./src/commands/run.rs:74):

```rust
// Current
.range(100..=299)

// Example expanded range
.range(100..=1000)
```

The timeout is already passed as a generic `Duration`, so these parts need no functional change:

- gRPC client
- protobuf
- DCS-gRPC server
- Lua recovery method

Supporting changes should include:

- Update the comment saying it must remain below 300 ms.
- Update the valid range in [ADMIN_GUIDE.md](./docs/ADMIN_GUIDE.md:35).
- Add a CLI test confirming that 500 is accepted and values above the chosen maximum are rejected.

But that only makes the client wait 500 ms. It does not make a 500 ms gate gradable.

The gate code would still reject any bracket above 300 ms as `stale_gate_bracket` in [track.rs](./src/track.rs:1539).

I would not simply change:

```rust
SAMPLE_GAP_WARNING_MS: 300.0
```

to 500, because that constant also controls:

- telemetry warning state;
- extrapolation-history validity;
- gate validity;
- wire-crossing freshness;
- event-to-wire correlation.

The cleaner future design would be separate limits:

```rust
HIGH_QUALITY_GATE_BRACKET_MS = 300.0;
MAX_USABLE_GATE_BRACKET_MS = 500.0;
```

Then classify:

- ≤300 ms: valid/high quality;
- 300–500 ms: degraded but usable;
- >500 ms: invalid.

That requires changes to `GateStatus`, `capture_gate`, `all_valid`, grading availability, renderer labels and tests.

One more consequence: with a 100 ms scheduler, waiting 500 ms can skip approximately five sampling ticks. A timeout could therefore create a 500–600 ms hole. It may recover slow responses, but it does not increase sampling performance.

So:

- **Accept 500 as a CLI value:** one functional parser change plus tests/docs.
- **Use 500 ms evidence for grading:** a broader, deliberate degraded-quality policy change.
- **Server/Lua changes:** none required.

No code was modified.

> let see next test&#x20;
> but i was think also if we cannot improve perf&#x20;
> that we will need to be less strict, like using 500ms or other method like `b846972`  ( find right balance)
> we could comment on grading with confidence level, for human to review the grade
> i was thinking also how tacview get all those data without our issue?

<details><summary>7 previous messages</summary>

> Agreed—we should wait for the carrier-only test before selecting a threshold. In parallel, I’ll compare our acquisition path with Tacview’s actual DCS export path. I’m using the code graph and DCS scripting references for our side, and official Tacview/DCS sources for Tacview so we don’t assume its smooth display means it receives perfect 10 Hz telemetry.
>
> <details><summary>Ran a command</summary>
>
> - Ran `Get-Content -Raw -LiteralPath 'C:\Users\thierry\.agents\skills\graphify\SKILL.md'; Get-Content -Raw -LiteralPath 'C:\Users\thierry\.agents\skills\graphify\references\query.md'; Get-Content -Raw -LiteralPath 'C:\Users\thierry\.agents\skills\dcs_scripting\SKILL.md'; rg -n "tacview|Writer|Record::Frame|write_acmi|recovery_interval|GetRecoverySnapshot|sample_gap|GateStatus|GateQuality" src README.md docs/BENCHMARK_PROTOCOL.md`
>
> </details>
>
> Context compaction
>
> Source: automatic
> Status: completed
>
> Graph query expanded to `[recovery, interval, transform, track, gate, grade, grpc, tacview]`. I’m using it only to locate the current acquisition and grading boundaries; the checked-out source remains authoritative because this graph predates the snapshot work.
>
> <details><summary>Ran a command, searched the web</summary>
>
> - Ran `$graph = Get-Content -Raw -LiteralPath '.\graphify-out\graph.json' | ConvertFrom-Json; $terms = @('recovery','interval','transform','track','gate','grade','grpc','tacview'); $matches = foreach ($node in $graph.nodes) { $haystack = (($node.id, $node.name, $node.label, $node.path, $node.kind, $node.type) -join ' ').ToLowerInvariant(); if ($terms | Where-Object { $haystack.Contains($_) }) { [pscustomobject]@{ id=$node.id; name=$node.name; path=$node.path; kind=$node.kind } } }; $matches | Select-Object -First 35 | Format-Table -AutoSize`
>
> Searched the web for `site:tacview.net/documentation DCS LuaExportAfterNextFrame Tacview exporter telemetry ...`
>
> Searched the web for `site:raia-software-inc.gitbook.io/tacview DCS World exporter recording interval ...`
>
> </details>
>
> Tacview’s smooth playback is an important clue, but not proof of equally dense raw samples: Tacview’s own documentation says its DCS exporter uses different rates by object (10 Hz for the player, 5 Hz for other aircraft, 1 Hz for many objects) and deliberately interpolates unchanged or sparse values. I’m checking the installed exporter too, if present, to separate documented behavior from inference.
>
> <details><summary>Ran 18 commands</summary>
>
> - Ran `$roots = @('C:\Users\thierry\Saved Games\DCS\Scripts','C:\Users\thierry\Saved Games\DCS.openbeta\Scripts','C:\ProgramData\Tacview','C:\Program Files (x86)\Tacview','C:\Program Files\Tacview'); foreach ($root in $roots) { if (Test-Path -LiteralPath $root) { Write-Output $root; rg --files $root | rg 'Tacview.*(Export|GameExport)|Export\.lua|Tacview.*\.lua' } }`
> - Ran `rg -n -C 4 "LuaExportActivityNextEvent|LuaExportAfterNextFrame|LoGetWorldObjects|LoGetSelfData|ExportPeriod|UpdatePeriod|Sampling|sample|frequency|NextEvent|Tacview.Events|Tacview.Context" 'C:\Users\thierry\Saved Games\DCS\Scripts\TacviewGameExport.lua' 'C:\Users\thierry\Saved Games\DCS\Scripts\Export.lua'`
> - Ran `$path = 'C:\Users\thierry\Saved Games\DCS\Scripts\TacviewGameExport.lua'; $lines = Get-Content -LiteralPath $path; for ($i = 0; $i -lt [Math]::Min($lines.Count, 180); $i++) { '{0,4}: {1}' -f ($i + 1), $lines[$i] }`
> - Ran `$path = 'C:\Users\thierry\Saved Games\DCS\Scripts\Export.lua'; $lines = Get-Content -LiteralPath $path; for ($i = 0; $i -lt $lines.Count; $i++) { '{0,4}: {1}' -f ($i + 1), $lines[$i] }`
> - Ran `rg -n -C 3 "tacview::Writer|Writer::new|write_frame|recovery_snapshot|GetRecoverySnapshot|interval\(|MissedTickBehavior|capture_gate|stale_gate_bracket|SAMPLE_GAP_WARNING_MS|SAMPLE_GAP_INCOMPLETE_MS" src`
> - Ran `$path = 'src\tasks\record_recovery.rs'; $lines = Get-Content -LiteralPath $path; foreach ($range in @(@(286,320),@(390,430),@(530,570))) { for ($i=$range[0]-1; $i -lt [Math]::Min($range[1],$lines.Count); $i++) { '{0,4}: {1}' -f ($i+1),$lines[$i] }; '' }`
> - Ran `rg -n -C 4 "function grpc\.onSimulationFrame|function grpc\.next|function grpc\.handleRequest|mission\.doString|net\.dostring_in|callbacks|queue" docs\dcs_grpc_0_9_0\Scripts\DCS-gRPC\grpc.lua docs\dcs_grpc_0_9_0\Scripts\DCS-gRPC\grpc-mission.lua`
> - Ran `rg --files docs | rg "grpc(-mission)?\.lua$|DCS-gRPC" | Select-Object -First 40`
> - Ran `rg -n -C 5 "function grpc\.onSimulationFrame|function grpc\.next|function grpc\.handleRequest|net\.dostring_in|mission\.doString|callbacks|queue" 'docs\DCS-gRPC-0.9.1\Scripts\DCS-gRPC\grpc.lua' 'docs\DCS-gRPC-0.9.1\Scripts\DCS-gRPC\grpc-mission.lua' 'docs\DCS-gRPC-0.9.1\Scripts\Hooks\DCS-gRPC.lua'`
> - Ran `Get-Content -LiteralPath 'docs\DCS-gRPC-0.9.1\Scripts\DCS-gRPC\grpc.lua' -TotalCount 240; Get-Content -LiteralPath 'docs\DCS-gRPC-0.9.1\Scripts\DCS-gRPC\grpc-mission.lua' -TotalCount 180; Get-Content -LiteralPath 'docs\DCS-gRPC-0.9.1\Scripts\Hooks\DCS-gRPC.lua' -TotalCount 180`
> - Ran `$path='C:\Users\thierry\Saved Games\DCS\Config\dcs-grpc.lua'; if (Test-Path -LiteralPath $path) { rg -n "throughputLimit|autostart|host|port" $path } else { 'No installed dcs-grpc config found' }`
> - Ran `rg --files 'C:\ProgramData\Tacview' 'C:\Users\thierry\Saved Games\DCS' | rg -i "tacview.*(cfg|ini|xml|lua)$|options.*lua$" | Select-Object -First 80`
> - Ran `rg -n -i -C 2 "tacview|flightDataRecording|recording|sampling|interval" 'C:\Users\thierry\Saved Games\DCS\Config\options.lua' 'C:\Users\thierry\Saved Games\DCS\Mods\tech\Tacview\Options\optionsData.lua' 'C:\Users\thierry\Saved Games\DCS\Mods\tech\Tacview\Options\optionsDb.lua'`
> - Ran `rg -n "local interval =|callsPerTick|grpc\.next|timer\.scheduleFunction" 'docs\DCS-gRPC-0.9.1\Scripts\DCS-gRPC\grpc.lua'; rg -n "tacviewDataCaptureMode|tacviewFlightDataRecordingEnabled|tacviewRealTimeTelemetryEnabled" 'C:\Users\thierry\Saved Games\DCS\Config\options.lua'; rg -n "dofile\(.*Tacview|DCS-BIOS|DCS-SimpleRadioStandalone|DCSDTC|Olympus|SimShaker" 'C:\Users\thierry\Saved Games\DCS\Scripts\Export.lua'`
> - Ran `$path='C:\Users\thierry\.codex\memories\MEMORY.md'; $lines=Get-Content -LiteralPath $path; foreach($range in @(@(20,28),@(116,124))){for($i=$range[0]-1;$i -lt $range[1];$i++){'{0}: {1}' -f ($i+1),$lines[$i]};''}`
> - Ran `rg -n -i "atomic snapshot|telemetry baseline|groove gaps|MAX_HOOK_EVIDENCE|Two unary|100 ms timer|schema_version" 'C:\Users\thierry\.codex\memories\MEMORY.md'`
> - Ran `$path='C:\Users\thierry\.codex\memories\MEMORY.md'; $lines=Get-Content -LiteralPath $path; for($i=144;$i -lt 174;$i++){'{0}: {1}' -f ($i+1),$lines[$i]}`
> - Ran `$path='C:\Users\thierry\.codex\memories\MEMORY.md'; $lines=Get-Content -LiteralPath $path; for($i=0;$i -lt 28;$i++){'{0}: {1}' -f ($i+1),$lines[$i]}`
>
> </details>
</details>

Yes—let’s keep `250 ms` for the next carrier-only test so the comparison remains clean. If performance is still insufficient, your confidence-based grading idea is the best compromise.

### Recommended grading balance

Do not return completely to `b846972`: it produced grades more easily because it accepted the first available sample without checking its age or interpolation quality. Instead, retain the new evidence and interpolation logic but soften the binary decision:

| Gate evidence | Result |
|---|---|
| Three gates, brackets ≤300 ms | Official grade — high confidence |
| Three gates, brackets 300–500 ms | Provisional grade — medium confidence |
| Two gates, or a bracket 500–1000 ms | Estimated grade — low confidence, human review |
| Fewer than two gates, >1000 ms, reordered/bad geometry | Incomplete |

The report could say, for example:

> Grade: OK 3 — provisional
> Confidence: Low
> 2/3 gates directly measured; ½-NM gate reconstructed across an 870 ms gap. Human review recommended.

This restores the usability philosophy of `b846972` without pretending uncertain telemetry is precise.

A `500 ms timeout` and a `500 ms gate limit` are different:

- Increasing the RPC timeout to 500 ms allows a slow request to finish, but can also delay the next sample.
- Allowing 500 ms gate brackets accepts wider interpolation and directly reduces `InsufficientGates`.
- In the two existing atomic tests, the brackets were `1080/960/990 ms` and `90/900/1080 ms`. Therefore, a 500 ms gate limit alone would recover only one of six gates. We need confidence-based fallback, not just one larger number.

### Why Tacview looks better

Your installed Tacview exporter loads a compiled C++ recorder and invokes it at the beginning and end of every DCS simulation frame: [TacviewGameExport.lua](<~/Saved Games/DCS/Scripts/TacviewGameExport.lua:18>). That avoids our chain:

```text
LSO timer → gRPC request → request queue → DCS mission scheduler
→ Lua snapshot → gRPC response → LSO processing
```

But Tacview does not necessarily receive every object at 10 Hz. Its documentation specifies approximately:

- Player aircraft: 10 Hz
- Other aircraft: 5 Hz
- Weapons: 8 Hz
- Ground/projectiles: 2 Hz
- Other objects: 1 Hz

Tacview also deliberately interpolates between updates, which makes playback look smooth even with sparse raw data. [Tacview data-size documentation](https://raia-software-inc.gitbook.io/tacview/technical-documentation/data-size-optimization-2)

Its real-time player also adjusts playback timing around received samples and tolerates temporary transmission pauses. [Tacview real-time telemetry protocol](https://raia-software-inc.gitbook.io/tacview/technical-documentation/real-time-telemetry-public-protocol)

Therefore, Tacview is optimized for smooth visualization; our application needs defensible measurements precisely at three gate crossings. Those are different accuracy requirements.

Also, the ACMI produced by the LSO application is not an independent source: it is written from the same gRPC snapshots used for grading in [record_recovery.rs](./src/tasks/record_recovery.rs:542). We need Tacview’s native DCS recording for comparison.

### Next-test recommendation

For the carrier-only mission:

1. Keep the LSO timeout at `250 ms`.
2. Keep native Tacview recording enabled.
3. Preserve the native Tacview ACMI, LSO snapshot directory, application log and `dcs.log`.
4. Compare both timestamp streams around the three gates.

Interpretation:

- Tacview dense, LSO sparse → gRPC queue/scheduler/client path is the main bottleneck.
- Both sparse at the same times → DCS simulation/export scheduling is the likely bottleneck.
- Carrier-only becomes clean → busy-mission load is the major contributor.

One caveat: the carrier-only mission still loads DCS-BIOS, SRS, DCSDTC, Olympus, SimShaker and Tacview through your [Export.lua](<~/Saved Games/DCS/Scripts/Export.lua:4>). It isolates mission complexity, but not exporter/plugin contention. If gaps remain, that should be the second controlled A/B test.

No code was modified.

> but lso and dcs are running on dedicated server does it matter if each player load SRS ,simshaker or whatever tool?, this should impact player PC not the server

<details><summary>4 previous messages</summary>

> Yes—if SRS, SimShaker, DCS-BIOS, and Tacview are installed only on each player’s PC, they do not execute inside the dedicated server and cannot directly slow its gRPC mission callback. My previous exporter-contention caveat used your local `Saved Games` configuration, so it does not apply to the server unless the same exporters are installed there. I’ll tighten the test interpretation around the actual server/client boundary using the DCS scripting and code-graph references.
>
> <details><summary>Explored a file, ran 2 commands</summary>
>
> <details><summary>Explored 2 files</summary>
>
> - Read `~/.agents/skills/graphify/SKILL.md`
> - Read `~/.agents/skills/dcs_scripting/SKILL.md`
>
> </details>
>
> - Ran `rg -n -i "dedicated server|client export|mission environment|server-side|GetRecoverySnapshot|export environment" 'C:\Users\thierry\.codex\memories\MEMORY.md'`
>
> <details><summary>Explored a file</summary>
>
> - Read `~/.agents/skills/graphify/references/query.md`
>
> </details>
>
> - Ran `$data=Get-Content -Raw -LiteralPath '.\graphify-out\graph.json' | ConvertFrom-Json; $vocab=[System.Collections.Generic.HashSet[string]]::new([System.StringComparer]::OrdinalIgnoreCase); foreach($n in $data.nodes){$label=[string]$n.label; foreach($m in [regex]::Matches($label,'[^\W\d_]+')){if($m.Value.Length -ge 3 -and $m.Value.Length -le 30){[void]$vocab.Add($m.Value.ToLowerInvariant())}}}; @('grpc','recovery','mission','hook','server','client','event','simulation','frame','export','transform','snapshot') | ForEach-Object { if($vocab.Contains($_)){$_} }`
>
> </details>
>
> Graph query expanded to (from graph vocabulary, 8 tokens): `[grpc, recovery, mission, hook, client, event, frame, transform]`. The graph does not model the separate player-PC exporter boundary, so I’ll use it only to confirm that recovery telemetry belongs to the server mission/gRPC path.
>
> <details><summary>Ran a command</summary>
>
> - Ran `$data=Get-Content -Raw -LiteralPath '.\graphify-out\graph.json' | ConvertFrom-Json; $terms=@('grpc','recovery','mission','hook','client','event','frame','transform'); $ranked=foreach($n in $data.nodes){$label=[string]$n.label; $score=($terms|Where-Object{$label.ToLowerInvariant().Contains($_)}).Count; if($score -gt 0){[pscustomobject]@{score=$score;label=$label;source_file=$n.source_file;source_location=$n.source_location;id=$n.id}}}; $ranked|Sort-Object score -Descending|Select-Object -First 24 score,label,source_file,source_location|Format-Table -AutoSize -Wrap`
>
> </details>
</details>

Correct. If those tools are installed only on each player’s PC, they do not directly load the dedicated server’s DCS process.

The important distinction is:

| Component | Direct server impact |
|---|---|
| Player’s SimShaker, DCS-BIOS, Tacview | No |
| Player’s SRS client/export script | Normally no |
| SRS server/export script installed on dedicated server | Yes |
| Tacview installed on dedicated server | Yes |
| DCS-gRPC mission Lua and server hook | Yes |
| LSO process sharing the server’s CPU | Yes |
| Busy mission, AI and scripting | Yes |

A badly performing player PC could indirectly produce delayed aircraft network updates. However, it should not cause the complete server-side atomic snapshot—including carrier, aircraft and DCS timestamp—to stop arriving for approximately one second. Those common gaps point more strongly toward the dedicated server’s mission scheduler, gRPC request queue, DCS frame/load, or host CPU scheduling.

My previous inspection was of:

`C:\Users\thierry\Saved Games\DCS\Scripts\Export.lua`

If that is your player PC, its SRS/SimShaker/Tacview configuration is irrelevant to server performance. We would need to inspect the dedicated server’s own profile, typically something such as:

```text
Saved Games\DCS_server\
Saved Games\DCS.openbeta_server\
```

### Important consequence for Tacview

If Tacview records on the player PC, its smooth recording is not an apples-to-apples comparison:

```text
Player Tacview → player DCS Export environment
LSO → dedicated-server gRPC → server mission environment
```

Player Tacview may receive smooth locally simulated aircraft movement while the dedicated server’s mission callback is delayed. For a meaningful comparison, we would need either:

- A native Tacview recording generated on the dedicated server, or
- Simultaneous player Tacview plus server telemetry, clearly treating them as two different viewpoints.

### Revised next-test plan

For the carrier-only test:

1. Keep `--recovery-snapshot-timeout-ms 250`.
2. Leave player tools unchanged—they are not the variable being tested.
3. Collect the LSO recovery snapshot and application log.
4. Collect the dedicated server’s `dcs.log`.
5. Note server CPU usage and DCS simulation performance.
6. Check only the dedicated server’s `Export.lua`, Hooks and installed services.

If the carrier-only mission removes the gaps, busy-mission/server load is implicated. If the one-second gaps remain, we should concentrate on the server’s gRPC queue and DCS mission scheduler—not the players’ SRS or SimShaker installations.

> also Tacview is actually set to not record on DCS server
> ACMI produced by the LSO application is not an independent source:  this mean that lso produce acmi by itslef not using tacview?
> so then why we cannot get also data from this or is it using our json record to creat acmi?

<details><summary>4 previous messages</summary>

> Exactly: the LSO application contains its own ACMI writer; it does not need Tacview to be installed or recording. I’ll trace the current source to distinguish the live gRPC input, the JSON evidence record, and the ACMI output. I’m using the code graph for navigation and the DCS scripting boundary to verify which process supplies each datum.
>
> <details><summary>Ran 3 commands</summary>
>
> - Ran `Get-Content -Raw -LiteralPath 'C:\Users\thierry\.agents\skills\graphify\SKILL.md'; Get-Content -Raw -LiteralPath 'C:\Users\thierry\.agents\skills\graphify\references\query.md'`
>
> <details><summary>Explored a file</summary>
>
> - Read `~/.agents/skills/dcs_scripting/SKILL.md`
>
> </details>
>
> - Ran `rg -n -i "ACMI|tacview|recovery snapshot|JSON record|recording" 'C:\Users\thierry\.codex\memories\MEMORY.md'`
>
> - Ran `$data=Get-Content -Raw -LiteralPath '.\graphify-out\graph.json'|ConvertFrom-Json;$vocab=[System.Collections.Generic.HashSet[string]]::new([System.StringComparer]::OrdinalIgnoreCase);foreach($n in $data.nodes){foreach($m in [regex]::Matches([string]$n.label,'[^\W\d_]+')){if($m.Value.Length-ge 3-and$m.Value.Length-le 30){[void]$vocab.Add($m.Value.ToLowerInvariant())}}};@('tacview','writer','record','recording','recovery','transform','frame','json','track','grpc','acmi')|ForEach-Object{if($vocab.Contains($_)){$_}}`
>
> </details>
>
> Graph query expanded to (from graph vocabulary, 7 tokens): `[record, recovery, transform, frame, json, track, grpc]`. “Tacview” and “ACMI” are absent from this older graph vocabulary, so I’ll traverse the available recording path and verify the missing details directly in the current Rust source.
>
> <details><summary>Ran 2 commands</summary>
>
> - Ran `$data=Get-Content -Raw -LiteralPath '.\graphify-out\graph.json'|ConvertFrom-Json;$terms=@('record','recovery','transform','frame','json','track','grpc');$ranked=foreach($n in $data.nodes){$label=([string]$n.label).ToLowerInvariant();$score=($terms|Where-Object{$label.Contains($_)}).Count;if($score-gt 0){[pscustomobject]@{score=$score;label=$n.label;source_file=$n.source_file;source_location=$n.source_location}}};$ranked|Sort-Object score -Descending|Select-Object -First 25|Format-Table -AutoSize -Wrap; rg -n -C 4 "Writer::new_compressed|write_acmi!|Record::Frame|serde_json|to_string_pretty|\.acmi|recording\.take|recording\.as_mut|finish\(|Track::new|next_sample" src\tasks\record_recovery.rs src`
> - Ran `$p='src\tasks\record_recovery.rs';$l=Get-Content -LiteralPath $p;foreach($range in @(@(360,570),@(850,930),@(960,1035),@(1070,1145))){for($i=$range[0]-1;$i-lt[Math]::Min($range[1],$l.Count);$i++){'{0,4}: {1}'-f($i+1),$l[$i]};''}`
>
> </details>
</details>

Yes. The LSO application creates the ACMI itself. It uses the Rust `tacview` library only to write the ACMI file format—it does not use Tacview.exe or the Tacview DCS exporter.

The actual flow is:

```text
DCS dedicated server
        │
        ▼
GetRecoverySnapshot through gRPC
        │
        ▼
One successful telemetry sample
        ├──► LSO ACMI writer → .zip.acmi
        └──► Track/grading    → JSON + charts + database
```

The ACMI is not created from the JSON. They are sibling outputs produced from the same live sample:

- The [ACMI writer](./src/tasks/record_recovery.rs:296) is opened in memory.
- The [gRPC snapshot](./src/tasks/record_recovery.rs:390) supplies the carrier and aircraft transforms.
- Those transforms are immediately written as ACMI frames.
- The same sample goes into `Track::next_sample()` for grading.
- At completion, the [JSON report](./src/tasks/record_recovery.rs:968) is generated from the finished track.

### Why the ACMI cannot restore missing samples

When a gRPC request times out or fails, the code executes `continue`. Therefore:

```text
No gRPC response
  → no TelemetrySample
  → no ACMI frame
  → no JSON datum
```

The ACMI contains the same gaps as the JSON evidence. Tacview may visually interpolate smoothly between the two recorded ACMI positions, but that interpolated movement is not an additional DCS measurement.

We can parse the LSO ACMI and recalculate a recovery—the project already supports ACMI replay in [file.rs](./src/commands/file.rs:51)—but the result cannot contain more raw information than originally received.

The JSON is actually richer for diagnostics because it contains:

- Sample gaps and telemetry health
- Request sequence and acquisition mode
- Gate bracket quality
- Hook evidence
- Events and completeness reasons
- Confidence and grading availability

The ACMI mainly contains:

- Frame timestamp
- Carrier position and orientation
- Aircraft position and orientation
- Aircraft angle of attack
- Landing/message events

So the LSO ACMI is valuable for visual human review and offline regrading, but it is not an independent backup telemetry source. With server Tacview recording disabled, there is currently no second independent recording on the dedicated server.

This reinforces the confidence approach: ACMI can help a human inspect a provisional grade, but it cannot prove what happened inside an unobserved 900–1100 ms interval.

> i understand that you want to stick to high quality , but as possible "workaround " combining both data source would help to get good balance
> at least initially to get good drawing for discord png output

<details><summary>4 previous messages</summary>

> That is a sensible workaround for the Discord PNG—as long as we separate “visual reconstruction” from “grading evidence.” I’ll inspect the current drawing path with the code graph and source, then propose where interpolation can be added without making an uncertain grade look authoritative. No code changes yet.
>
> <details><summary>Ran 3 commands</summary>
>
> - Ran `Get-Content -Raw -LiteralPath 'C:\Users\thierry\.agents\skills\graphify\SKILL.md'; Get-Content -Raw -LiteralPath 'C:\Users\thierry\.agents\skills\graphify\references\query.md'`
> - Ran `rg -n -i "presentation|drawing|display smoothing|ACMI|telemetry.*grading|Discord.*PNG|visual" 'C:\Users\thierry\.codex\memories\MEMORY.md'`
> - Ran `$data=Get-Content -Raw -LiteralPath '.\graphify-out\graph.json'|ConvertFrom-Json;$vocab=[System.Collections.Generic.HashSet[string]]::new([System.StringComparer]::OrdinalIgnoreCase);foreach($n in $data.nodes){foreach($m in [regex]::Matches([string]$n.label,'[^\W\d_]+')){if($m.Value.Length-ge 3-and$m.Value.Length-le 30){[void]$vocab.Add($m.Value.ToLowerInvariant())}}};@('draw','chart','discord','render','track','datum','datums','recovery','grade','grading','sample','telemetry','png','frame')|ForEach-Object{if($vocab.Contains($_)){$_}}`
>
> </details>
>
> Graph query expanded to (from graph vocabulary, 10 tokens): `[draw, chart, discord, track, datum, datums, recovery, grade, grading, png]`. I’m tracing whether the renderer can consume a presentation-only reconstructed path while the grading path retains the original samples.
>
> <details><summary>Ran 3 commands</summary>
>
> - Ran `$data=Get-Content -Raw -LiteralPath '.\graphify-out\graph.json'|ConvertFrom-Json;$terms=@('draw','chart','discord','track','datum','datums','recovery','grade','grading','png');$ranked=foreach($n in $data.nodes){$label=([string]$n.label).ToLowerInvariant();$score=($terms|Where-Object{$label.Contains($_)}).Count;if($score-gt 0){[pscustomobject]@{score=$score;label=$n.label;source_file=$n.source_file;source_location=$n.source_location}}};$ranked|Sort-Object score -Descending|Select-Object -First 30|Format-Table -AutoSize -Wrap; rg -n -C 4 "pub fn draw_chart|fn draw_chart|datums|LineSeries|PathElement|draw_series|draw_pattern|render_track|spawn_blocking|chart" src\draw.rs src\tasks\record_recovery.rs | Select-Object -First 260`
> - Ran `rg -n "pub fn draw_chart|pub fn draw_pattern_chart|fn draw_|LineSeries::new|PathElement::new|track\.datums|final_datums|select_final_approach_datums|DrawingArea" src\draw.rs; $p='src\draw.rs';$l=Get-Content -LiteralPath $p;foreach($range in @(@(100,245),@(350,580),@(900,1160))){for($i=$range[0]-1;$i-lt[Math]::Min($range[1],$l.Count);$i++){'{0,4}: {1}'-f($i+1),$l[$i]};''}; rg -n -A 35 "pub struct Datum" src\track.rs`
> - Ran `$p='src\draw.rs';$l=Get-Content -LiteralPath $p;for($i=1132;$i-lt1215;$i++){'{0,4}: {1}'-f($i+1),$l[$i]}; for($i=610;$i-lt835;$i++){'{0,4}: {1}'-f($i+1),$l[$i]}`
>
> </details>
</details>

Yes—for the Discord PNG, this is a good initial workaround. We can reconstruct a visually continuous path without weakening the grading rules.

Technically, ACMI and JSON are not two independent sources. The safest implementation would therefore not reread the ACMI. Instead, it would use the richer JSON/`TrackResult` datums and apply ACMI-style interpolation only inside the renderer:

```text
Raw telemetry ──► grading and JSON — unchanged
              └─► presentation interpolation ──► Discord PNG
```

The current renderer deliberately separates the trace whenever samples are more than one second apart in [draw.rs](./src/draw.rs:189). It even prints `TRACE PARTIAL` rather than joining telemetry outages. That protects accuracy, but produces an unattractive or confusing chart.

### Recommended visual policy

| Gap | Discord rendering |
|---|---|
| ≤300 ms | Normal solid measured trace |
| 300–500 ms | Smooth solid interpolation |
| 500–1000 ms | Dashed/translucent reconstructed segment |
| 1000–2000 ms | Dotted neutral segment labelled with the gap |
| >2000 ms | Leave disconnected |

For reconstructed portions:

- Interpolate carrier-relative `x`, `y` and altitude at 100 ms display intervals.
- Use a neutral grey or amber line.
- Do not invent AoA colours across the missing interval.
- Keep measured portions in their normal AoA colours.
- Add a legend such as `Dashed = reconstructed telemetry`.
- Show `Visual reconstruction: 2 gaps, maximum 1080 ms`.

This should produce a complete, readable approach curve for Discord while remaining honest about where the aircraft was actually measured.

Most importantly, the reconstructed samples must never be added to `track.datums` or passed back into gate capture and grading. They should exist only temporarily inside `draw_chart()` and `draw_pattern_chart()`. The current PNG renderer already receives the finished track separately in [record_recovery.rs](./src/tasks/record_recovery.rs:1140), so this separation is practical.

The landing/runway-touch event can remain an authoritative final anchor; the application already consumes its event transform. It can help close the drawing at touchdown, but it cannot replace missing ¾, ½ or ¼-NM measurements.

So my recommendation is:

1. First add presentation-only reconstruction for the Discord PNG.
2. Keep JSON, raw ACMI and grading unchanged.
3. Clearly distinguish measured and reconstructed segments.
4. Later introduce provisional/confidence-based grading as a separate change.

That gives users a useful visual immediately without disguising uncertain telemetry or coupling the workaround to grading. No code has been modified yet.
