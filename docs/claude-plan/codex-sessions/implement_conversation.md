# Plan targeted server fix steps

> based on the conversation and analysis in this folder C:\Users\thierry\Documents\GitHub\sevenfifty777\rust-server\Feature\_inplementation\DCS\_gRPC\_analyse
> i want you to review the Possible targeted server fix steps section
> create plan&#x20;

<details><summary>10 previous messages</summary>

> I’ll review the existing analysis, trace the “Possible targeted server fix steps” back to the relevant server code, then turn it into a focused implementation plan with sequencing, risks, and acceptance criteria. I’m using the graphify skill because this request depends on codebase relationships and prior analysis in the repository.
>
> <details><summary>Ran 5 commands</summary>
>
> - Ran `Get-Content -LiteralPath 'C:\Users\thierry\.agents\skills\graphify\SKILL.md' -Raw; rg -n -i "rust-server|release validation|DCS_gRPC_analyse|targeted server fix" 'C:\Users\thierry\.codex\memories\MEMORY.md'`
> - Ran `$skill = 'C:\Users\thierry\.agents\skills\graphify\SKILL.md'; (Get-Content -LiteralPath $skill).Count; Get-Content -LiteralPath $skill | Select-Object -Skip 250 -First 300; Get-Content -LiteralPath 'C:\Users\thierry\.codex\memories\MEMORY.md' | Select-Object -First 70; Get-ChildItem -LiteralPath 'C:\Users\thierry\Documents\GitHub\sevenfifty777\rust-server\Feature\_inplementation\DCS_gRPC_analyse' -Force | Select-Object Mode,Length,Name; if (Test-Path -LiteralPath 'graphify-out\graph.json') { 'GRAPH_EXISTS' } else { 'NO_GRAPH' }`
> - Ran `$skill = 'C:\Users\thierry\.agents\skills\graphify\SKILL.md'; Get-Content -LiteralPath $skill | Select-Object -First 250; Get-Content -LiteralPath $skill | Select-Object -Skip 550; Get-ChildItem -LiteralPath 'C:\Users\thierry\Documents\GitHub\sevenfifty777\rust-server\Feature' -Force | Select-Object Mode,Length,Name`
> - Ran `Get-Content -LiteralPath 'C:\Users\thierry\.agents\skills\graphify\references\query.md' -Raw; Get-ChildItem -LiteralPath . -Force | Select-Object Mode,Length,Name; rg --files -g '*analyse*' -g '*analysis*' -g '*.md' | Select-String -Pattern 'DCS|analyse|implementation|Feature'`
> - Ran `Get-ChildItem -LiteralPath 'Feature_inplementation' -Force | Select-Object Mode,Length,Name; Get-ChildItem -LiteralPath 'Feature_inplementation\DCS_gRPC_analyse' -Force -ErrorAction SilentlyContinue | Select-Object Mode,Length,Name; rg -n -i "Possible targeted server fix steps|targeted server fix|GetRecoverySnapshot|TelemetryAligner" 'Feature_inplementation'`
>
> </details>
>
> I found the folder—the Markdown escaping in the path hid the actual name, and the repository directory is `Feature_inplementation` (including that spelling). The target section is in `DCS_GRPC_LSO_RECOVERY_ANALYSIS.md`; I’m now checking its proposal against the current protobuf → Rust service → Lua bridge structure before drafting the plan.
>
> <details><summary>Ran 2 commands</summary>
>
> - Ran `if (-not (Test-Path -LiteralPath 'graphify-out\.graphify_python')) { throw 'Missing graphify interpreter marker' }; & (Get-Content -LiteralPath 'graphify-out\.graphify_python' -Raw) -c "import json,re; from pathlib import Path; data=json.loads(Path('graphify-out/graph.json').read_text(encoding='utf-8')); vocab=set(); [(vocab.add(p.lower())) for n in data['nodes'] for c in re.findall(r'[^\W\d_]+', n.get('label','') or '', re.UNICODE) for p in (re.findall(r'[A-Z]+(?=[A-Z][a-z])|[A-Z]?[a-z]+|[A-Z]+', c) or [c]) if 3 <= len(p) <= 30]; Path('graphify-out/.vocab.txt').write_text('\n'.join(sorted(vocab)),encoding='utf-8'); print(f'vocab: {len(vocab)} tokens')"; rg -n -i "recovery|snapshot|transform|lua|proto|service|telemetry|hook|stream|unit|mission|request|response" 'graphify-out\.vocab.txt'`
> - Ran `Get-Command graphify -ErrorAction SilentlyContinue | Select-Object Name,Source; $graph = Get-Content -LiteralPath 'graphify-out\graph.json' -Raw | ConvertFrom-Json -Depth 100; $tokens = [System.Collections.Generic.HashSet[string]]::new([System.StringComparer]::OrdinalIgnoreCase); foreach ($node in $graph.nodes) { foreach ($match in [regex]::Matches([string]$node.label, '[A-Za-z]{3,30}')) { [void]$tokens.Add($match.Value.ToLowerInvariant()) } }; $tokens | Sort-Object | Where-Object { $_ -match 'recovery|snapshot|transform|lua|proto|service|telemetry|hook|stream|unit|mission|request|response' }`
>
> </details>
>
> Query expanded to (from graph vocabulary, 10 tokens): `[gettransformrequest, gettransformresponse, rawtransform, transform, missionservice, lua, hook, streamunitsrequest, request, response]`. This graph appears to describe the current server API, so I’ll use it to validate where the proposed recovery RPC belongs; the plan itself will also be checked directly against source because the new RPC is not yet represented in the graph.
>
> <details><summary>Ran a command</summary>
>
> - Ran `graphify query "gettransformrequest gettransformresponse rawtransform transform missionservice lua hook streamunitsrequest request response" --budget 3000`
>
> </details>
>
> The graph query hit the known Windows `Failed to canonicalize script path` issue. Per the project’s prior troubleshooting notes, I’m switching to direct graph/source inspection and will treat the graph as navigation evidence, not as proof that the proposed API already exists.
>
> <details><summary>Ran 14 commands</summary>
>
> - Ran `git status --short; rg --files -g 'AGENTS.md' -g 'AGENTS.override.md'; $path='Feature_inplementation\DCS_gRPC_analyse\DCS_GRPC_LSO_RECOVERY_ANALYSIS.md'; $lines=Get-Content -LiteralPath $path; for($i=1;$i -le 180;$i++){ '{0,5}: {1}' -f $i,$lines[$i-1] }; for($i=340;$i -le [Math]::Min(460,$lines.Count);$i++){ '{0,5}: {1}' -f $i,$lines[$i-1] }; $conversation='Feature_inplementation\DCS_gRPC_analyse\conervsation.md'; $c=Get-Content -LiteralPath $conversation; for($i=470;$i -le [Math]::Min(550,$c.Count);$i++){ '{0,5}: {1}' -f $i,$c[$i-1] }`
> - Ran `rg -n "rpc GetTransform|message GetTransform|GetTransformRequest|GetTransformResponse|StreamUnits|poll_rate|MissionRpc|mission.*queue|mpsc|request_id|handle_request|method_name|timer.getTime|getUnitTransform|getDrawArgumentValue|draw argument" protos src lua build.rs Cargo.toml; rg -n "rpc GetTransform|message GetTransform|GetTransformRequest|GetTransformResponse|StreamUnits|poll_rate|MissionRpc|mission.*queue|mpsc|timer.getTime|getUnitTransform|getDrawArgumentValue" 'Feature_inplementation\DCS_gRPC_analyse\rust-server-new\protos' 'Feature_inplementation\DCS_gRPC_analyse\rust-server-new\src' 'Feature_inplementation\DCS_gRPC_analyse\rust-server-new\lua'`
> - Ran `$files=@('protos\dcs\unit\v0\unit.proto','src\rpc\unit.rs','lua\DCS-gRPC\methods\unit.lua','src\rpc.rs','src\stream.rs','build.rs','src\server.rs','src\stats.rs'); foreach($file in $files){ if(Test-Path -LiteralPath $file){ "===== $file ====="; $lines=Get-Content -LiteralPath $file; switch($file){ 'protos\dcs\unit\v0\unit.proto' { $ranges=@(@(1,115)) } 'src\rpc\unit.rs' { $ranges=@(@(1,95)) } 'lua\DCS-gRPC\methods\unit.lua' { $ranges=@(@(1,105)) } 'src\rpc.rs' { $ranges=@(@(1,190)) } 'src\stream.rs' { $ranges=@(@(1,120),@(280,360)) } 'build.rs' { $ranges=@(@(1,220)) } 'src\server.rs' { $ranges=@(@(190,290)) } default { $ranges=@(@(1,[Math]::Min(220,$lines.Count))) } }; foreach($range in $ranges){ for($i=$range[0];$i -le [Math]::Min($range[1],$lines.Count);$i++){ '{0,5}: {1}' -f $i,$lines[$i-1] } } } }; git rev-parse HEAD; rg -n "version\s*=|dcs-grpc-stubs|tonic-build|prost-build" Cargo.toml Cargo.lock build.rs; Get-ChildItem -LiteralPath stubs -Recurse -File | Select-Object -First 30 FullName`
> - Ran `Get-Content -LiteralPath 'protos\dcs\unit\v0\unit.proto' | Select-Object -First 120; Get-Content -LiteralPath 'src\rpc\unit.rs' | Select-Object -First 100; Get-Content -LiteralPath 'lua\DCS-gRPC\methods\unit.lua' | Select-Object -First 110; Get-Content -LiteralPath 'src\rpc.rs' | Select-Object -First 240; Get-Content -LiteralPath 'stubs\build.rs' | Select-Object -First 240; Get-Content -LiteralPath 'stubs\src\lib.rs' | Select-Object -First 200`
> - Ran `Get-Content -LiteralPath 'stubs\src\unit.rs' | Select-Object -First 180; Get-Content -LiteralPath 'protos\dcs\dcs.proto' | Select-Object -First 180; Get-Content -LiteralPath 'src\server.rs' | Select-Object -Skip 200 -First 130; Get-Content -LiteralPath 'src\stats.rs' | Select-Object -First 260; rg -n "struct IPC|pub async fn request|request\(" "$env:USERPROFILE\.cargo\registry\src" -g '*.rs' | Select-String 'dcs-module-ipc|dcs_module_ipc' | Select-Object -First 40`
> - Ran `$ipc='C:\Users\thierry\.cargo\registry\src\index.crates.io-1949cf8c6b5b557f\dcs-module-ipc-0.9.1\src\ipc.rs'; Get-Content -LiteralPath $ipc | Select-Object -First 260; rg -n "track_block_time|track_call|request\(|receive|respond|poll|queue" src lua 'C:\Users\thierry\.cargo\registry\src\index.crates.io-1949cf8c6b5b557f\dcs-module-ipc-0.9.1\src'`
> - Ran `Get-Content -LiteralPath 'src\lib.rs' | Select-Object -Skip 90 -First 150; Get-Content -LiteralPath 'lua\DCS-gRPC\grpc.lua' | Select-Object -Skip 170 -First 85; Get-Content -LiteralPath 'Cargo.toml' | Select-Object -First 90; Get-Content -LiteralPath 'stubs\Cargo.toml' | Select-Object -First 120; rg -n "dcs-module-ipc" Cargo.lock Cargo.toml; rg -n "hook.*argument|HOOK|argument.*25|get_draw_argument|GetDrawArgument" 'Feature_inplementation\DCS_gRPC_analyse\DCS-gRPC-lso-new\src' 'Feature_inplementation\DCS_gRPC_analyse\DCS-gRPC-lso-new\*.toml'`
> - Ran `Get-Content -LiteralPath 'src\stream.rs' | Select-Object -Skip 185 -First 90; Get-Content -LiteralPath 'Feature_inplementation\DCS_gRPC_analyse\DCS-gRPC-lso-new\src\tasks\record_recovery.rs' | Select-Object -First 470; Get-Content -LiteralPath 'Feature_inplementation\DCS_gRPC_analyse\DCS-gRPC-lso-new\src\client\unit_client.rs' | Select-Object -First 160; Get-Content -LiteralPath 'Feature_inplementation\DCS_gRPC_analyse\DCS-gRPC-lso-new\src\commands\run.rs' | Select-Object -First 100`
> - Ran `rg -n "message RawTransform|message Transform|struct RawTransform|impl From<RawTransform>|raw_transform" protos stubs src; Get-Content -LiteralPath 'protos\dcs\common\v0\common.proto' | Select-Object -Skip 190 -First 130; Get-Content -LiteralPath 'stubs\src\common.rs' | Select-Object -First 240; Get-Content -LiteralPath 'Feature_inplementation\DCS_gRPC_analyse\DCS-gRPC-lso-new\src\utils\interval.rs' | Select-Object -First 120; rg -n "RPC_DEADLINE|request_with_deadline|request_with_timeout" 'Feature_inplementation\DCS_gRPC_analyse\DCS-gRPC-lso-new\src\client'`
> - Ran `rg -n "protolint|protoc|api.html|cargo test|cargo clippy|luacheck|stubs|release|proto" README.md CONTRIBUTING.md Makefile .github docs; Get-Content -LiteralPath 'Makefile' -Raw; Get-Content -LiteralPath '.protolint.yaml' -Raw; Get-ChildItem -LiteralPath 'Feature_inplementation\DCS_gRPC_analyse\DCS-gRPC-lso-new\protos' -Recurse -File | Select-Object -First 20 FullName; rg -n "package dcs.unit|rpc GetTransform|dcs-grpc-stubs|stubs =" 'Feature_inplementation\DCS_gRPC_analyse\DCS-gRPC-lso-new\protos' 'Feature_inplementation\DCS_gRPC_analyse\DCS-gRPC-lso-new\Cargo.toml' 'Feature_inplementation\DCS_gRPC_analyse\DCS-gRPC-lso-new\Cargo.lock'`
> - Ran `Get-Content -LiteralPath 'Feature_inplementation\DCS_gRPC_analyse\DCS-gRPC-lso-new\Cargo.toml' | Select-Object -First 90; Get-Content -LiteralPath 'Feature_inplementation\DCS_gRPC_analyse\DCS-gRPC-lso-new\src\client\mod.rs' | Select-Object -First 70; rg -n -C 5 "name = \"dcs-grpc-stubs\"|source = \"git\+" 'Feature_inplementation\DCS_gRPC_analyse\DCS-gRPC-lso-new\Cargo.lock'; rg -n "GET_RECOVERY|RecoverySnapshot|atomic snapshot|feature flag|legacy_inline_hook_sampling" 'Feature_inplementation\DCS_gRPC_analyse\DCS-gRPC-lso-new' -g '*.md' -g '*.rs' -g '*.toml'`
> - Ran `Get-Item -LiteralPath 'Feature_inplementation\DCS_gRPC_analyse\DCS-gRPC-lso-new\Cargo.toml' | Select-Object FullName,Length; Get-Content -LiteralPath 'Feature_inplementation\DCS_gRPC_analyse\DCS-gRPC-lso-new\Cargo.toml' -Raw`
> - Ran `rg --files | rg -i "test|fixture|bench"; rg -n "#\[cfg\(test\)\]|#\[test\]|tokio::test" src stubs lua; Get-Content -LiteralPath '.github\workflows\ci.yml' | Select-Object -First 130; Get-Content -LiteralPath 'CONTRIBUTING.md' | Select-Object -Skip 70 -First 60`
> - Ran `Get-Content -LiteralPath 'build.rs' -Raw; rg -n "methods\\|methods/|WalkDir|include_str|lua_files" build.rs src lua; rg -n "hook_sampling|telemetry_mode|RecoveryReport|schema_version" 'Feature_inplementation\DCS_gRPC_analyse\DCS-gRPC-lso-new\src'`
>
> </details>
>
> The review found several important adjustments before implementation: the current IPC queue is unbounded FIFO, exact queue-wait timing is not available through the existing API, cancelled/deadline-expired requests can remain queued, and `StreamUnits` fan-out is the actual burst source—not the public `GetTransform` method by itself. I’m incorporating those constraints so the plan does not promise instrumentation or fairness that the present architecture cannot deliver.
>
> <details><summary>Created a file, edited a file, ran 3 commands</summary>
>
> - Wrote `./Feature_inplementation/DCS_gRPC_analyse/DCS_GRPC_LSO_RECOVERY_ANALYSIS.md` (+2 -0)
> - Wrote `./Feature_inplementation/DCS_gRPC_analyse/GET_RECOVERY_SNAPSHOT_IMPLEMENTATION_PLAN.md` (+387 -0)
> - Ran ````git status --short; git diff --check; git diff --stat; rg -n "Reviewed implementation plan|## Review of the proposed|Work package|Definition of done|deadline below|dcs-module-ipc|StreamUnits" 'Feature_inplementation\DCS_gRPC_analyse\DCS_GRPC_LSO_RECOVERY_ANALYSIS.md' 'Feature_inplementation\DCS_gRPC_analyse\GET_RECOVERY_SNAPSHOT_IMPLEMENTATION_PLAN.md'; $plan='Feature_inplementation\DCS_gRPC_analyse\GET_RECOVERY_SNAPSHOT_IMPLEMENTATION_PLAN.md'; $text=Get-Content -LiteralPath $plan -Raw; [pscustomobject]@{Lines=(Get-Content -LiteralPath $plan).Count; CodeFences=([regex]::Matches($text,'```')).Count; Tables=([regex]::Matches($text,'\| ---')).Count; Bytes=(Get-Item -LiteralPath $plan).Length} | Format-List; Get-Content -LiteralPath $plan | Select-Object -First 80````
> - Ran `git check-ignore -v 'Feature_inplementation/DCS_gRPC_analyse/GET_RECOVERY_SNAPSHOT_IMPLEMENTATION_PLAN.md' 'Feature_inplementation/DCS_gRPC_analyse/DCS_GRPC_LSO_RECOVERY_ANALYSIS.md'; git ls-files 'Feature_inplementation/DCS_gRPC_analyse/*'; git diff -- 'Feature_inplementation/DCS_gRPC_analyse/DCS_GRPC_LSO_RECOVERY_ANALYSIS.md'; Get-Content -LiteralPath '.gitignore' -Raw`
> - Wrote `./Feature_inplementation/DCS_gRPC_analyse/DCS_GRPC_LSO_RECOVERY_ANALYSIS.md` (+8 -8)
> - Ran ````$analysis='Feature_inplementation\DCS_gRPC_analyse\DCS_GRPC_LSO_RECOVERY_ANALYSIS.md'; $plan='Feature_inplementation\DCS_gRPC_analyse\GET_RECOVERY_SNAPSHOT_IMPLEMENTATION_PLAN.md'; rg -n "Reviewed implementation plan|Add a dedicated|First extend the mission IPC|Poll the atomic|core endpoint" $analysis; rg -n "^## (Goal|Review|Decisions|Target contract|Dependency|Validation commands|Risks|Definition)|^### Work package" $plan; $text=Get-Content -LiteralPath $plan -Raw; if(([regex]::Matches($text,'```')).Count % 2 -ne 0){ throw 'Unbalanced code fences' }; $required=@('protos\dcs\dcs.proto','stubs\build.rs','stubs\src\lib.rs','src\rpc.rs','src\server.rs','lua\DCS-gRPC\grpc.lua','Feature_inplementation\DCS_gRPC_analyse\DCS-gRPC-lso-new\Cargo.toml','Feature_inplementation\DCS_gRPC_analyse\DCS-gRPC-lso-new\src\tasks\record_recovery.rs'); foreach($item in $required){ [pscustomobject]@{Exists=(Test-Path -LiteralPath $item);Path=$item} }; $memory='C:\Users\thierry\.codex\memories\MEMORY.md'; $m=Get-Content -LiteralPath $memory; for($i=1;$i -le 30;$i++){ '{0,4}: {1}' -f $i,$m[$i-1] }````
>
> </details>
</details>

