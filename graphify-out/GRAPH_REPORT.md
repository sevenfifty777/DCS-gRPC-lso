# Graph Report - DCS-gRPC-lso  (2026-08-28)

## Corpus Check
- Large corpus: 192 files ╖ ~711,371 words. Semantic extraction will be expensive (many Claude tokens). Consider running on a subfolder.

## Summary
- 561 nodes · 854 edges · 92 communities (41 shown, 51 thin omitted)
- Extraction: 96% EXTRACTED · 4% INFERRED · 0% AMBIGUOUS · INFERRED: 32 edges (avg confidence: 0.84)
- Token cost: 276,767 input · 19,480 output

## Community Hubs (Navigation)
- Grading Logic
- Drawing & Graphics
- Time & Errors
- Async Context
- Position & Tracking
- Atmosphere & Hook Clients
- Command Execution
- Database & Web
- Recovery Tasks
- LSO Notation
- Unit Client
- Mission Client
- Recovery Detection
- DCS gRPC Lua
- Net Client
- Community 15
- Community 16
- Community 17
- Community 18
- Community 19
- Community 20
- Community 22
- Community 23
- Community 24
- Community 30
- Community 31
- Community 32
- Community 33
- Community 34
- Community 35
- Community 36
- Community 37
- Community 38
- Community 39
- Community 40
- Community 41
- Community 42
- Community 43
- Community 44
- Community 45
- Community 46
- Community 47
- Community 48
- Community 49
- Community 50
- Community 51
- Community 52
- Community 53
- Community 54
- Community 55
- Community 56
- Community 57
- Community 58
- Community 59
- Community 60
- Community 61
- Community 62
- Community 63
- Community 64
- Community 80
- Community 81
- Community 82
- Community 83
- Community 84
- Community 85
- Community 86
- Community 87
- Community 88
- Community 89
- Community 91

## God Nodes (most connected - your core abstractions)
1. `gates_deg()` - 26 edges
2. `Track` - 23 edges
3. `draw_side_view()` - 21 edges
4. `TrackResult` - 21 edges
5. `Transform` - 19 edges
6. `draw_top_view()` - 17 edges
7. `AirplaneInfo` - 15 edges
8. `TaskParams` - 15 edges
9. `CarrierPlanePair` - 14 edges
10. `ShutdownHandle` - 14 edges

## Surprising Connections (you probably didn't know these)
- `LSO Example Report` --references--> `DCS-gRPC LSO README`  [EXTRACTED]
  docs/example.png → README.md
- `LSO Glideslope Analysis` --conceptually_related_to--> `UnitService`  [INFERRED]
  trap sample/LSO-20260729-215701-Nazgul91NOeZJustice.png → docs/DCS-gRPC/api.html
- `LSO Trap Pattern Analysis` --conceptually_related_to--> `UnitService`  [INFERRED]
  trap sample/LSO-20260729-215701-Nazgul91NOeZJustice-pattern.png → docs/DCS-gRPC/api.html
- `DCS-gRPC LSO README` --references--> `LSO Installation and Administration Guide`  [EXTRACTED]
  README.md → docs/ADMIN_GUIDE.md
- `DCS-gRPC LSO README` --references--> `LSO Technical Analysis`  [EXTRACTED]
  README.md → docs/LSO_ANALYSIS.md

## Import Cycles
- 1-file cycle: `src/track.rs -> src/track.rs`

