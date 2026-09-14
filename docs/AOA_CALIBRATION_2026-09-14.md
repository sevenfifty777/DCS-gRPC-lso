# AoA calibration result

CSV rows: 211753 (pilots: Ghost-72 - TT, Justice), reports matched: 20

## Per pass: LSO computed AoA minus flight-model true AoA (degrees, groove only)

| report | pilot | type | wind ref | clock offset | matched | median | mean | p10 | p90 | per deg of bank |
|---|---|---|---|---|---|---|---|---|---|---|
| 20260914-203356 | Ghost-72 - TT | T-45 | yes | -0.65 s | 392 | -0.01 | +0.03 | -0.27 | +0.39 | +0.010 |
| 20260914-203718 | Ghost-72 - TT | T-45 | yes | -0.60 s | 574 | -0.03 | -0.01 | -0.35 | +0.37 | -0.050 |
| 20260914-203943 | Ghost-72 - TT | T-45 | yes | -0.55 s | 335 | +0.11 | +0.09 | -0.40 | +0.53 | -0.015 |
| 20260914-204144 | Ghost-72 - TT | T-45 | yes | -0.65 s | 465 | +0.09 | +0.11 | -0.29 | +0.47 | -0.032 |
| 20260914-204928 | Ghost-72 - TT | T-45 | yes | -0.60 s | 275 | +0.07 | +0.07 | -0.37 | +0.46 | -0.118 |
| 20260914-205230 | Ghost-72 - TT | T-45 | yes | -0.60 s | 317 | -0.04 | -0.03 | -0.32 | +0.20 | +0.001 |
| 20260914-210054 | Ghost-72 - TT | F-14BU | yes | -0.65 s | 281 | -0.05 | +0.00 | -0.63 | +0.83 | +0.045 |
| 20260914-210407 | Ghost-72 - TT | F-14BU | yes | -0.25 s | 300 | -0.08 | +0.03 | -0.26 | +0.31 | -0.043 |
| 20260914-212150 | Justice | F-14BU | yes (fallback) | -0.50 s | 366 | +0.21 | +0.18 | -0.29 | +0.60 | +0.031 |
| 20260914-212408 | Ghost-72 - TT | F-14BU | yes | -0.65 s | 239 | -0.55 | -0.32 | -0.95 | +0.79 | -0.016 |
| 20260914-212520 | Justice | F-14BU | yes | -0.65 s | 337 | +0.13 | +0.11 | -0.55 | +0.72 | -0.034 |
| 20260914-212638 | Ghost-72 - TT | F-14BU | yes | -0.10 s | 500 | -0.03 | -0.02 | -0.20 | +0.20 | +0.008 |
| 20260914-212753 | Justice | F-14BU | yes | -0.65 s | 320 | +0.16 | +0.12 | -0.34 | +0.56 | +0.032 |
| 20260914-213030 | Justice | F-14BU | yes | -0.50 s | 463 | +0.16 | +0.18 | -0.32 | +0.62 | +0.004 |
| 20260914-213312 | Justice | F-14BU | yes | -0.60 s | 581 | +0.08 | +0.08 | -0.43 | +0.59 | -0.024 |
| 20260914-213457 | Ghost-72 - TT | F-14BU | yes | -0.60 s | 446 | +0.13 | +0.10 | -0.38 | +0.50 | +0.014 |
| 20260914-213551 | Justice | F-14BU | yes | -0.65 s | 458 | +0.11 | +0.10 | -0.37 | +0.53 | -0.041 |
| 20260914-213801 | Ghost-72 - TT | F-14BU | yes | -0.60 s | 249 | -0.02 | +0.01 | -0.52 | +0.53 | -0.056 |
| 20260914-213841 | Justice | F-14BU | yes | -0.65 s | 361 | +0.19 | +0.16 | -0.37 | +0.57 | -0.047 |
| 20260914-214116 | Justice | F-14BU | yes | -0.65 s | 364 | +0.15 | +0.15 | -0.25 | +0.60 | -0.044 |

