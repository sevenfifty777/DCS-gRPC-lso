# AOA Collection: SimShaker vs DCS-gRPC-LSO (`feature/refonte-v3-lua-buffer`)

How angle of attack is obtained for the F/A-18C, F-14 and T-45 in SimShaker for Aviators,
how it is obtained in this repository on `feature/refonte-v3-lua-buffer`, and what — if
anything — should change.

Date: 2026-09-12
Scope examined:
- `C:\Program Files\SimShaker\SimShaker for Aviators Alpha` (Lua export + decompiled `SimShaker for Aviators Alpha.dll`)
- `DCS-gRPC-lso` @ `feature/refonte-v3-lua-buffer`

---

## 1. Executive summary

The two applications solve **different problems** and their AOA methods are not
interchangeable.

| | SimShaker | LSO `feature/refonte-v3-lua-buffer` |
|---|---|---|
| Source | `LoGetAngleOfAttack()` (DCS flight model) | derived from gRPC telemetry + wind correction |
| Transport | Lua script inside DCS | gRPC (`GetTransform` + `AtmosphereService.GetWind`) |
| Requires a Lua export script in DCS | **Yes** | No |
| Wind-aware | Implicit (true AOA by construction) | Yes, once a reference is established |
| Sideslip/crab contamination | None | None (lateral component discarded) |
| Signed (negative AOA representable) | Yes | Yes |
| Per-aircraft AOA code | None | None (one shared formula) |
| Per-aircraft constants | 1 stall threshold | 5-band indexer table |

**Conclusion: the v3 method is sound and should be kept as-is.** It is the correct
derivation given the constraints, and it is measurably better structured than SimShaker's
per-aircraft model. SimShaker's approach is **not adoptable** — it depends on a Lua export
running inside DCS, which this tool deliberately does not require.

The recommendations in §5 are therefore all *internal hardening* of v3: closing a test gap,
improving auditability, and validating with assets already in this repo. No change to the
AOA formula is recommended.

---

## 2. How SimShaker collects AOA

Included as the comparison baseline that prompted this analysis.

### 2.1 Acquisition — one generic call for every aircraft

`Data/SimShaker.lua:356-361`, executed inside DCS's `Export.lua` sandbox:

```lua
local aoa = LoGetAngleOfAttack()
if aoa == nil then
  dataTable.aoa = 0
else
  dataTable.aoa = string.format("%.2f", aoa)
end
```

`LoGetAngleOfAttack()` is a DCS Export API call returning the value **the flight model
itself computes**, in degrees. This is true AOA: wind, sideslip and air-mass movement are
already accounted for, because the flight model derives it from the airflow it is
simulating.

SimShaker does not *calculate* AOA at all — it reads it. That single line sits **outside**
any aircraft-specific branch; the F/A-18C, F-14 and T-45 all take this exact path.

### 2.2 There is no per-aircraft AOA acquisition

The Lua has `elseif` branches for two of the three aircraft
(`Data/SimShaker.lua:655-680`), but neither touches AOA:

- `FA-18C_hornet` → engine RPM only.
- `string.find(obj.Name, "F-14")` → RPM plus a `GetDevice(6)` "epoxy" variable dump into `additionalData`.
- T-45 → **no branch at all.**

In the compiled application, `Plane.CalculateAoA()` is `virtual`:

```csharp
internal virtual void CalculateAoA()
{
    try {
        CalculatedAoA = Convert.ToDouble(_masterManager.DCSDataProvider.DCS_AoA, FormatProvider);
    } catch (Exception exception) {
        Aircraft.Logger.Warn(exception, "Could not parse AoA");
        CalculatedAoA = 0.0;
    }
}
```

Across all 236 decompiled source files, **no aircraft subclass overrides it**. The pipeline is:

```
LoGetAngleOfAttack()  →  dataTable.aoa  →  DCSDataProvider.DCS_AoA (string)
  →  Plane.CalculateAoA()  →  PlaneDCS: AoA = CalculatedAoA  →  StallEffect
```

### 2.3 Per-aircraft difference: a single stall threshold

| Aircraft | Case label(s) | Module | `CriticalAoA` | `delta` |
|---|---|---|---|---|
| F/A-18C | `FA-18C_hornet` | `FA_18C` | 17.0° | 5.0 |
| F-14 | `F-14A`, `F-14B`, `F-14A-135-GR`, `F-14A-95-GR`, `F-14A-135-GR-Early` | `F_14AB` | 16.0° | 5.0 |
| T-45 | → `A-4E-C` | `A_4EC_Community` | 19.0° | 5.0 |