## Hyperedges (group relationships)
- **DCS-gRPC API Core Services** — dcs_atmosphere_v0_atmosphere_service, dcs_coalition_v0_coalition_service, dcs_controller_v0_controller_service, dcs_group_v0_group_service, dcs_mission_v0_mission_service [EXTRACTED 1.00]
- **DCS-gRPC Core Services** — dcs_mission_v0_missionservice, dcs_net_v0_netservice, dcs_spot_v0_spotservice, dcs_srs_v0_srsservice, dcs_timer_v0_timerservice, dcs_trigger_v0_triggerservice [INFERRED 0.90]
- **DCS gRPC API Services** — docs_dcs_grpc_api_unit_service, docs_dcs_grpc_api_warehouse_service, docs_dcs_grpc_api_weapon_service, docs_dcs_grpc_api_world_service [EXTRACTED 1.00]
- **LSO Grading and Analysis Flow** — docs_dcs_grpc_api_unit_service, img_trap_sample_pattern, img_trap_sample_glideslope [INFERRED 0.90]
- **Meteor 8-6 Phenex Landing Attempts** — trap_sample_lso_20260730_001054_meteor86phenex, trap_sample_lso_20260730_001322_meteor86phenex, trap_sample_lso_20260804_225836_meteor86phenex, trap_sample_lso_20260804_230252_meteor86phenex, trap_sample_lso_20260804_230605_meteor86phenex, trap_sample_lso_20260804_230848_meteor86phenex [EXTRACTED 1.00]
- **Phoenix 1-3 Mioril Landing Attempts** — trap_sample_lso_20260804_224253_phoenix13miorilfb, trap_sample_lso_20260804_224434_phoenix13miorilfb [EXTRACTED 1.00]
- **Meteor 8-6 Phenex Landing Data** — trap_sample_lso_20260804_231124_meteor86phenex, trap_sample_lso_20260804_231919_meteor86phenex, trap_sample_lso_20260804_233131_meteor86phenex, trap_sample_lso_20260804_234613_meteor86phenex, trap_sample_lso_20260825_022635_meteor86phenex, trap_sample_lso_20260825_023229_meteor86phenex, trap_sample_lso_20260825_023805_meteor86phenex, trap_sample_lso_20260825_024307_meteor86phenex [EXTRACTED 0.90]
- **PARTY 4-1 ERGO Landing Data** — trap_sample_lso_20260804_232546_party41ergonoez, trap_sample_lso_20260804_234124_party41ergonoez [EXTRACTED 0.90]
- **Meteor 8-6 Landing Session 2026-08-25** — trap_sample_lso_20260825_024309_meteor86phenex, trap_sample_lso_20260825_024749_meteor86phenex, trap_sample_lso_20260825_025506_meteor86phenex, trap_sample_lso_20260825_025825_meteor86phenex, trap_sample_lso_20260825_025826_meteor86phenex, trap_sample_lso_20260825_030251_meteor86phenex, trap_sample_lso_20260825_031018_meteor86phenex [EXTRACTED 1.00]

## Communities (92 total, 51 thin omitted)

### Community 0 - "Grading Logic"
Cohesion: 0.06
Nodes (42): Default, compute_pass_grade(), compute_vstol_approach_grade_points(), compute_vstol_final_grade_from_points(), gates_deg(), grade_from_gates(), grade_single_gate(), map_vstol_approach_points_to_grade() (+34 more)

### Community 1 - "Drawing & Graphics"
Cohesion: 0.09
Nodes (45): BitMapBackend, DrawingArea, DrawingAreaErrorKind, DynamicImage, ErrorType, Hint, ImageError, Range (+37 more)

### Community 2 - "Time & Errors"
Cohesion: 0.09
Nodes (28): OffsetDateTime, ParseError, PartialEq, Read, SerenityError, CarrierPlanePair, execute(), extract_recoveries() (+20 more)

### Community 3 - "Async Context"
Cohesion: 0.10
Nodes (22): Context, F, Future, Output, Pin, Poll, Receiver, S (+14 more)

### Community 4 - "Position & Tracking"
Cohesion: 0.10
Nodes (21): DRotor3, From, Orientation, Position, PatternDatum, DVec3, Into, Option (+13 more)

### Community 5 - "Atmosphere & Hook Clients"
Cohesion: 0.08
Nodes (20): AtmosphereServiceClient, HookServiceClient, AtmosphereClient, Channel, Result, Self, Status, HookClient (+12 more)

### Community 6 - "Command Execution"
Cohesion: 0.13
Nodes (25): Duration, Instant, check_candidate(), execute(), Opts, print_greenie_board(), Arc, Channel (+17 more)

### Community 7 - "Database & Web"
Cohesion: 0.11
Nodes (21): Connection, Html, Json, Mutex, DbPass, outcome_round_trips_through_sqlite(), RecoveryDb, Option (+13 more)