## Per pass: what was flown in the groove (flight model and cockpit indexer)

| report | pilot | type | outcome | DCS LSO | true AoA median | p10 | p90 | indexer share of groove |
|---|---|---|---|---|---|---|---|---|
| 20260914-203356 | Ghost-72 - TT | T-45 | T&G (CQ) | none | 9.33 | 8.49 | 9.90 | fast 2%, slightly_fast 0%, on_speed 4%, slightly_slow 9%, slow 85% |
| 20260914-203718 | Ghost-72 - TT | T-45 | Bolter | none | 9.44 | 7.97 | 10.82 | fast 8%, slightly_fast 3%, on_speed 7%, slightly_slow 12%, slow 69% |
| 20260914-203943 | Ghost-72 - TT | T-45 | T&G (CQ) | none | 5.54 | 3.90 | 8.37 | fast 86%, slightly_fast 2%, on_speed 9%, slightly_slow 1%, slow 2% |
| 20260914-204144 | Ghost-72 - TT | T-45 | Arrested — wire 2 (DCS/LQM + Rust) | C : _LULIM_  (DLIM)  _LOIC_  _PP _PIC_  _PPPIC_  _DRIC_  (LURIC)  _LOAR_  WO(AFU)TL  (DLX)  _LULX_  _FX_  3PTSIW  (EGIW)  WIRE# 2 | 5.43 | 4.61 | 6.15 | fast 100% |
| 20260914-204928 | Ghost-72 - TT | T-45 | Bolter | none | 7.84 | -0.69 | 8.48 | fast 57%, slightly_fast 20%, on_speed 16%, slightly_slow 1%, slow 6% |
| 20260914-205230 | Ghost-72 - TT | T-45 | Arrested — DCS/LQM wire 2; Rust estimate unavailable | C : _LULIM_  _LULIC_  _TMRDAR_  WIRE# 2 _EGIW_ | 7.59 | 2.07 | 8.16 | fast 86%, slightly_fast 9%, on_speed 5% |
| 20260914-210054 | Ghost-72 - TT | F-14BU | T&G (CQ) | none | 7.82 | 0.90 | 9.85 | fast 80%, slightly_fast 8%, on_speed 12% |
| 20260914-210407 | Ghost-72 - TT | F-14BU | Arrested — DCS/LQM wire 4; Rust estimate unavailable | C : _LULIM_  _TMRDIC_   _LOIC_  (DLX)  _LULX_  (DLIM)  _LULIC_  LNFIW  WIRE# 4 _EGIW_ | 3.69 | 2.21 | 5.07 | fast 100% |
| 20260914-212150 | Justice | F-14BU | T&G (CQ) | none | 5.91 | 4.38 | 7.13 | fast 100% |
| 20260914-212408 | Ghost-72 - TT | F-14BU | T&G (CQ) | none | 7.06 | 0.09 | 10.72 | fast 72%, slightly_fast 6%, on_speed 7%, slightly_slow 4%, slow 10% |
| 20260914-212520 | Justice | F-14BU | T&G (CQ) | none | 7.53 | 3.34 | 8.62 | fast 100% |
| 20260914-212638 | Ghost-72 - TT | F-14BU | Approach only — outcome unknown | none | 8.71 | 6.02 | 9.88 | fast 78%, slightly_fast 11%, on_speed 7%, slightly_slow 3% |
| 20260914-212753 | Justice | F-14BU | T&G (CQ) | none | 6.21 | 2.42 | 7.18 | fast 100% |
| 20260914-213030 | Justice | F-14BU | T&G (CQ) | WO  _TMRDAR_  (NX)  _SLOX_  _LOIC_  _PIC_  _PPPIC_  WO(AFU)IC | 13.16 | 12.46 | 14.59 | slow 100% |
| 20260914-213312 | Justice | F-14BU | T&G (CQ) | none | 13.07 | 11.70 | 14.29 | slow 100% |
| 20260914-213457 | Ghost-72 - TT | F-14BU | T&G (CQ) | none | 10.14 | 9.02 | 10.77 | fast 12%, slightly_fast 26%, on_speed 53%, slightly_slow 9% |
| 20260914-213551 | Justice | F-14BU | T&G (CQ) | none | 12.56 | 11.95 | 14.07 | slow 100% |
| 20260914-213801 | Ghost-72 - TT | F-14BU | Arrested — DCS/LQM wire 1; Rust estimate unavailable | C : _LULIM_  _LOIC_  _LOAR_  WIRE# 1 _EGIW_ | 9.41 | 1.42 | 10.33 | fast 41%, slightly_fast 23%, on_speed 36% |
| 20260914-213841 | Justice | F-14BU | T&G (CQ) | none | 10.41 | 4.60 | 11.19 | slightly_fast 6%, on_speed 70%, slightly_slow 18%, slow 6% |
| 20260914-214116 | Justice | F-14BU | Arrested — DCS/LQM wire 2; Rust estimate unavailable | C : _SLOX_  _TMRDAR_  _DRX_  (LURIM)  _DRIM_  WIRE# 2 _EGIW_ | 10.28 | 7.79 | 11.28 | fast 7%, slightly_fast 14%, on_speed 62%, slightly_slow 7%, slow 10% |