Consumed only by `StallEffect.Execute()` to ramp a vibration, gated on `TAS > 50` and airborne.
SimShaker needs only a stall onset point — it never needs to know *on-speed* AOA, which is
why one constant suffices there and would be useless here.

### 2.4 The T-45 is impersonating an A-4E-C

The string `T-45` **does not appear anywhere in the DLL**. It works only because the Lua
renames the aircraft before the data leaves DCS (`Data/SimShaker.lua:339-343`):

```lua
-- BEGIN VNAO T-45 SIMSHAKER V3 A4EC ALIAS
if dataTable.name == "T-45" then
  dataTable.name = "A-4E-C"
end
```

The T-45 therefore inherits the Skyhawk's 19° threshold — a value with no aerodynamic
justification for a Goshawk. The alias also exists in `Data/SimShaker.lua` but **not** in
`Data/SimShaker - Alpha.lua`.

**Relevance to this repo:** aliasing an airframe to another type silently transfers *all* of
that type's constants. Our `src/data.rs` keeps `T45` as a first-class `AirplaneInfo` with its
own thresholds — the better design, and worth protecting against future "just alias it"
shortcuts.

---

## 3. How `feature/refonte-v3-lua-buffer` collects AOA

### 3.1 No native AOA field exists in gRPC — deriving it is unavoidable

I checked the full DCS-gRPC 0.9.2 proto set. `Orientation` exposes `heading`, `yaw`,
`pitch`, `roll`, `forward`, `right`, `up`; `Velocity` exposes `heading`, `speed`,
`velocity`. **There is no AOA field anywhere.**

Unlike SimShaker, this app cannot read the flight model's value. Derivation is not a design
preference — it is the only option over pure gRPC. The question is only how well it is done.

### 3.2 The correct derivation: wind-corrected and pitch-plane-only

`src/track.rs:699-706`:

```rust
fn corrected_aoa_deg(velocity: DVec3, wind: DVec3, rotation: DRotor3) -> f64 {
    let true_airspeed = velocity - wind;
    if true_airspeed.mag_sq() <= f64::EPSILON {
        return f64::NAN;
    }
    let body = true_airspeed.rotated_by(rotation.reversed());
    (-body.y).atan2(body.z).to_degrees()
}
```

Two properties make this correct, and both matter for carrier work:

**`velocity - wind` converts ground velocity to true airspeed.** AOA is defined against the
*relative airflow*. gRPC's `Velocity.velocity` is motion over the ground; the difference is
the wind vector. Carrier recoveries are flown into ~25-30 kt of deck wind by doctrine, so
this correction is never negligible and never random — at 135 kt approach speed it is worth
roughly 1.5-2°, comparable to the entire Hornet OnSpeed band (1.4° wide). Omitting it would
bias every reading in the same direction.

**`rotated_by(rotation.reversed())` then `atan2(-body.y, body.z)` keeps only the vertical
component.** Real AOA is a pitch-plane quantity — a vane senses airflow only in the
aircraft's vertical plane of symmetry. Discarding the lateral component means sideslip and
crab, routine when correcting lineup on a moving deck, do not inflate the reading. Using
`atan2` rather than `acos` also makes the result **signed**, so genuinely negative AOA is
representable rather than clipped.

I verified the sign convention numerically: for pure pitch in level airflow the function
returns exactly the pitch angle, and for a typical approach (9° pitch, −3° flight-path
angle) it returns 12.00°. So **AOA = pitch − flight-path angle, positive nose-above-airflow**
— the expected convention.

### 3.3 Honest fallback rather than a fabricated value

`src/track.rs` (`effective_aoa`):

```rust
fn effective_aoa(&self, plane: &Transform) -> f64 {
    match &self.wind_reference {
        Some(reference) => corrected_aoa_deg(
            plane.velocity, reference.at_altitude(plane.alt), plane.rotation),
        None => plane.aoa,   // raw geometric approximation
    }
}
```

Wind comes from two `AtmosphereService.GetWind` probes taken at groove entry, interpolated
by altitude (`set_wind_reference`). Without a reference the raw geometric value from
`Transform::from` is used — never an invented one.

That raw value (`src/transform.rs:39-47`) is the unsigned total 3-D angle between the nose
axis and the ground-velocity vector. It is an approximation only: it carries both the wind
bias and the crab contamination that §3.2 removes. Its role in v3 is strictly a
last-resort fallback, and §3.4 is what keeps that safe.