Created the reviewed implementation plan:

[GET_RECOVERY_SNAPSHOT_IMPLEMENTATION_PLAN.md](</C:/Users/thierry/Documents/GitHub/sevenfifty777/rust-server/Feature_inplementation/DCS_gRPC_analyse/GET_RECOVERY_SNAPSHOT_IMPLEMENTATION_PLAN.md:1>)

I also revised the original [“Possible targeted server fix” section](</C:/Users/thierry/Documents/GitHub/sevenfifty777/rust-server/Feature_inplementation/DCS_gRPC_analyse/DCS_GRPC_LSO_RECOVERY_ANALYSIS.md:21>) to reflect the review.

Key corrections include:

- A dedicated additive `RecoveryService`.
- Optional draw-argument number instead of a CATOBAR boolean.
- Explicit hook unavailable/not-requested states.
- IPC observability and cancellation as a prerequisite.
- Prevention of timed-out requests executing later.
- Bounded `StreamUnits` fan-out before considering priority.
- A deadline below the 300 ms stale boundary, not necessarily below the 100 ms sampling period.
- Separate correctness gates from live performance targets.
- Eight focused work packages with files, acceptance criteria, commit boundaries, A/B validation, rollout, and rollback.

Markdown structure, links, code fences, and referenced source paths were checked. No Rust or Lua tests were run because this was planning-only work.