## Per type: indexer lamp thresholds and the on-speed band

### T-45

- LSO minus true AoA over all matched groove samples: n=2358 median=0.02 mean=0.04 p10=-0.32 p90=0.44
- True AoA while the indexer showed each state (groove samples of all passes):
  - fast: n=1984 median=6.11 mean=6.26 p10=4.72 p90=7.80
  - slightly_fast: n=165 median=8.12 mean=8.11 p10=8.01 p90=8.24
  - on_speed: n=240 median=8.48 mean=8.49 p10=8.28 p90=8.72
  - slightly_slow: n=180 median=8.87 mean=8.87 p10=8.79 p90=8.96
  - slow: n=1238 median=9.69 mean=9.84 p10=9.16 p90=10.76
  - off: n=318 median=-1.66 mean=-1.66 p10=-2.24 p90=-0.94
- Lamp switch thresholds measured over the whole flight (true AoA at the switch):
  - fast_max: 8.00 deg from 80 switches (towards slow 8.02 (n=41), towards fast 7.98 (n=39))
  - slightly_fast_max: 8.25 deg from 88 switches (towards slow 8.27 (n=45), towards fast 8.24 (n=43))
  - on_speed_max: 8.75 deg from 72 switches (towards slow 8.77 (n=37), towards fast 8.73 (n=35))
  - slightly_slow_max: 9.00 deg from 76 switches (towards slow 9.02 (n=39), towards fast 8.98 (n=37))
  - other state changes seen: fast->on_speed n=2 median 8.36, on_speed->slow n=2 median 9.17, slow->slightly_fast n=1 median 8.21
- Band in src/data.rs: fast <= 8.0, slightly fast <= 8.25, on speed < 8.75, slightly slow < 9.0, slow >= 9.0
- Measured band (rounded to 0.1 deg): fast <= 8.0, slightly fast <= 8.3, on speed < 8.8, slightly slow < 9.0, slow >= 9.0

### F-14BU

- LSO minus true AoA over all matched groove samples: n=5265 median=0.07 mean=0.08 p10=-0.43 p90=0.56
- True AoA while the indexer showed each state (groove samples of all passes):
  - fast: n=1793 median=6.80 mean=6.55 p10=3.47 p90=9.12
  - slightly_fast: n=310 median=9.73 mean=9.73 p10=9.54 p90=9.94
  - on_speed: n=705 median=10.32 mean=10.34 p10=9.98 p90=10.69
  - slightly_slow: n=104 median=11.02 mean=11.03 p10=10.77 p90=11.26
  - slow: n=754 median=12.88 mean=13.07 p10=12.13 p90=14.33
  - off: n=3113 median=8.84 mean=8.20 p10=1.02 p90=13.10