### 3.4 Grading is gated on reliability

`src/track.rs:2822` sets `aoa_reliable: self.wind_reference.is_some()`, consumed by
`compute_catobar_assessment`. Without a wind reference, AOA episodes are still serialised
for audit but carry `affects_grade = false` and zero effective severity. V/STOL never uses
AOA for its score at all.

This is the right call: an uncorrected AOA can never silently penalise a pilot. It is also
what makes the §3.3 fallback acceptable rather than dangerous.

### 3.5 Per-aircraft handling — classification only

As in SimShaker, there is **one shared formula** and no per-aircraft acquisition. The
per-type knowledge lives in `AirplaneInfo::aoa_rating` (`src/data.rs`), as a 5-band indexer
rather than a single stall number:

| Aircraft | Fast | SlightlyFast | OnSpeed | SlightlySlow | Slow | Source |
|---|---|---|---|---|---|---|
| F/A-18C | ≤6.9 | ≤7.4 | <8.8 | <9.3 | ≥9.3 | VRS indexer bracket |
| F-14A/B/B(U) | ≤9.7 | ≤10.2 | <11.1 | <11.6 | ≥11.6 | Heatblur manual, `deg=(units/1.0989)−3.01` |
| T-45C | ≤6.0 | ≤6.5 | <7.5 | <8.0 | ≥8.0 | VNAO v1.0.2 `DisplayElectronicsUnit.lua`, `deg≈UNITS−10` |
| AV-8B | <10.0 | — | ≤12.0 | — | >12.0 | display/colour only |

I verified both unit conversions arithmetically:

- F-14: 15.4 units → 11.00°, 14.0 units → 9.73° — consistent with the table.
- T-45: 16.5 units → 6.5°, 18.0 units → 8.0° — consistent with the table.

These tables are calibrated against true, vane-referenced AOA, which is what a cockpit
indexer displays. That is exactly what `corrected_aoa_deg` produces — so the tables and the
computation are consistent, and the thresholds should not be touched.

### 3.6 A known DCS quirk is handled carefully

`GetWind` intermittently returns a null vector (`180°/0.0 m/s`). Per `AGENTS.md`, this was
traced to the DCS engine itself, not to this repo or the fork. Because it is mathematically
indistinguishable from genuine calm, it is never blind-rejected: each probe is retried
**once** (`query_wind_with_sentinel_retry`), and if the low probe stays suspicious the high
probe is reused for both interpolation points, with
`wind_reference_probes.low_reading_overridden_by_high` flagging the fallback and the raw
reading always preserved.

`AGENTS.md` marks this as a 10 September 2026 fix **not yet revalidated in live mission** —
picked up as P2 below.

---

## 4. Why SimShaker's method is not adoptable

1. **It requires a Lua export script inside DCS.** `LoGetAngleOfAttack()` exists only in the
   DCS Export sandbox. Adopting it means shipping and maintaining an `Export.lua` hook on
   every server, abandoning the "no server-side script" property that makes this tool
   deployable. SimShaker accepts that cost because it runs client-side, in the pilot's own
   cockpit; an LSO tool observing *other* aircraft cannot.

2. **gRPC exposes no AOA field** (§3.1), so there is nothing to simply read.

3. **Its per-aircraft model is weaker than ours.** One stall threshold per type versus 5-band
   indexer tables. SimShaker only needs stall onset; we need on-speed discrimination at
   ~0.5° resolution.

4. **v3's `corrected_aoa_deg` already recovers most of the accuracy** a flight-model read
   would give, with no in-sim script.

Cross-validating against SimShaker is also deliberately **excluded** from the
recommendations below: it is not installed by every pilot, so it cannot be part of a
repeatable validation procedure for this tool. §5 uses only assets already in this repo.

---

## 5. Recommendations for `feature/refonte-v3-lua-buffer`

All internal hardening. **No change to the AOA formula is recommended.**

### P1 — Add unit tests for AOA (largest gap)
`git grep -i aoa -- src/tests.rs` returns **nothing**. For a quantity this subtle, with
bands as narrow as 0.5°, `corrected_aoa_deg` is currently unprotected against regression.
It is pure and takes three plain arguments, so it is trivially testable. Suggested cases:

- **Pure pitch, zero wind, level airflow** → AOA == pitch. (Verified analytically: 5° → 5.00°.)
- **Typical approach**, 9° pitch with −3° flight-path angle → 12.00°. Pins
  `AOA = pitch − fpa` and guards the sign convention.