### Community 8 - "Recovery Tasks"
Cohesion: 0.11
Nodes (19): Coalition, Coords, HashSet, IntoIterator, changed_precision(), color(), create_initial_update(), record_recovery() (+11 more)

### Community 9 - "LSO Notation"
Cohesion: 0.16
Nodes (9): build_phrase(), greedy_decode(), lookup_deviation(), lookup_position(), Option, String, Vec, to_english() (+1 more)

### Community 10 - "Unit Client"
Cohesion: 0.21
Nodes (10): Channel, Into, Result, Self, Status, String, Unit, UnitServiceClient (+2 more)

### Community 11 - "Mission Client"
Cohesion: 0.21
Nodes (10): Event, MissionServiceClient, MissionClient, Channel, Item, Result, Self, Status (+2 more)

### Community 12 - "Recovery Detection"
Cohesion: 0.21
Nodes (13): detect_recovery_attempt(), is_recovery_attempt(), Result, CompletedPass, Arc, Channel, HashMap, Option (+5 more)

### Community 14 - "Net Client"
Cohesion: 0.24
Nodes (8): GetPlayerInfo, NetServiceClient, NetClient, Channel, Result, Self, Status, Vec

### Community 15 - "Community 15"
Cohesion: 0.22
Nodes (9): LSO Installation and Administration Guide, DCS-gRPC Fork Migration, LSO Example Report, LSO Grading Reference, Approach View Diagram, Groove View Diagram, LSO Technical Analysis, DCS-gRPC LSO README (+1 more)

### Community 16 - "Community 16"
Cohesion: 0.33
Nodes (7): CoalitionService, Group, Unit, ControllerService, CustomService, GroupService, MissionService

### Community 17 - "Community 17"
Cohesion: 0.33
Nodes (6): MissionService, StreamEvents, StreamEventsResponse, BaseCaptureEvent, LandingQualityMarkEvent, StreamUnits

### Community 19 - "Community 19"
Cohesion: 0.50
Nodes (4): F-14B(U) Tomcat, Meteor 8-6 | Phenex, LSO Grade Sheet (02:43:09), Landing Pattern Diagram (02:43:09)

### Community 20 - "Community 20"
Cohesion: 0.50
Nodes (4): Sensor, UnitService, LSO Glideslope Analysis, LSO Trap Pattern Analysis

## Knowledge Gaps
- **97 isolated node(s):** `lso`, `LSO Changelog`, `Contributing to DCS-gRPC LSO`, `AV-8B / LHA Tarawa V/STOL support`, `LSO Installation and Administration Guide` (+92 more)
  These have ≤1 connection - possible missing edges or undocumented components.
- **51 thin communities (<3 nodes) omitted from report** — run `graphify query` to explore isolated nodes.

## Suggested Questions
_Questions this graph is uniquely positioned to answer:_

- **Why does `Transform` connect `Position & Tracking` to `Grading Logic`, `Unit Client`, `Time & Errors`, `Recovery Detection`?**
  _High betweenness centrality (0.100) - this node is a cross-community bridge._
- **Why does `Track` connect `Position & Tracking` to `Grading Logic`, `Drawing & Graphics`, `Time & Errors`, `Recovery Tasks`?**
  _High betweenness centrality (0.098) - this node is a cross-community bridge._
- **Why does `ShutdownHandle` connect `Command Execution` to `Async Context`, `Recovery Detection`?**
  _High betweenness centrality (0.097) - this node is a cross-community bridge._
- **Are the 5 inferred relationships involving `draw_side_view()` (e.g. with `ft_to_nm()` and `m_to_ft()`) actually correct?**
  _`draw_side_view()` has 5 INFERRED edges - model-reasoned connections that need verification._
- **What connects `lso`, `LSO Changelog`, `Contributing to DCS-gRPC LSO` to the rest of the system?**
  _97 weakly-connected nodes found - possible documentation gaps or missing edges._
- **Should `Grading Logic` be split into smaller, more focused modules?**
  _Cohesion score 0.06284153005464481 - nodes in this community are weakly interconnected._
- **Should `Drawing & Graphics` be split into smaller, more focused modules?**
  _Cohesion score 0.08853410740203194 - nodes in this community are weakly interconnected._