Note: `Feature_inplementation` is ignored by the repository’s `.gitignore`, so these documents exist locally but will not appear in normal `git status` or be committed without force-adding them.

> can you implement the change following the plan

<details><summary>24 previous messages</summary>

> I’ll implement the staged, code-complete portion of the plan: IPC safety/observability, the additive server RPC and Lua handler, the LSO opt-in client path, tests, and documentation. I’ll keep grading/geometry changes separate and will report live-DCS A/B validation as pending because it cannot be proven by static checks. I’m using the graphify, DCS scripting, and DCS-gRPC client skills for the architecture trace, Lua bridge, and typed Rust client respectively.
>
> <details><summary>Ran 4 commands</summary>
>
> - Ran `$paths=@('C:\Users\thierry\.agents\skills\graphify.old\SKILL.md','C:\Users\thierry\.agents\skills\dcs_scripting\SKILL.md','C:\Users\thierry\Documents\GitHub\sevenfifty777\rust-server\.agents\skills\dcs-grpc-client-development\SKILL.md'); foreach($path in $paths){ "===== $path ====="; (Get-Content -LiteralPath $path).Count; Get-Content -LiteralPath $path | Select-Object -First 240 }; rg -n -i "GetRecoverySnapshot|atomic snapshot|dcs-module-ipc|hook buffer|rust-server.*0.9.1" 'C:\Users\thierry\.codex\memories\MEMORY.md'`
> - Ran `$skill='C:\Users\thierry\.agents\skills\graphify.old\SKILL.md'; Get-Content -LiteralPath $skill | Select-Object -Skip 240 -First 240; Get-ChildItem -LiteralPath 'C:\Users\thierry\.agents\skills\dcs_scripting\references' -File | Select-Object Name,Length; rg -n -i "timer.getTime|getDrawArgumentValue|getPosition|getVelocity|Unit.getByName|Object.getPosition" 'C:\Users\thierry\.agents\skills\dcs_scripting\references'`
> - Ran `$skill='C:\Users\thierry\.agents\skills\graphify.old\SKILL.md'; Get-Content -LiteralPath $skill | Select-Object -Skip 480; $refs='C:\Users\thierry\.agents\skills\dcs_scripting\references'; foreach($name in @('func_getByName.md','func_getDrawArgumentValue.md','func_getPosition.md','func_getVelocity.md','func_getTime.md','class_Unit.md')){ "===== $name ====="; Get-Content -LiteralPath (Join-Path $refs $name) -Raw }`
> - Ran `Get-Content -LiteralPath 'C:\Users\thierry\.agents\skills\graphify.old\SKILL.md' | Select-Object -Skip 240 -First 240`
>
> </details>
>
> Before touching dependencies, I need to verify the exact `dcs-module-ipc 0.9.1` source and advisory status because the plan requires extending that crate’s queue metadata and cancellation behavior. I’ll vendor the already locked version only if its license/source match this workspace; no floating upgrade.
>
> <details><summary>Ran a command, searched the web</summary>
>
> Searched the web for `site:crates.io/crates/dcs-module-ipc 0.9.1 ...`
>
> - Ran `git status --short; git submodule status; Get-Content -LiteralPath 'C:\Users\thierry\.cargo\registry\src\index.crates.io-1949cf8c6b5b557f\dcs-module-ipc-0.9.1\Cargo.toml' -Raw; Get-Content -LiteralPath 'C:\Users\thierry\.cargo\registry\src\index.crates.io-1949cf8c6b5b557f\dcs-module-ipc-0.9.1\Cargo.toml.orig' -Raw; $graph = Get-Content -LiteralPath 'graphify-out\graph.json' -Raw | ConvertFrom-Json -Depth 100; $tokens=[System.Collections.Generic.HashSet[string]]::new([System.StringComparer]::OrdinalIgnoreCase); foreach($node in $graph.nodes){ foreach($match in [regex]::Matches([string]$node.label,'[A-Za-z]{3,40}')){[void]$tokens.Add($match.Value.ToLowerInvariant())}}; $tokens | Sort-Object | Where-Object { $_ -match 'recovery|snapshot|transform|streamunits|unitservice|missionrpc|pendingrequest|stats|lua|stubs|proto|request' }`
>
> </details>
>
> Query expanded to (from the existing graph vocabulary, 10 tokens): `[missionrpc, pendingrequest, request, stats, streamunitsrequest, unitservice, gettransformrequest, rawtransform, stubs, lua]`. I’ll use this to confirm the current dependency path, then inspect the source directly for implementation details the stale graph cannot contain.
>
> <details><summary>Ran a command</summary>
>
> - Ran `graphify query "missionrpc pendingrequest request stats streamunitsrequest unitservice gettransformrequest rawtransform stubs lua" --budget 3000`
>
> </details>
>
> The graph query hit the same known Windows canonicalization failure, so I’m following the skill’s direct-source fallback. The DCS API references confirm the Lua calls and an important semantic detail: an invalid draw-argument index returns numeric `0`, so `0` must remain a valid observed value rather than being treated as “unavailable.”
>
> <details><summary>Edited a file, ran 3 commands</summary>
>
> - Ran `$lso='Feature_inplementation\DCS_gRPC_analyse\DCS-gRPC-lso-new'; Get-Content -LiteralPath "$lso\src\track.rs" | Select-Object -First 180; Get-Content -LiteralPath "$lso\src\track.rs" | Select-Object -Skip 1240 -First 150; rg -n -C 5 "MAX_HOOK_EVIDENCE|BufferLimit|buffer_limit|dropped_samples|observe_hook_sample|HookObservation" "$lso\src"; Get-Content -LiteralPath "$lso\src\track.rs" | Select-Object -Skip 1900 -First 250; Get-Content -LiteralPath "$lso\src\tasks\mod.rs" | Select-Object -First 120; Get-Content -LiteralPath "$lso\src\commands\run.rs" | Select-Object -Skip 270 -First 85; Get-Content -LiteralPath "$lso\src\commands\run.rs" | Select-Object -Skip 810 -First 80`
> - Ran `$file='Feature_inplementation\DCS_gRPC_analyse\DCS-gRPC-lso-new\src\track.rs'; Get-Content -LiteralPath $file | Select-Object -Skip 180 -First 230; rg -n -C 8 "Completeness::BufferLimit|dropped_samples|grading_availability" $file 'Feature_inplementation\DCS_gRPC_analyse\DCS-gRPC-lso-new\src\grading.rs'; rg -n "hook.*512|MAX_HOOK|buffer.*hook|observe_hook_sample" $file`
> - Wrote `./Feature_inplementation/DCS_gRPC_analyse/DCS-gRPC-lso-new/src/track.rs` (+68 -14)
> - Ran `cargo test hook_timeline_compaction_preserves_recent_final_evidence_without_buffer_limit`
>
> </details>
>
> The first hook-buffer test exposed a workspace-layout issue in the copied LSO checkout: because it is nested under `rust-server`, Cargo treats it as an undeclared workspace member. I’ll add a local empty `[workspace]` marker to that copied checkout so its tests run independently; this affects only the ignored analysis copy, not the server workspace.
>
> <details><summary>Edited a file, ran 5 commands</summary>
>
> - Ran `$crate='C:\Users\thierry\.cargo\registry\src\index.crates.io-1949cf8c6b5b557f\dcs-module-ipc-0.9.1'; Get-ChildItem -LiteralPath $crate -Recurse -File | Select-Object FullName,Length; Get-Content -LiteralPath "$crate\src\lib.rs" -Raw; Get-Content -LiteralPath "$crate\src\ipc.rs" -Raw; rg -n -C 4 "name = \"dcs-module-ipc\"" Cargo.lock; cargo audit --version`
> - Ran `rg -n "IPC::default|IPC::<|ipc_mission|ServerState|pub struct Server" src; Get-Content -LiteralPath 'src\server.rs' | Select-Object -First 230; Get-Content -LiteralPath 'Cargo.lock' | Select-Object -Skip 490 -First 35; Get-Content -LiteralPath 'C:\Users\thierry\.cargo\registry\src\index.crates.io-1949cf8c6b5b557f\dcs-module-ipc-0.9.1\.cargo_vcs_info.json' -Raw; git check-ignore -v 'Feature_inplementation/DCS_gRPC_analyse/DCS-gRPC-lso-new/Cargo.toml'`
> - Ran `Get-Content -LiteralPath '.cargo\config.toml' -Raw; Get-Content -LiteralPath 'C:\Users\thierry\.cargo\registry\src\index.crates.io-1949cf8c6b5b557f\dcs-module-ipc-0.9.1\.cargo\config.toml' -Raw; rg -n "LUA_LIB|LUA_LINK|LUA_INC" .cargo Cargo.toml .github; Get-ChildItem -LiteralPath '.cargo' -Force | Select-Object Name,Length`
> - Ran `rg -n "errorInvalid|errorNotFound|errorInternal|GRPC.error" lua\DCS-gRPC; rg -n "enum .*Status|STATUS_UNSPECIFIED|optional uint32|optional double" protos\dcs -g '*.proto' | Select-Object -First 100; Get-Content -LiteralPath 'lua\DCS-gRPC\grpc.lua' | Select-Object -First 180; Get-Content -LiteralPath 'lua\DCS-gRPC\exporters\unit.lua' -Raw -ErrorAction SilentlyContinue; rg -n "rawTransform" lua\DCS-gRPC\exporters* lua\DCS-gRPC -g '*.lua'`
> - Wrote `./Feature_inplementation/DCS_gRPC_analyse/DCS-gRPC-lso-new/Cargo.toml` (+3 -0)
> - Ran `cargo fmt -- --check; cargo test hook_timeline_compaction_preserves_recent_final_evidence_without_buffer_limit`
>
> </details>
>
> The hook-buffer prerequisite is now implemented and its focused test passes. The skill-directed DCS API check also confirmed that draw-argument `0` is a valid observation. I’m moving to the queue layer now: the vendored code will be the exact locked 0.9.1 source plus focused cancellation, capacity, request-ID, and timing additions, with its original MIT/Apache licensing metadata preserved.
>
> <details><summary>Created 8 files, edited 8 files, ran 5 commands</summary>
>
> - Wrote `./Cargo.toml` (+3 -3)
> - Wrote `./ipc/Cargo.toml` (+23 -0)
> - Wrote `./ipc/README.md` (+11 -0)
> - Wrote `./ipc/src/ipc.rs` (+365 -0)
> - Wrote `./ipc/src/lib.rs` (+3 -0)
> - Ran `cargo fmt --all -- --check; cargo check --locked -p dcs-module-ipc`
> - Wrote `./ipc/src/ipc.rs` (+4 -2)
> - Ran `git status --short; git diff -- Cargo.lock Cargo.toml; cargo test --locked -p dcs-module-ipc`
> - Wrote `./ipc/src/ipc.rs` (+18 -19)
> - Ran `cargo fmt --all; cargo test --locked -p dcs-module-ipc`
> - Ran `Get-ChildItem -LiteralPath 'src\lua5.1' -Force | Select-Object Name,Length; Get-ChildItem -LiteralPath 'C:\Users\thierry\.cargo\registry\src\index.crates.io-1949cf8c6b5b557f\dcs-module-ipc-0.9.1\lua5.1' -Force | Select-Object Name,Length; Get-ChildItem -LiteralPath 'target\debug\deps' -Filter 'lua*.dll' | Select-Object FullName,Length`
> - Ran `rg --files -g '*.dll' | Select-Object -First 50; Get-ChildItem -Path 'C:\Program Files\Eagle Dynamics' -Filter 'lua*.dll' -Recurse -ErrorAction SilentlyContinue | Select-Object -First 10 FullName,Length`
> - Wrote `./lua/DCS-gRPC/grpc.lua` (+3 -2)
> - Wrote `./lua/DCS-gRPC/methods/recovery.lua` (+51 -0)
> - Wrote `./protos/dcs/dcs.proto` (+3 -2)
> - Wrote `./protos/dcs/recovery/v0/recovery.proto` (+56 -0)
> - Wrote `./src/rpc.rs` (+3 -2)
> - Wrote `./src/rpc/recovery.rs` (+16 -0)
> - Wrote `./src/server.rs` (+6 -4)
> - Wrote `./stubs/build.rs` (+9 -5)
> - Wrote `./stubs/src/lib.rs` (+3 -2)
> - Wrote `./stubs/src/recovery.rs` (+105 -0)
>
> </details>
>
> Context compaction
>
> Source: automatic
> Status: completed
>
> The additive snapshot API is now laid out. I’m compiling that boundary first, then I’ll wire queue diagnostics and the LSO opt-in/fallback path so each stage stays independently reviewable.
>
> <details><summary>Edited a file, ran 5 commands</summary>
>
> - Ran `git status --short; rg -n "GetRecoverySnapshot|DrawArgumentStatus|dcs-module-ipc|with_queue_capacity|HookObservation" Cargo.toml Cargo.lock protos stubs/src src ipc Feature_inplementation/DCS_gRPC_analyse/DCS-gRPC-lso-new/src Feature_inplementation/DCS_gRPC_analyse/DCS-gRPC-lso-new/Cargo.toml`
> - Ran `Get-Content -LiteralPath 'stubs/src/recovery.rs'; Get-Content -LiteralPath 'protos/dcs/recovery/v0/recovery.proto'; Get-Content -LiteralPath 'src/rpc/recovery.rs'; Get-Content -LiteralPath 'lua/DCS-gRPC/methods/recovery.lua'; Get-Content -LiteralPath 'ipc/src/ipc.rs' | Select-Object -First 230`
> - Wrote `./protos/dcs/recovery/v0/recovery.proto` (+2 -1)
> - Ran `cargo fmt --all -- --check; cargo check --locked -p dcs-grpc-stubs; cargo test --locked -p dcs-grpc-stubs recovery`
> - Ran `Get-Content -LiteralPath 'src/stats.rs'; Get-Content -LiteralPath 'src/lib.rs'; Get-Content -LiteralPath 'src/rpc.rs'; Get-Content -LiteralPath 'src/server.rs' | Select-Object -First 190; Get-Content -LiteralPath 'ipc/src/ipc.rs' | Select-Object -Skip 225 -First 180`
> - Ran `rg -n "\.request\(|notification\(" src ipc -g '*.rs'; rg -n "ipc\.request|self\.ipc" src -g '*.rs'`
>
> </details>
>
> The protobuf boundary compiles and its conversion tests pass, including the important “observed value = 0” case. I’m now adding the measurements that make the fix diagnosable: bounded queue pressure, discarded-cancellation counts, queue wait, Lua execution time, and RPC total time.
>
> <details><summary>Created a file, edited 12 files, ran 10 commands</summary>
>
> - Wrote `./src/lib.rs` (+46 -22)
> - Wrote `./src/rpc.rs` (+40 -19)
> - Wrote `./src/server.rs` (+7 -4)
> - Wrote `./src/stats.rs` (+108 -8)
> - Wrote `./src/lib.rs` (+10 -5)
> - Wrote `./src/rpc.rs` (+2 -2)
> - Ran `cargo fmt --all; cargo check --locked --workspace --all-targets`
> - Wrote `./src/lib.rs` (+3 -3)
> - Ran `Get-Content -LiteralPath 'Feature_inplementation/DCS_gRPC_analyse/GET_RECOVERY_SNAPSHOT_IMPLEMENTATION_PLAN.md'; Get-Content -LiteralPath 'Feature_inplementation/DCS_gRPC_analyse/DCS-gRPC-lso-new/src/client/mod.rs'; Get-Content -LiteralPath 'Feature_inplementation/DCS_gRPC_analyse/DCS-gRPC-lso-new/src/tasks/mod.rs'; Get-Content -LiteralPath 'Feature_inplementation/DCS_gRPC_analyse/DCS-gRPC-lso-new/src/commands.rs' | Select-Object -First 260; Get-Content -LiteralPath 'Feature_inplementation/DCS_gRPC_analyse/DCS-gRPC-lso-new/src/telemetry.rs'`
> - Ran `rg --files 'Feature_inplementation/DCS_gRPC_analyse/DCS-gRPC-lso-new/src' | rg 'commands|run.rs|record_recovery.rs|interval.rs|transform.rs|metrics.rs'; rg -n "TaskParams \{|hook_sampling|legacy-inline|schema_version|TelemetrySample|next_sample|try_join|interval\(" 'Feature_inplementation/DCS_gRPC_analyse/DCS-gRPC-lso-new/src'`
> - Ran `$p='Feature_inplementation/DCS_gRPC_analyse/DCS-gRPC-lso-new/src/tasks/record_recovery.rs'; Get-Content -LiteralPath $p | Select-Object -First 220; Get-Content -LiteralPath $p | Select-Object -Skip 220 -First 300; Get-Content -LiteralPath $p | Select-Object -Skip 820 -First 100; $q='Feature_inplementation/DCS_gRPC_analyse/DCS-gRPC-lso-new/src/commands/run.rs'; Get-Content -LiteralPath $q | Select-Object -First 100; Get-Content -LiteralPath $q | Select-Object -Skip 270 -First 90; Get-Content -LiteralPath 'Feature_inplementation/DCS_gRPC_analyse/DCS-gRPC-lso-new/src/utils/interval.rs'; Get-Content -LiteralPath 'Feature_inplementation/DCS_gRPC_analyse/DCS-gRPC-lso-new/src/transform.rs' | Select-Object -First 180; Get-Content -LiteralPath 'Feature_inplementation/DCS_gRPC_analyse/DCS-gRPC-lso-new/Cargo.toml' | Select-Object -First 100`
> - Ran `Get-Content -LiteralPath 'Feature_inplementation/DCS_gRPC_analyse/DCS-gRPC-lso-new/src/client/unit_client.rs'; Get-Content -LiteralPath 'Feature_inplementation/DCS_gRPC_analyse/DCS-gRPC-lso-new/src/metrics.rs' | Select-Object -First 240; Get-Content -LiteralPath 'Feature_inplementation/DCS_gRPC_analyse/DCS-gRPC-lso-new/src/track.rs' | Select-Object -Skip 420 -First 150; Get-Content -LiteralPath 'Feature_inplementation/DCS_gRPC_analyse/DCS-gRPC-lso-new/src/tasks/detect_recovery_attempt.rs' | Select-Object -First 180`
> - Ran `rg -n "pub struct Datum|Datum \{" 'Feature_inplementation/DCS_gRPC_analyse/DCS-gRPC-lso-new/src/track.rs'; Get-Content -LiteralPath 'Feature_inplementation/DCS_gRPC_analyse/DCS-gRPC-lso-new/src/track.rs' | Select-Object -First 130; Get-Content -LiteralPath 'Feature_inplementation/DCS_gRPC_analyse/DCS-gRPC-lso-new/src/track.rs' | Select-Object -Skip 620 -First 90; Get-Content -LiteralPath 'Feature_inplementation/DCS_gRPC_analyse/DCS-gRPC-lso-new/src/metrics.rs' | Select-Object -Skip 200 -First 180`
> - Ran `$p='Feature_inplementation/DCS_gRPC_analyse/DCS-gRPC-lso-new/src/track.rs'; Get-Content -LiteralPath $p | Select-Object -Skip 885 -First 75; Get-Content -LiteralPath $p | Select-Object -Skip 1000 -First 70; Get-Content -LiteralPath 'Feature_inplementation/DCS_gRPC_analyse/DCS-gRPC-lso-new/src/tasks/record_recovery.rs' | Select-Object -Skip 860 -First 100; Get-Content -LiteralPath 'Feature_inplementation/DCS_gRPC_analyse/DCS-gRPC-lso-new/src/commands/run.rs' | Select-Object -Skip 820 -First 60`
> - Wrote `./Feature_inplementation/DCS_gRPC_analyse/DCS-gRPC-lso-new/Cargo.toml` (+1 -2)
> - Wrote `./Feature_inplementation/DCS_gRPC_analyse/DCS-gRPC-lso-new/src/client/mod.rs` (+2 -0)
> - Wrote `./Feature_inplementation/DCS_gRPC_analyse/DCS-gRPC-lso-new/src/client/recovery_client.rs` (+109 -0)
> - Wrote `./Feature_inplementation/DCS_gRPC_analyse/DCS-gRPC-lso-new/src/metrics.rs` (+6 -2)
> - Wrote `./Feature_inplementation/DCS_gRPC_analyse/DCS-gRPC-lso-new/src/tasks/mod.rs` (+25 -0)
> - Wrote `./Feature_inplementation/DCS_gRPC_analyse/DCS-gRPC-lso-new/src/utils/interval.rs` (+7 -0)
> - Wrote `./Feature_inplementation/DCS_gRPC_analyse/DCS-gRPC-lso-new/src/telemetry.rs` (+3 -0)
> - Wrote `./Feature_inplementation/DCS_gRPC_analyse/DCS-gRPC-lso-new/src/track.rs` (+4 -0)
> - Ran `rg -n "TelemetrySample \{" 'Feature_inplementation/DCS_gRPC_analyse/DCS-gRPC-lso-new/src'`
> - Wrote `./Feature_inplementation/DCS_gRPC_analyse/DCS-gRPC-lso-new/src/commands/run.rs` (+21 -1)
> - Ran `rg -n "TaskParams \{" 'Feature_inplementation/DCS_gRPC_analyse/DCS-gRPC-lso-new/src'`
> - Ran `Get-Content -LiteralPath 'Feature_inplementation/DCS_gRPC_analyse/DCS-gRPC-lso-new/src/error.rs'; rg -n "impl From<.*Status|tonic::Status" 'Feature_inplementation/DCS_gRPC_analyse/DCS-gRPC-lso-new/src/error.rs'`
>
> </details>
>
> The LSO side now has the typed client, CLI controls (`auto`, `legacy`, `atomic`), a bounded snapshot deadline, and a skip-based scheduler. I’m wiring mode selection once per recovery; `auto` will fall back only on `UNIMPLEMENTED`, so transient failures cannot silently mix acquisition paths.
>
> <details><summary>Edited 6 files, ran 8 commands</summary>
>
> - Wrote `./Feature_inplementation/DCS_gRPC_analyse/DCS-gRPC-lso-new/src/tasks/record_recovery.rs` (+111 -19)
> - Ran `rg -n -C 2 "hook_sampling\.mode == super::HookSamplingMode::Independent" 'Feature_inplementation/DCS_gRPC_analyse/DCS-gRPC-lso-new/src/tasks/record_recovery.rs'; rg -n "RecoveryReport \{" 'Feature_inplementation/DCS_gRPC_analyse/DCS-gRPC-lso-new/src/tasks/record_recovery.rs'`
> - Wrote `./Feature_inplementation/DCS_gRPC_analyse/DCS-gRPC-lso-new/src/tasks/record_recovery.rs` (+6 -2)
> - Wrote `./Feature_inplementation/DCS_gRPC_analyse/DCS-gRPC-lso-new/src/tasks/record_recovery.rs` (+1 -1)
> - Ran `cargo fmt --all; cargo check --locked --workspace --all-targets`
> - Ran `cargo fmt; cargo check --all-targets`
> - Ran `rg -n "methods/|grpc.lua|integrity" build.rs src/integrity.rs lua -g '*.rs' -g '*.lua'; Get-Content -LiteralPath 'src/integrity.rs'; rg -n "RecoveryServiceServer|add_service" src/server.rs | Select-Object -First 80; rg -n "recovery.lua" lua/DCS-gRPC/grpc.lua`
> - Ran `Get-Content -LiteralPath 'build.rs' | Select-Object -First 90; Get-Content -LiteralPath 'Feature_inplementation/DCS_gRPC_analyse/DCS-gRPC-lso-new/src/client/recovery_client.rs'`
> - Wrote `./Feature_inplementation/DCS_gRPC_analyse/DCS-gRPC-lso-new/src/client/recovery_client.rs` (+23 -8)
> - Wrote `./Feature_inplementation/DCS_gRPC_analyse/DCS-gRPC-lso-new/src/tasks/record_recovery.rs` (+0 -1)
> - Wrote `./stubs/src/recovery.rs` (+24 -0)
> - Wrote `./ipc/src/ipc.rs` (+25 -0)
> - Ran `cargo fmt; cargo check --locked --all-targets; cargo test --locked no_acmi_and_hook_ab_configuration_are_accepted hook_timeline_compaction_preserves_recent_final_evidence_without_buffer_limit`
> - Ran `cargo test --locked no_acmi_and_hook_ab_configuration_are_accepted; cargo test --locked hook_timeline_compaction_preserves_recent_final_evidence_without_buffer_limit`
> - Wrote `./ipc/src/ipc.rs` (+16 -1)
> - Wrote `./src/lib.rs` (+3 -0)
> - Wrote `./src/stats.rs` (+3 -1)
> - Ran `Get-Command luacheck -ErrorAction SilentlyContinue | Select-Object -ExpandProperty Source; Get-Command lua -ErrorAction SilentlyContinue | Select-Object -ExpandProperty Source; Get-Command protolint -ErrorAction SilentlyContinue | Select-Object -ExpandProperty Source; git diff --check; cargo fmt --all -- --check; cargo check --locked --workspace --all-targets; cargo test --locked -p dcs-grpc-stubs recovery; cargo test --locked -p dcs-module-ipc --no-run`
>
> </details>
>
> Static server validation is green, and all four recovery-stub tests pass. The IPC tests compile, but this Windows checkout cannot execute that test binary without the DCS Lua 5.1 runtime DLL; I’m preserving the tests and documenting that live/runtime gate instead of masking it.
>
> <details><summary>Created a file, edited 11 files, ran 14 commands</summary>
>
> - Ran `rg -n "mod tests|use super" 'Feature_inplementation/DCS_gRPC_analyse/DCS-gRPC-lso-new/src/tasks/record_recovery.rs'; git status --short; git diff -- lua/DCS-gRPC/version.lua; git diff --stat; Get-Content -LiteralPath 'CHANGELOG.md' | Select-Object -First 90; rg -n "GetTransform|Services|UnitService|API" README.md docs -g '*.md' -g '*.html'`
> - Ran `Get-Content -LiteralPath 'Feature_inplementation/DCS_gRPC_analyse/DCS-gRPC-lso-new/src/tasks/record_recovery.rs' | Select-Object -Skip 1450 -First 45`
> - Wrote `./Feature_inplementation/DCS_gRPC_analyse/DCS-gRPC-lso-new/src/tasks/record_recovery.rs` (+28 -4)
> - Wrote `./CHANGELOG.md` (+6 -1)
> - Wrote `./Feature_inplementation/DCS_gRPC_analyse/GET_RECOVERY_SNAPSHOT_IMPLEMENTATION.md` (+48 -0)
> - Ran `rg -n "hook-sampling|legacy-inline|cargo run.*run|Options|--no-acmi" 'Feature_inplementation/DCS_gRPC_analyse/DCS-gRPC-lso-new/README.md' 'Feature_inplementation/DCS_gRPC_analyse/DCS-gRPC-lso-new/docs' -g '*.md'`
> - Ran `Get-Content -LiteralPath 'Feature_inplementation/DCS_gRPC_analyse/DCS-gRPC-lso-new/README.md' | Select-Object -Skip 45 -First 45; Get-Content -LiteralPath 'Feature_inplementation/DCS_gRPC_analyse/DCS-gRPC-lso-new/docs/ADMIN_GUIDE.md' | Select-Object -First 60; Get-Content -LiteralPath 'Feature_inplementation/DCS_gRPC_analyse/DCS-gRPC-lso-new/docs/BENCHMARK_PROTOCOL.md' | Select-Object -Skip 55 -First 45`
> - Wrote `./Feature_inplementation/DCS_gRPC_analyse/DCS-gRPC-lso-new/README.md` (+6 -1)
> - Wrote `./Feature_inplementation/DCS_gRPC_analyse/DCS-gRPC-lso-new/docs/ADMIN_GUIDE.md` (+7 -4)
> - Wrote `./Feature_inplementation/DCS_gRPC_analyse/DCS-gRPC-lso-new/docs/BENCHMARK_PROTOCOL.md` (+17 -0)
> - Ran `git diff -- Cargo.toml Cargo.lock src/lib.rs src/rpc.rs src/server.rs src/stats.rs stubs/build.rs stubs/src/lib.rs protos/dcs/dcs.proto lua/DCS-gRPC/grpc.lua CHANGELOG.md; Get-Content -LiteralPath 'ipc/src/ipc.rs'; Get-Content -LiteralPath 'stubs/src/recovery.rs'; Get-Content -LiteralPath 'src/stats.rs' | Select-Object -First 300`
> - Ran `$crate='C:\Users\thierry\.cargo\registry\src'; Get-ChildItem -LiteralPath $crate -Directory | ForEach-Object { Join-Path $_.FullName 'dcs-module-ipc-0.9.1' } | Where-Object { Test-Path -LiteralPath $_ } | ForEach-Object { Get-ChildItem -LiteralPath $_ -Force | Select-Object Name,Length; Get-Content -LiteralPath (Join-Path $_ '.cargo_vcs_info.json') }`
> - Ran `cargo audit --locked; cargo clippy --locked --workspace --all-targets -- -D warnings`
> - Wrote `./src/lib.rs` (+22 -18)
> - Wrote `./src/stats.rs` (+22 -21)
> - Ran `cargo fmt --all; cargo audit; cargo clippy --locked --workspace --all-targets -- -D warnings`
> - Ran `cargo audit`
> - Ran `cargo fmt -- --check; cargo clippy --locked --all-targets -- -D warnings; cargo test --locked --no-fail-fast`
> - Wrote `./Feature_inplementation/DCS_gRPC_analyse/DCS-gRPC-lso-new/src/client/recovery_client.rs` (+8 -4)
> - Wrote `./Feature_inplementation/DCS_gRPC_analyse/DCS-gRPC-lso-new/src/client/recovery_client.rs` (+3 -0)
> - Wrote `./Feature_inplementation/DCS_gRPC_analyse/DCS-gRPC-lso-new/src/tasks/record_recovery.rs` (+10 -2)
> - Wrote `./Feature_inplementation/DCS_gRPC_analyse/DCS-gRPC-lso-new/src/telemetry.rs` (+3 -0)
> - Wrote `./Feature_inplementation/DCS_gRPC_analyse/DCS-gRPC-lso-new/src/track.rs` (+4 -0)
> - Wrote `./Feature_inplementation/DCS_gRPC_analyse/GET_RECOVERY_SNAPSHOT_IMPLEMENTATION.md` (+1 -1)
> - Ran `cargo fmt; cargo fmt -- --check; cargo check --locked --all-targets; cargo clippy --locked --all-targets -- -D warnings; cargo test --locked auto_falls_back_only_when_snapshot_rpc_is_unimplemented; cargo test --locked hook_timeline_compaction_preserves_recent_final_evidence_without_buffer_limit`
> - Ran `Get-Content -LiteralPath 'ipc/Cargo.toml'; Get-Content -LiteralPath 'Cargo.toml' | Select-Object -First 42; git diff --check; cargo fmt --all -- --check; cargo check --locked --workspace --all-targets; cargo clippy --locked --workspace --all-targets -- -D warnings; cargo test --locked --workspace --no-run`
> - Wrote `./Feature_inplementation/DCS_gRPC_analyse/GET_RECOVERY_SNAPSHOT_IMPLEMENTATION.md` (+14 -1)
> - Ran `git -C 'Feature_inplementation/DCS_gRPC_analyse/DCS-gRPC-lso-new' status --short; git -C 'Feature_inplementation/DCS_gRPC_analyse/DCS-gRPC-lso-new' diff --stat; git status --short; rg -n "schema_version: 4|acquisition_mode|observation_sequence|request_round_trip_ms|recovery_telemetry_mode|RecoverySnapshot" 'Feature_inplementation/DCS_gRPC_analyse/DCS-gRPC-lso-new/src' 'Feature_inplementation/DCS_gRPC_analyse/DCS-gRPC-lso-new/README.md' 'Feature_inplementation/DCS_gRPC_analyse/DCS-gRPC-lso-new/docs'`
> - Ran `Get-Content -LiteralPath 'protos/dcs/spot/v0/spot.proto' | Select-Object -First 30; Get-Content -LiteralPath 'protos/dcs/unit/v0/unit.proto' | Select-Object -First 20; Get-Content -LiteralPath 'stubs/build.rs' | Select-Object -First 55; Get-Content -LiteralPath 'stubs/src/common.rs' | Select-Object -First 110`
>
> </details>
>
> Implementation is complete through the static-validation gate. I’ve intentionally left `StreamUnits` throttling/priority untouched because the reviewed plan makes that conditional on live contention evidence; the remaining work is the controlled DCS A/B run, not another speculative code change.
>
> <details><summary>Ran a command</summary>
>
> - Ran `$p='C:\Users\thierry\.codex\memories\MEMORY.md'; $lines=Get-Content -LiteralPath $p; for($i=15;$i -le 31;$i++){ '{0}: {1}' -f ($i+1),$lines[$i] }; git diff --check; git status --short`
>
> </details>
>
> <details><summary>Plan</summary>
>
> The targeted implementation is code-complete and statically validated. Conditional StreamUnits/fairness changes were intentionally not made because the plan requires live evidence first; controlled DCS A/B validation remains an operational follow-up.
>
> - [x] Inspect repository, implementation plan, and current acquisition/IPC paths
> - [x] Implement the independent hook-buffer prerequisite in the LSO copy
> - [x] Vendor and extend mission IPC with cancellation, bounds, and diagnostics
> - [x] Add the callback-atomic RecoveryService protobuf, stubs, Rust handler, and Lua method
> - [x] Add LSO auto/legacy/atomic acquisition, deadlines, skip scheduling, and persisted provenance
> - [x] Update documentation and run static/unit validation
>
> </details>
</details>

