Date: 2026-06-10
Version analyzed: 0.2.0
Repository: DCS-gRPC/lso

Table of Contents
Overview
Architecture
Data Collection Pipeline
Coordinate System & Transformations
Data Collected Per Frame
Static Reference Data
Grading & Cable Estimation Logic
Data Export Formats
Current Limitations & Gaps
Potential Improvements & New Features
Source File Map
Overview
LSO is a Rust async CLI tool (Tokio runtime) that connects to a running DCS World server via DCS-gRPC (gRPC/tonic protocol). It automatically:

Detects carrier recovery attempts in real-time
Tracks the aircraft's approach path relative to the carrier's angled deck
Estimates which arresting cable was caught
Generates visual PNG reports and TacView-compatible ACMI recordings
Optionally posts reports to Discord via webhook
It has two operating modes:

Mode	CLI Command	Description
Live	lso.exe run	Connects to a live DCS-gRPC session, monitors all carrier/plane pairs
Replay	lso.exe file <acmi>	Re-processes a previously saved .zip.acmi recording offline
Architecture
High-Level Data Flow

DCS World
  │
  ▼
┌─────────────────────────────────────────────┐
│ DCS-gRPC Rust Server (v0.8.1)               │
│ Exposes: CoalitionService, GroupService,    │
│          UnitService, MissionService,        │
│          HookService                         │
└──────────────┬──────────────────────────────┘
               │ gRPC (tonic) over HTTP/2
               ▼
┌─────────────────────────────────────────────┐
│ LSO CLI (tokio async runtime)               │
│                                              │
│ ┌───────────┐   ┌────────────────────────┐  │
│ │ commands/  │   │ tasks/                 │  │
│ │  ├─ run.rs │──▶│  ├─ detect_recovery    │  │
│ │  └─ file.rs│   │  └─ record_recovery    │  │
│ └───────────┘   └───────────┬────────────┘  │
│                              │               │
│                   ┌──────────▼───────────┐   │
│                   │ track.rs (Track)     │   │
│                   │ transform.rs         │   │
│                   │ data.rs (CarrierInfo │   │
│                   │          AirplaneInfo)│   │
│                   └──────────┬───────────┘   │
│                              │               │
│             ┌────────────────┼────────────┐  │
│             ▼                ▼            ▼  │
│      draw.rs         track.rs       record_ │
│      (PNG chart)     (cable est.)   recovery│
│                                      (.acmi)│
│                                      (Disc.)│
└─────────────────────────────────────────────┘
Module Structure

lso/src/
├── main.rs            # CLI parsing (clap), tracing init, shutdown handling
├── client/
│   ├── mod.rs         # Re-exports
│   ├── hook_client.rs    # HookServiceClient wrapper
│   ├── mission_client.rs # MissionServiceClient wrapper (events, scenario time)
│   └── unit_client.rs    # UnitServiceClient wrapper (transforms, units, descriptors)
├── commands/
│   ├── mod.rs
│   ├── run.rs         # Live mode: discover units, spawn detection tasks, birth events
│   └── file.rs        # Replay mode: parse ACMI files, extract tracks
├── tasks/
│   ├── mod.rs         # TaskParams shared context struct
│   ├── detect_recovery_attempt.rs  # 2s polling loop, approach detection
│   └── record_recovery.rs         # 100ms recording loop, ACMI write, Discord post
├── data.rs            # Static carrier & aircraft reference data
├── track.rs           # Track data structure, grading, cable estimation
├── transform.rs       # DCS gRPC → Transform conversion (position/orientation)
├── draw.rs            # PNG chart generation (plotters library)
├── error.rs           # Unified error enum
└── utils/
    ├── mod.rs         # Unit conversions (m↔nm, m↔ft, ft↔nm)
    ├── interval.rs    # Abortable tokio interval stream
    ├── precision.rs   # Float precision rounding trait
    └── shutdown.rs    # Graceful shutdown signal mechanism
Key Dependencies
Crate	Version	Purpose
tokio	1.2	Async runtime (multi-thread, signals, fs, sync)
tonic	0.11	gRPC client
dcs-grpc-stubs	0.8.1	Generated protobuf stubs from rust-server
plotters / plotters-bitmap	0.3	PNG chart rendering
tacview	0.2	ACMI file format read/write
serde_json	1.0	Discord user mapping deserialization
serenity	0.12	Discord webhook HTTP client
ultraviolet	0.9	3D vector/quaternion math (f64)
clap	4.0	CLI argument parsing
backoff	0.4	Exponential retry on connection loss
Data Collection Pipeline
Phase 1 — Initial Unit Discovery (commands/run.rs, lines 97–169)
On startup, the run command:

Connects to DCS-gRPC via tonic::Endpoint (default http://127.0.0.1:50051)
Calls CoalitionService.GetGroups(All) to enumerate all mission groups
Filters groups to GroupCategory::Airplane and GroupCategory::Ship only
For each group, calls GroupService.GetUnits(active=true) to get all active units
For each unit, calls UnitService.GetDescriptor() to retrieve unit attributes
Classifies units via check_candidate():
Carrier → if attributes contain "AircraftCarrier With Arresting Gear" AND type name matches a known carrier (CarrierInfo::by_type())
Plane → if type name matches a known aircraft AND (has a player name OR --ki flag is set)
Stores results in two HashMaps:
planes: HashMap<String, (u32, String, &'static AirplaneInfo)> keyed by unit name
carriers: HashMap<String, (u32, &'static CarrierInfo)> keyed by unit name
Spawns a detect_recovery_attempt Tokio task for every carrier × plane combination (N_carriers × N_planes tasks)
Phase 2 — Live Birth Event Listening (commands/run.rs, lines 226–294)
A background async task runs concurrently:

Streams MissionService.StreamEvents() continuously
Watches for Event::Birth(BirthEvent) containing a Unit initiator
On new airplane birth → spawns recovery detection tasks for all existing carriers
On new carrier birth → spawns recovery detection tasks for all existing planes
Propagates errors through an mpsc channel to the main task
Phase 3 — Recovery Attempt Detection (tasks/detect_recovery_attempt.rs)
Each spawned detection task runs a polling loop at 2-second intervals using tokio::time::interval.

For each tick, it calls UnitService.GetTransform() for both the carrier and plane simultaneously via futures::try_join.

A recovery attempt is detected when ALL of these conditions are true:


// 1. Plane below 500 ft AGL
m_to_ft(plane.alt) <= 500.0

// 2. Distance between 200m and 1.5 NM
//    (200m minimum filters out takeoffs on the deck)
let distance = (carrier.position - plane.position).mag();
m_to_nm(distance) <= 1.5 && distance >= 200.0

// 3. Plane is behind the carrier
//    (carrier forward vector · vector from plane to carrier > 0)
carrier.forward.normalized()
    .dot((carrier.position - plane.position).normalized()) > 0.0

// 4. Plane nose roughly points toward carrier
//    (plane forward · vector to carrier > 0.65, ~49° cone)
plane.forward.normalized()
    .dot((carrier.position - plane.position).normalized()) > 0.65
When all conditions are met, the task transitions to high-frequency recording.

Phase 4 — High-Frequency Recording (tasks/record_recovery.rs)
Once a recovery is detected, recording starts:

Polling rate: 100ms (10 Hz)
gRPC calls per tick: 2 × GetTransform (carrier + plane) simultaneously
Stream: MissionService.StreamEvents() for landing and LSO events
Output format: Compressed TacView ACMI via tacview::Writer
Recording is written to an in-memory buffer (Cursor<Vec<u8>>) and flushed to disk only when complete, on a per-second basis, lowering the frequency of writes.

Each tick:

Fetches carrier and plane transforms
Writes reference coordinates on first frame (ReferenceLatitude, ReferenceLongitude)
Creates Update records with delta-compressed coordinates (only changed values)
Writes Frame marker + both updates, handling time ordering
Feeds transforms into Track::next() for cable estimation and datum accumulation
Phase 5 — Landing Event Handling
Two types of DCS events are consumed:

Event	gRPC Type	Purpose
LandingQualityMark	Event::LandingQualityMark	DCS native LSO grade (e.g. "WIRE# 3")
RunwayTouch	Event::RunwayTouch	Physical landing event (hook touchdown)
Both events carry Initiator::Unit (plane) and Place::Airbase::Unit (carrier), which are matched against the tracked IDs.

On RunwayTouch:

Carrier and plane transforms at that time are written to ACMI
A Landed event is written to ACMI
Track::landed() is called → cable estimation runs
A 10-second post-landing timer starts (to capture the roll-out)
On LandingQualityMark:

DCS's grade text is stored via Track::set_dcs_grading()
A Message event with the grade is written to ACMI
Termination Conditions
Recording stops when any of:

Bolter detected: distance from plane to landing position increases by >150m from minimum
Post-landing timeout: 10 seconds elapsed after RunwayTouch
Unit lost: Crash, Dead, PlayerLeaveUnit, or UnitLost events for either carrier or plane
Too high: plane was never below 100 ft → recording discarded entirely (not a real approach)
Coordinate System & Transformations
Position Conventions
DCS uses a right-hand coordinate system where +x points north. The LSO converts this to a left-hand system where +z points north (and +x points east) for compatibility with TacView and standard aviation conventions:


fn fix_vector(v: Vector) -> DVec3 {
    DVec3::new(v.z, v.y, v.x)  // DCS (x=N,y=alt,z=E) → (x=E,y=alt,z=N)
}
Transform Structure
Each GetTransform response is converted into a Transform struct:

Field	Source	Precision	Notes
position	Position.u, alt, Position.v	±0.01 m	v.z→x, alt→y, v.x→z (fix_vector)
lat, lon	Position.lat, Position.lon	7 decimals	~1.1 cm precision
alt	Position.alt	±0.01 m	
heading	Orientation.heading	±0.1°	
yaw	Orientation.yaw	±0.1°	
pitch	Orientation.pitch	±0.1°	
roll	Orientation.roll	±0.1°	
forward	Computed	—	(sin(yaw)·cos(pitch), sin(pitch), cos(yaw)·cos(pitch))
rotation	Computed	—	DRotor3 quaternion from Euler angles (roll, pitch, heading)
aoa	Computed	±0.01°	acos(forward · velocity.normalized())
time	Scenario time	±0.01 s	
Important: forward is computed from yaw/pitch rather than taken from the gRPC response to ensure consistency between live mode and ACMI replay mode (where only yaw/pitch are available).

Carrier-Relative Deck Coordinates
For each tracking frame, the plane's position is projected into the angled deck coordinate system:

x-axis: Aligned to the angled deck = carrier.heading - carrier.deck_angle rotated in XZ plane
y-axis: Perpendicular to the deck axis (positive = left of centerline)
Origin x=0: The optimal touchdown point (midpoint between cables 2 and 3, adjusted for hook offset and glide slope angle)

// x = distance along deck centerline
let x = ray_from_plane_to_carrier.dot(deck_forward);

// y = lateral offset (positive = left)
let y = sqrt(distance² - x²);
if ray_from_plane_to_carrier.dot(deck_right) > 0.0 {
    y = -y;  // plane is right of centerline
}
Data Collected Per Frame
The Track struct accumulates Datum records. Each datum contains:

Field	Unit	Meaning
x	meters	Distance along angled deck centerline from optimal touchdown point
y	meters	Lateral offset from glide slope centerline (+ = right, - = left)
aoa	degrees	Aircraft angle of attack
alt	meters	Hook altitude above carrier deck (computed as plane.alt - deck_alt + hook_offset.y, clamped ≥ 0)
What is NOT currently tracked:

Airspeed (could be derived from velocity.mag())
Vertical speed / sink rate
Engine RPM / throttle position
Wind speed and direction at the carrier
Aircraft weight / fuel state
Static Reference Data
Carriers
Type Name(s)	Class	Deck Angle	Deck Altitude	Notes
CVN_71, CVN_72, CVN_73, CVN_75, Stennis	Nimitz	9.1359°	20.1494 m	Theodore Roosevelt, Abraham Lincoln, George Washington, Harry S. Truman
Forrestal (CV-59)	Forrestal	9.42°	18.46 m	
Cable pendant positions (left/right connectors) are stored as 3D vectors relative to the ship's origin, extracted from DCS ModelViewer2 using the Connector Tool:


Cable 1: POINT_TROS_01_01 / POINT_TROS_01_02
Cable 2: POINT_TROS_02_01 / POINT_TROS_02_02
Cable 3: POINT_TROS_03_01 / POINT_TROS_03_02
Cable 4: POINT_TROS_04_01 / POINT_TROS_04_02
The optimal landing offset is calculated as the midpoint between cable 2 and cable 3, adjusted for the plane's hook position rotated by the glide slope angle.

Aircraft
Type Name	Hook Offset (x, y, z) m	Glide Slope
FA-18C_hornet	(0, -2.241, -7.237)	3.5°
F-14A-135-GR, F-14B, F-14A/B, F-14B(U)	(0, -1.979, -6.564)	3.5°
T-45	(0, -1.779, -4.783)	3.5°
AOA Rating Thresholds
Rating	FA-18C	F-14A/B	T-45
Fast (red)	≤ 6.9°	≤ 9.7°	≤ 6.9° ⚠️
Slightly Fast (amber)	≤ 7.4°	≤ 10.2°	≤ 7.4° ⚠️
On Speed (yellow)	< 8.8°	< 11.1°	< 8.8° ⚠️
Slightly Slow (lime)	< 9.3°	< 11.6°	< 9.3° ⚠️
Slow (green)	≥ 9.3°	≥ 11.6°	≥ 9.3° ⚠️
⚠️ T-45 AOA brackets are explicitly flagged as "potentially wrong" in the source code — they currently use the same thresholds as the FA-18C. Real T-45 approach AOA is different.

Grading & Cable Estimation Logic
Cable Estimation Algorithm (track.rs, lines 172–241)
When a RunwayTouch event fires:

Compute hook touchdown position = plane.position + hook_offset (rotated by plane attitude)
Compensate +3 meters forward (because the land event fires slightly after the wire is caught, so the hook is already past the wire)
Compute the forward direction of the angled deck (carrier heading minus deck angle)
For each cable (1→4):
Calculate the midpoint between left and right pendants
Transform midpoint to world space using carrier rotation
Check if the cable midpoint is behind the hook touchdown (dot product with deck forward > 0)
Return the first cable that is behind the touchdown position

for (nr, mid_cable) in cables {
    let ray_to_cable = touchdown - mid_cable;
    if ray_to_cable.dot(forward) > 0.0 {
        return Some(nr);  // hook has passed this cable — this is the one caught
    }
}
DCS Native Grading Override
When DCS provides a LandingQualityMark event (e.g. "WIRE# 3"):

The DCS-reported wire number is used as the primary grading cable field
The geometric estimate is preserved in cable_estimated for comparison
Grading States

Grading::Unknown
  └─ Initial state while tracking before any landing event

Grading::Bolter
  └─ Plane moved >150m away after getting closer (go-around without trapping)

Grading::Recovered {
    cable: Option<u8>,           // DCS-reported wire (or estimated if no DCS grade)
    cable_estimated: Option<u8>  // Always the geometric estimate
}
  └─ A RunwayTouch event was received
TrackResult (Final Output)

pub struct TrackResult {
    pub pilot_name: String,
    pub grading: Grading,
    pub dcs_grading: Option<String>,  // Raw DCS LSO grading text
    pub datums: Vec<Datum>,           // All recorded frames
    pub plane_info: &'static AirplaneInfo,
}
Data Export Formats
1. PNG Chart (draw.rs)
A 1000px wide bitmap image with two vertically stacked views, generated using the plotters crate.

Constants:

Width: 1000 px
X range: 0.02 to 0.78 NM from carrier (right to left)
Overlap: 130 px between top and side views
Top View (horizontal lineup):

Y range: ±0.15 NM lateral offset
Guide lines at ±0.25°, ±0.75°, ±3°, ±6° from centerline
Color coding: gray → green → yellow → red
Carrier deck silhouette rendered as background image
Approach path colored segment-by-segment by AOA rating (red=fast, amber=slightly fast, yellow=on speed, lime=slightly slow, green=slow)
Side View (glide slope):

Y range: 0–350 ft altitude
Guide lines at optimal ±0.25° (green), ±0.6°/0.7° (yellow), ±0.9°/1.5° (red)
Carrier side silhouette rendered as background image
Same AOA-colored approach path
Text overlay:

Top-left: "Pilot: {pilot_name}"
Below: Grading result (e.g. "Cable 3" or "Bolter")
Color palette:

Element	Color	Hex
Background	Dark gray	#1F2937
Text	Light gray	#9CA3AF
Fast AOA	Red	#EF4444
Slightly Fast	Amber	#EFA544
On Speed	Yellow	#FEF08A
Slightly Slow	Lime	#AAC522
Slow	Green	#22C55E
2. TacView ACMI Recording (.zip.acmi)
A compressed TacView-compatible file written using the tacview Rust crate:

Global properties:

ReferenceTime — scenario start datetime from MissionService.GetScenarioStartTime()
RecordingTime — current UTC time in RFC 3339 format
Title — "Carrier Recovery during {mission_name}"
Author — "dcs-grpc-lso v{version}"
ReferenceLatitude, ReferenceLongitude — carrier's position on first frame
Per-frame data (10 Hz):

Carrier (id=1): position (lat/lon/alt/UV), orientation (yaw/pitch/roll/heading)
Plane (id=2): position, orientation, AOA
Coordinates are delta-compressed — only changed values are written
Events:

Landed event with params [plane_id, carrier_id] — on RunwayTouch
Message event with DCS LSO grade text — on LandingQualityMark
Initial update:

Unit type, name, group, coalition color, pilot name
3. Discord Webhook (optional, record_recovery.rs)
When --discord-webhook is provided:


Embed:
├── Field: "Pilot" -> Discord @mention (if user mapping exists) or player name
├── Field: "Grading" -> "Cable #N" or "Bolter" or "unknown"
└── Attachments:
    ├── PNG chart
    └── .zip.acmi file
File naming: LSO-YYYYMMDD-HHMMSS-<PilotNameAlphanumeric>.png / .zip.acmi

4. ACMI Replay Extraction (commands/file.rs)
The file subcommand re-parses .zip.acmi recordings:

Reads frames, updates, and events from the ACMI file
Reconstructs Track objects for each carrier/plane pair
Runs the same cable estimation and draw logic
Outputs PNG charts to the current directory
Current Limitations & Gaps
Functional Gaps
#	Gap	Impact
1	No proper NATOPS LSO grading	Only reports wire number + AOA color. Real LSOs produce pass grades: OK, Fair, No Grade, Bolter, Cut Pass, Wave Off — with deviation calls at key gates
2	No glideslope deviation scoring	Chart shows it visually but no numeric score (e.g. "high at the start, low in close")
3	No lineup deviation scoring	Same — lateral offset is plotted but not quantified as a grade component
4	No power calls	Throttle data not tracked; DCS-gRPC doesn't directly expose it
5	No greenie board	No persistent database — each run is independent, no pilot statistics or history
6	No speed tracking	Airspeed is available from velocity.mag() but not recorded in datums
7	No wind/relative wind	Glide slope corrections depend on wind-over-deck, not captured
8	No flag grades	Not implemented (e.g. F/A-18 "Flag" light in the cockpit on bad approaches)
Data Gaps
#	Gap	Details
9	T-45 AOA brackets are wrong	Uses FA-18C values; explicitly noted in source code as "potentially wrong"
10	Only 2 carrier classes	Nimitz and Forrestal. No Super Carrier (CVN-78 Gerald R. Ford). CVN-74 mapped to Nimitz but is actually John C. Stennis
11	Only 4 aircraft types	FA-18C, F-14A, F-14B, T-45. Missing: AV-8B, A-4E-C, S-3B, E-2C, Su-33 (Kuznetsov), etc.
12	Polling-based detection (2s)	Could miss very short/fast approaches; event-driven proximity would be more reliable
13	No multi-carrier disambiguation	Each plane tracks all carriers simultaneously — no logic to determine which carrier is being approached
14	No config file	All thresholds are hardcoded; users can't tune detection parameters
Code Quality Notes
#	Note
15	data.rs uses #![allow(unused)] — masks dead code warnings
16	draw.rs contains commented-out AOA logic (lines 366–406)
17	commands/file.rs has #[allow(unused)] on extract_recoveries (used in tests)
18	main.rs has a TODO comment: "better error report than unwrap?" on line 67
19	m_to_nm, nm_to_m, m_to_ft, ft_to_nm, nm_to_ft are in utils/mod.rs rather than a dedicated conversions module
Potential Improvements & New Features
🔴 High Priority — Core Grading
#	Feature	Effort	Description
P1	Full NATOPS LSO Grading System	Large	Implement pass grades (OK, Fair, No Grade, Bolter, Cut Pass, WO) based on glideslope/lineup/AOA deviations at key gates: ¾ NM, ½ NM, in-close, at-the-ramp. Generate deviation calls (e.g. "H, LO, LUL, LOLO, F, SLO").
P2	Greenie Board	Medium	SQLite database storing all passes per pilot. Query API for stats (greenie % per pilot, trend charts). Discord bot command to display board.
P3	Numeric Deviation Scores	Medium	Compute glideslope deviation in tenths of degrees and lineup deviation in feet at each gate. Feed into grading system.
🟡 Medium Priority — Data & Platform
#	Feature	Effort	Description
P4	Add aircraft types	Small-Med	AV-8B (STOVL — different logic needed), A-4E-C, S-3B Viking, E-2C Hawkeye. Extract hook positions from ModelViewer2. Research AOA brackets.
P5	Add Super Carrier (CVN-78)	Small	Extract cable pendant positions from DCS ModelViewer2. Add to CarrierInfo. May need to handle different deck configuration.
P6	Fix T-45 AOA brackets	Small	Research real T-45 Goshawk approach AOA and update thresholds.
P7	Record airspeed	Small	Add speed: f64 field to Datum struct. Compute from velocity.mag() in transform.rs. Display on chart as secondary line or label at gates.
P8	Event-driven detection	Medium	Subscribe to all aircraft position events instead of polling at 2s. Watch for proximity to known carriers. More efficient and lower latency.
🟢 Lower Priority — Extensions
#	Feature	Effort	Description
P9	JSON/CSV data export	Small	Export structured approach data (datums + grades) to JSON or CSV for external analysis (Machine Learning, statistics).
P10	Web dashboard	Large	Embedded HTTP server (axum) serving live approach visualization. Real-time plots updating as aircraft gets closer.
P11	Configurable thresholds	Small	Load detection/recording parameters from lso.toml configuration file. Allow users to adjust AOA brackets, detection distances, polling intervals.
P12	Multi-carrier disambiguation	Medium	Track which carrier a plane is actually approaching by comparing heading alignment and distance trends across carriers.
P13	SRS radio integration	Large	Connect to DCS-SRS to play automated LSO radio calls in real-time based on approach quality.
P14	Historical database	Medium	PostgreSQL/SQLite backend for storing all passes. Web-based leaderboard. Export tools for squadron debriefing.
P15	Wind-over-deck computation	Medium	Track carrier speed + natural wind to compute relative wind. Factor into glide slope corrections in grading.
P16	Ball call detection	Medium	Detect when the pilot calls "ball" and start grading from that point. Integrate with SRS voice or DCS events.
P17	Landing signal officer view (PLAT cam)	Large	Render a simulated PLAT (Pilot Landing Aid Television) view from the carrier's perspective showing the final approach.
Source File Map
File	Lines	Purpose
src/main.rs	70	Entry point, CLI parsing, tracing setup
src/commands/run.rs	337	Live mode: unit discovery, task spawning, birth events
src/commands/file.rs	383	ACMI replay mode: parse tracks, extract grades
src/tasks/detect_recovery_attempt.rs	104	2s polling, approach detection conditions
src/tasks/record_recovery.rs	518	100ms recording, ACMI write, Discord post
src/tasks/mod.rs	27	TaskParams context struct
src/track.rs	248	Track, Datum, Grading, cable estimation
src/transform.rs	80	DCS gRPC → Transform conversion
src/data.rs	273	Static carrier & aircraft reference data
src/draw.rs	452	PNG chart generation (plotters)
src/client/hook_client.rs	24	gRPC HookServiceClient wrapper
src/client/mission_client.rs	50	gRPC MissionServiceClient wrapper
src/client/unit_client.rs	66	gRPC UnitServiceClient wrapper
src/error.rs	19	Unified error enum
src/utils/mod.rs	23	Unit conversion functions
src/utils/interval.rs	13	Abortable interval stream
src/utils/precision.rs	10	Float precision rounding trait
src/utils/shutdown.rs	163	Graceful shutdown signal mechanism
src/tests.rs	57	Integration tests with ACMI recordings
Total	~2,650	Source lines of code across 19 files
Test Coverage
Integration tests verify cable estimation accuracy using recorded ACMI files:

Test	Recording	Expected Cable	Expected Estimated
wire_1_01	FA-18C	1	1
wire_2_01	FA-18C	2	2
wire_3_01	T-45	3	3
wire_4_01	FA-18C	4	4
wire_4_02	F-14A	4	4
All 5 tests pass, confirming the geometric cable estimation works correctly for all 4 cables and across aircraft types.

CLI Reference

lso.exe [OPTIONS] <COMMAND>

OPTIONS:
  -v, --verbose    Increase log verbosity (repeatable: -v = DEBUG, -vv = TRACE)
  --color          Enable colorized output

COMMANDS:
  run     Connect to DCS-gRPC to track carrier recoveries in real-time
  file    Extract carrier recoveries from ACMI recordings

run OPTIONS:
  -o, --out-dir <DIR>          Output directory (default: .)
  --uri <URI>                  DCS-gRPC server URI (default: http://127.0.0.1:50051)
  --discord-webhook <URL>      Discord webhook URL for posting reports
  --discord-users <PATH>       JSON file mapping player names to Discord user IDs
  --ki                         Include KI (AI) units in tracking