- **Pure crab, zero AOA** → ≈0. This is the case the lateral-discard exists for, and the one
  a naive total-angle implementation gets wrong; it is the single highest-value test here.
- **Pure headwind** → corrected value differs from the raw geometric one by the expected
  amount, proving the wind term is actually applied.
- **Negative AOA** → stays negative, guarding against a regression to unsigned `acos`.
- **Degenerate input**: `velocity == wind` → `NaN` rather than a panic or a bogus 0°.
- **Each `aoa_rating` table at its band boundaries** (6.9/7.4/8.8/9.3 for the Hornet, etc.),
  including the exact `<=` vs `<` edges, which differ between bands.

### P2 — Revalidate the `GetWind` sentinel fix in a live mission
`AGENTS.md` flags the 10 September 2026 fix as not yet revalidated live. Since
`wind_reference_established` and `wind_reference_probes` are already serialised, this needs
no new instrumentation — fly a session and confirm from the JSON that the sentinel is
still being caught, that the high-probe fallback engages when expected, and that
`low_reading_overridden_by_high` is set only in those cases. Closes an open flag with
existing diagnostics.

### P3 — Serialise the raw AOA alongside the corrected value
Today only the effective AOA is written to `datums`/`pattern_datums`, plus wind provenance.
Recording the raw geometric value in parallel (a separate field, not a replacement) would
let the wind-correction magnitude be measured **directly from any trap JSON**, with no
SimShaker and no extra flying. That turns the existing corpus into a validation asset and
makes a future correction regression visible in the data rather than only in tests.

### P4 — Mine the existing trap corpus and TacView replay path
The repo carries ~100 trap sample JSONs and a TacView replay command
(`src/commands/file.rs`). Replaying a fixed set through the pipeline gives a deterministic,
SimShaker-free regression baseline for AOA — including a check that
`wind_reference_established` is true as often as expected in practice, since the
`affects_grade` gate means a frequently-unestablished reference would quietly suppress AOA
grading. Combined with P3, replay diffs become quantitative.

### P5 — Evaluate `Orientation.up`/`right` instead of reconstructing rotation
`corrected_aoa_deg` uses `Transform::rotation`, built via `DRotor3::from_euler_angles` from
angles pre-rounded to 0.1° for TacView parity. gRPC supplies the `forward`/`right`/`up`
basis directly. Worth **measuring** whether the supplied basis reduces error versus
reconstruction-plus-rounding. Note the 0.1° rounding is a deliberate parity choice, so this
is an investigation, not a change to make blind — and TacView reproducibility must be
preserved either way. Lowest priority: likely a second-order effect next to the wind term.

### P6 — Document the raw value as a biased fallback
The comment on `Transform::aoa` (`src/transform.rs`) explains why it can be computed on
unrounded data, but not that it is a wind-biased, crab-contaminated approximation superseded
by `corrected_aoa_deg`. A one-line pointer prevents a future caller from mistaking it for
true AOA — a realistic risk, since it is the more convenient of the two to reach for.

### Explicitly not recommended
- **Shipping a Lua export to call `LoGetAngleOfAttack()`** (§4).
- **Cross-validating against SimShaker** — not universally installed, so not a repeatable
  procedure.
- **Aliasing the T-45 to the A-4E-C** in `data.rs`, SimShaker-style (§2.4).
- **Changing the `aoa_rating` thresholds** — their published-source derivations verify
  correctly (§3.5) and are consistent with what `corrected_aoa_deg` produces.

---

## 6. Reference map

| Concern | SimShaker | LSO `feature/refonte-v3-lua-buffer` |
|---|---|---|
| AOA acquisition | `Data/SimShaker.lua:356-361` | `src/track.rs:699-706` (`corrected_aoa_deg`) |
| Raw fallback | n/a | `src/transform.rs:39-47` |
| Effective value selection | `Plane.CalculateAoA()`, `PlaneDCS` | `Track::effective_aoa` |
| Per-aircraft constants | `MasterManager` `CriticalAoA` | `AirplaneInfo::aoa_rating`, `src/data.rs` |
| Consumer | `StallEffect.Execute()` | `classify_catobar_episodes`, `normalized_aoa_error`, `src/grading.rs` |
| Reliability gate | none | `aoa_reliable`, `src/track.rs:2822` |
| T-45 handling | alias → `A-4E-C`, `Data/SimShaker.lua:339-343` | first-class `T45` entry in `src/data.rs` |
| Wind | implicit in flight model | `set_wind_reference` + `AtmosphereService.GetWind` |