- Lamp switch thresholds measured over the whole flight (true AoA at the switch):
  - fast_max: 9.47 deg from 74 switches (towards slow 9.55 (n=41), towards fast 9.38 (n=33))
  - slightly_fast_max: 9.93 deg from 83 switches (towards slow 10.01 (n=47), towards fast 9.85 (n=36))
  - on_speed_max: 10.82 deg from 62 switches (towards slow 10.89 (n=32), towards fast 10.70 (n=30))
  - slightly_slow_max: 11.27 deg from 30 switches (towards slow 11.41 (n=14), towards fast 11.19 (n=16))
  - other state changes seen: on_speed->fast n=2 median 8.97, on_speed->slow n=1 median 11.47
- Band in src/data.rs: fast <= 9.45, slightly fast <= 9.95, on speed < 10.8, slightly slow < 11.25, slow >= 11.25
- Measured band (rounded to 0.1 deg): fast <= 9.5, slightly fast <= 9.9, on speed < 10.8, slightly slow < 11.3, slow >= 11.3

## Per pass: how each band rates the LSO's own groove samples

| report | pilot | type | indexer (cockpit) | code band | measured band |
|---|---|---|---|---|---|
| 20260914-203356 | Ghost-72 - TT | T-45 | fast 2%, slightly_fast 0%, on_speed 4%, slightly_slow 9%, slow 85% | fast 3%, slightly_fast 0%, on_speed 4%, slightly_slow 14%, slow 79% | fast 3%, slightly_fast 0%, on_speed 5%, slightly_slow 12%, slow 79% |
| 20260914-203718 | Ghost-72 - TT | T-45 | fast 8%, slightly_fast 3%, on_speed 7%, slightly_slow 12%, slow 69% | fast 13%, slightly_fast 2%, on_speed 7%, slightly_slow 9%, slow 69% | fast 13%, slightly_fast 3%, on_speed 8%, slightly_slow 7%, slow 69% |
| 20260914-203943 | Ghost-72 - TT | T-45 | fast 86%, slightly_fast 2%, on_speed 9%, slightly_slow 1%, slow 2% | fast 87%, slightly_fast 3%, on_speed 5%, slightly_slow 4%, slow 1% | fast 87%, slightly_fast 3%, on_speed 5%, slightly_slow 4%, slow 1% |
| 20260914-204144 | Ghost-72 - TT | T-45 | fast 100% | fast 100% | fast 100% |
| 20260914-204928 | Ghost-72 - TT | T-45 | fast 57%, slightly_fast 20%, on_speed 16%, slightly_slow 1%, slow 6% | fast 66%, slightly_fast 14%, on_speed 13%, slightly_slow 2%, slow 4% | fast 66%, slightly_fast 16%, on_speed 12%, slightly_slow 1%, slow 4% |
| 20260914-205230 | Ghost-72 - TT | T-45 | fast 86%, slightly_fast 9%, on_speed 5% | fast 81%, slightly_fast 18%, slightly_slow 0% | fast 81%, slightly_fast 18%, slightly_slow 0% |
| 20260914-210054 | Ghost-72 - TT | F-14BU | fast 80%, slightly_fast 8%, on_speed 12% | fast 84%, slightly_fast 7%, on_speed 9% | fast 85%, slightly_fast 5%, on_speed 10% |
| 20260914-210407 | Ghost-72 - TT | F-14BU | fast 100% | fast 100% | fast 100% |
| 20260914-212150 | Justice | F-14BU | fast 100% | fast 100% | fast 100% |
| 20260914-212408 | Ghost-72 - TT | F-14BU | fast 72%, slightly_fast 6%, on_speed 7%, slightly_slow 4%, slow 10% | fast 72%, slightly_fast 9%, on_speed 3%, slightly_slow 2%, slow 14% | fast 73%, slightly_fast 7%, on_speed 4%, slightly_slow 2%, slow 14% |
| 20260914-212520 | Justice | F-14BU | fast 100% | fast 100% | fast 100% |
| 20260914-212638 | Ghost-72 - TT | F-14BU | fast 78%, slightly_fast 11%, on_speed 7%, slightly_slow 3% | fast 80%, slightly_fast 10%, on_speed 5%, slightly_slow 1%, slow 4% | fast 82%, slightly_fast 8%, on_speed 5%, slightly_slow 2%, slow 3% |
| 20260914-212753 | Justice | F-14BU | fast 100% | fast 100% | fast 100% |
| 20260914-213030 | Justice | F-14BU | slow 100% | on_speed 0%, slightly_slow 0%, slow 99% | on_speed 0%, slightly_slow 1%, slow 99% |
| 20260914-213312 | Justice | F-14BU | slow 100% | fast 2%, slightly_fast 0%, on_speed 1%, slightly_slow 0%, slow 97% | fast 2%, slightly_fast 0%, on_speed 1%, slightly_slow 1%, slow 96% |
| 20260914-213457 | Ghost-72 - TT | F-14BU | fast 12%, slightly_fast 26%, on_speed 53%, slightly_slow 9% | fast 18%, slightly_fast 24%, on_speed 42%, slightly_slow 15%, slow 1% | fast 20%, slightly_fast 21%, on_speed 43%, slightly_slow 15%, slow 1% |
| 20260914-213551 | Justice | F-14BU | slow 100% | fast 2%, slightly_fast 0%, on_speed 0%, slightly_slow 0%, slow 98% | fast 2%, slightly_fast 0%, on_speed 0%, slightly_slow 0%, slow 98% |
| 20260914-213801 | Ghost-72 - TT | F-14BU | fast 41%, slightly_fast 23%, on_speed 36% | fast 44%, slightly_fast 19%, on_speed 34%, slightly_slow 3% | fast 44%, slightly_fast 17%, on_speed 36%, slightly_slow 3% |
| 20260914-213841 | Justice | F-14BU | slightly_fast 6%, on_speed 70%, slightly_slow 18%, slow 6% | fast 4%, slightly_fast 11%, on_speed 55%, slightly_slow 14%, slow 16% | fast 4%, slightly_fast 9%, on_speed 57%, slightly_slow 17%, slow 14% |
| 20260914-214116 | Justice | F-14BU | fast 7%, slightly_fast 14%, on_speed 62%, slightly_slow 7%, slow 10% | fast 12%, slightly_fast 15%, on_speed 47%, slightly_slow 11%, slow 15% | fast 13%, slightly_fast 12%, on_speed 49%, slightly_slow 13%, slow 13% |