Implemented the targeted recovery-snapshot change.

Key results:

- Added callback-atomic `RecoveryService.GetRecoverySnapshot`: [recovery.proto](./protos/dcs/recovery/v0/recovery.proto)
- Added the mission Lua handler: [recovery.lua](./lua/DCS-gRPC/methods/recovery.lua)
- Vendored the existing [`dcs-module-ipc` 0.9.1 source](https://github.com/rkusa/dcs-module-ipc) with bounded queues, cancellation removal, correlation IDs, and timing diagnostics: [ipc](./ipc/README.md)
- Added LSO `auto`, `legacy`, and `atomic` modes, 250 ms default deadline, skipped ticks, sequence/RTT persistence, and schema v4 reporting.
- Fixed hook evidence retention by keeping the most recent 512 observations.
- Preserved grading, geometry, smoothing, generic RPCs, and V/STOL grading behavior.
- Documented implementation and pending live gates: [GET_RECOVERY_SNAPSHOT_IMPLEMENTATION.md](./Feature_inplementation/DCS_gRPC_analyse/GET_RECOVERY_SNAPSHOT_IMPLEMENTATION.md)

Validation:

- Server workspace check and strict Clippy: passed.
- Recovery stub tests: 4 passed.
- LSO check and strict Clippy: passed.
- New focused LSO tests: passed.
- Full LSO suite: 102 passed, one pre-existing grading expectation failure.
- IPC tests compile, but execution requires `lua.dll`.
- `cargo audit` reports 12 vulnerabilities in the existing lockfile; dependency remediation was kept separate.
- Live DCS four-cell A/B validation remains required.
- `StreamUnits` throttling/priority was intentionally not implemented because the plan requires measured contention first.

Important: `Feature_inplementation` is ignored by the root repository, so the LSO-copy changes and implementation record do not appear in root `git status`.