## Per type: agreement between the cockpit indexer and the LSO rating (groove samples)

### T-45

**code band, LSO AoA raw**: exact agreement 85%, within one step 97%

| cockpit shows \ LSO rates | fast | slightly_fast | on_speed | slightly_slow | slow |
|---|---|---|---|---|---|
| fast (n=1209) | 97% | 2% | 1% | 0% | 0% |
| slightly_fast (n=135) | 36% | 44% | 19% | 1% | 0% |
| on_speed (n=169) | 4% | 18% | 27% | 38% | 13% |
| slightly_slow (n=104) | 1% | 1% | 23% | 38% | 38% |
| slow (n=707) | 3% | 0% | 0% | 3% | 93% |

**code band, LSO AoA smoothed over 0.5 s**: exact agreement 85%, within one step 97%

| cockpit shows \ LSO rates | fast | slightly_fast | on_speed | slightly_slow | slow |
|---|---|---|---|---|---|
| fast (n=1209) | 97% | 2% | 1% | 0% | 0% |
| slightly_fast (n=135) | 39% | 42% | 19% | 0% | 0% |
| on_speed (n=169) | 4% | 18% | 29% | 39% | 9% |
| slightly_slow (n=104) | 1% | 1% | 22% | 44% | 32% |
| slow (n=707) | 3% | 0% | 1% | 3% | 93% |

**measured band, LSO AoA raw**: exact agreement 85%, within one step 97%

| cockpit shows \ LSO rates | fast | slightly_fast | on_speed | slightly_slow | slow |
|---|---|---|---|---|---|
| fast (n=1209) | 97% | 2% | 1% | 0% | 0% |
| slightly_fast (n=135) | 36% | 45% | 18% | 1% | 0% |
| on_speed (n=169) | 4% | 22% | 30% | 31% | 13% |
| slightly_slow (n=104) | 1% | 1% | 28% | 33% | 38% |
| slow (n=707) | 3% | 0% | 0% | 3% | 93% |

**measured band, LSO AoA smoothed over 0.5 s**: exact agreement 85%, within one step 97%

| cockpit shows \ LSO rates | fast | slightly_fast | on_speed | slightly_slow | slow |
|---|---|---|---|---|---|
| fast (n=1209) | 97% | 2% | 1% | 0% | 0% |
| slightly_fast (n=135) | 39% | 44% | 17% | 0% | 0% |
| on_speed (n=169) | 4% | 21% | 33% | 33% | 9% |
| slightly_slow (n=104) | 1% | 1% | 31% | 36% | 32% |
| slow (n=707) | 3% | 0% | 1% | 3% | 93% |

### F-14BU

**code band, LSO AoA raw**: exact agreement 84%, within one step 98%

| cockpit shows \ LSO rates | fast | slightly_fast | on_speed | slightly_slow | slow |
|---|---|---|---|---|---|
| fast (n=1313) | 97% | 2% | 1% | 0% | 0% |
| slightly_fast (n=237) | 21% | 37% | 42% | 0% | 0% |
| on_speed (n=559) | 1% | 17% | 64% | 15% | 3% |
| slightly_slow (n=82) | 0% | 5% | 26% | 17% | 52% |
| slow (n=788) | 0% | 0% | 1% | 1% | 98% |

**code band, LSO AoA smoothed over 0.5 s**: exact agreement 84%, within one step 99%

| cockpit shows \ LSO rates | fast | slightly_fast | on_speed | slightly_slow | slow |
|---|---|---|---|---|---|
| fast (n=1313) | 97% | 3% | 1% | 0% | 0% |
| slightly_fast (n=237) | 21% | 37% | 43% | 0% | 0% |
| on_speed (n=559) | 1% | 16% | 67% | 14% | 2% |
| slightly_slow (n=82) | 0% | 5% | 26% | 12% | 57% |
| slow (n=788) | 0% | 0% | 1% | 1% | 98% |

**measured band, LSO AoA raw**: exact agreement 84%, within one step 99%

| cockpit shows \ LSO rates | fast | slightly_fast | on_speed | slightly_slow | slow |
|---|---|---|---|---|---|
| fast (n=1313) | 97% | 2% | 1% | 0% | 0% |
| slightly_fast (n=237) | 24% | 32% | 44% | 0% | 0% |
| on_speed (n=559) | 2% | 14% | 65% | 17% | 1% |
| slightly_slow (n=82) | 0% | 5% | 26% | 20% | 50% |
| slow (n=788) | 0% | 0% | 1% | 1% | 98% |

**measured band, LSO AoA smoothed over 0.5 s**: exact agreement 85%, within one step 99%

| cockpit shows \ LSO rates | fast | slightly_fast | on_speed | slightly_slow | slow |
|---|---|---|---|---|---|
| fast (n=1313) | 97% | 2% | 1% | 0% | 0% |
| slightly_fast (n=237) | 22% | 32% | 46% | 0% | 0% |
| on_speed (n=559) | 2% | 14% | 68% | 15% | 1% |
| slightly_slow (n=82) | 0% | 4% | 27% | 20% | 50% |
| slow (n=788) | 0% | 0% | 1% | 1% | 98% |

