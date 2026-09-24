# AoA calibration result

CSV rows: 191416 (pilots: Ghost-72 | TT, Justice), reports matched: 23

## Per pass: LSO computed AoA minus flight-model true AoA (degrees, groove only)

| report | pilot | type | wind ref | clock offset | matched | median | mean | p10 | p90 | per deg of bank |
|---|---|---|---|---|---|---|---|---|---|---|
| 20260915-204159 | Ghost-72 TT | T-45 | yes | -0.60 s | 335 | -0.06 | -0.09 | -0.44 | +0.24 | -0.055 |
| 20260915-204519 | Ghost-72 TT | T-45 | yes | -0.70 s | 281 | -0.01 | +0.03 | -0.26 | +0.45 | -0.085 |
| 20260915-204736 | Ghost-72 TT | T-45 | yes (fallback) | -0.20 s | 340 | +0.02 | +0.02 | -0.15 | +0.20 | -0.008 |
| 20260915-204957 | Ghost-72 TT | T-45 | yes | -0.45 s | 292 | -0.05 | +0.05 | -0.22 | +0.61 | -0.057 |
| 20260915-205643 | Justice | F-14BU | yes (fallback) | -0.60 s | 472 | -0.02 | -0.01 | -0.42 | +0.39 | -0.024 |
| 20260915-205948 | Justice | F-14BU | yes | -0.15 s | 242 | -0.05 | -0.05 | -0.28 | +0.23 | +0.023 |
| 20260915-210053 | Ghost-72 TT | T-45 | yes (fallback) | -0.65 s | 260 | +0.06 | +0.03 | -0.63 | +0.51 | -0.107 |
| 20260915-210206 | Justice | F-14BU | yes | -0.65 s | 265 | -0.03 | -0.01 | -0.49 | +0.57 | +0.036 |
| 20260915-210346 | Ghost-72 TT | T-45 | yes | -0.50 s | 293 | +0.08 | +0.06 | -0.28 | +0.35 | -0.037 |
| 20260915-210427 | Justice | F-14BU | yes | -0.55 s | 341 | +0.17 | +0.21 | -0.41 | +0.91 | -0.074 |
| 20260915-211027 | Justice | F-14BU | yes | -0.10 s | 500 | -0.05 | -0.02 | -0.17 | +0.11 | -0.015 |
| 20260915-211558 | Justice | F-14BU | yes | -0.55 s | 422 | +0.04 | +0.07 | -0.53 | +0.65 | -0.062 |
| 20260915-211726 | Ghost-72 TT | F-14BU | yes | -0.80 s | 231 | -0.17 | -0.17 | -0.67 | +0.39 | -0.041 |
| 20260915-211813 | Justice | F-14BU | yes | -0.65 s | 343 | +0.07 | +0.04 | -0.37 | +0.39 | -0.008 |
| 20260915-212128 | Justice | F-14BU | yes (fallback) | -0.20 s | 415 | -0.02 | -0.05 | -0.30 | +0.09 | -0.007 |
| 20260915-212358 | Justice | F-14BU | yes | -0.65 s | 340 | +0.09 | +0.15 | -0.30 | +0.61 | -0.079 |
| 20260915-212652 | Ghost-72 TT | F-14BU | yes (fallback) | -0.50 s | 207 | +0.18 | +0.14 | -0.77 | +0.91 | -0.300 |
| 20260915-212947 | Ghost-72 TT | F-14BU | yes | -0.60 s | 726 | -0.10 | -0.06 | -0.58 | +0.48 | +0.016 |
| 20260915-213147 | Ghost-72 TT | F-14BU | yes | -0.10 s | 500 | +0.09 | +0.10 | -0.17 | +0.42 | +0.011 |
| 20260915-213416 | Ghost-72 TT | F-14BU | yes | -0.25 s | 350 | -0.02 | +0.07 | -0.34 | +0.53 | +0.040 |
| 20260915-213959 | Justice | F-14BU | yes | -0.60 s | 371 | +0.06 | +0.08 | -0.38 | +0.55 | -0.103 |
| 20260915-214303 | Justice | F-14BU | yes | -0.65 s | 375 | -0.01 | +0.00 | -0.44 | +0.47 | -0.012 |
| 20260915-214546 | Justice | F-14BU | yes | -0.60 s | 339 | +0.10 | +0.07 | -0.35 | +0.56 | -0.137 |

## Per pass: what was flown in the groove (flight model and cockpit indexer)

| report | pilot | type | outcome | DCS LSO | true AoA median | p10 | p90 | indexer share of groove |
|---|---|---|---|---|---|---|---|---|
| 20260915-204159 | Ghost-72 TT | T-45 | T&G (CQ) | none | 7.63 | 5.49 | 8.50 | fast 77%, slightly_fast 8%, on_speed 8%, slightly_slow 6%, slow 1% |
| 20260915-204519 | Ghost-72 TT | T-45 | Bolter | none | 7.75 | 0.05 | 8.06 | fast 79%, slightly_fast 18%, on_speed 3% |
| 20260915-204736 | Ghost-72 TT | T-45 | Waveoff/Go-around — initiator unknown | WO  _LULIM_  _LULIC_  _TMRDIC_  _LULX_  WO(AFU)IC | 7.21 | 6.48 | 8.83 | fast 80%, slightly_fast 8%, on_speed 2%, slightly_slow 1%, slow 9% |
| 20260915-204957 | Ghost-72 TT | T-45 | Arrested — wire 1 (DCS/LQM + Rust) | --- : _TMRDAR_  (EGTL)  WIRE# 1 | 7.02 | 4.41 | 7.43 | fast 98%, slightly_fast 1%, on_speed 1% |
| 20260915-205643 | Justice | F-14BU | T&G (CQ) | none | 9.51 | 8.48 | 11.18 | fast 37%, slightly_fast 25%, on_speed 21%, slightly_slow 10%, slow 8% |
| 20260915-205948 | Justice | F-14BU | Waveoff/Go-around — initiator unknown | OWO : _LULIM_  LOIM  LOIC  _DRIC_  (LURIC)  WO(AFU)IC | 7.22 | 6.86 | 9.03 | fast 92%, slightly_fast 2%, on_speed 2%, slightly_slow 5% |
| 20260915-210053 | Ghost-72 TT | T-45 | Bolter | none | 7.47 | -0.52 | 8.07 | fast 81%, slightly_fast 17%, on_speed 2% |
| 20260915-210206 | Justice | F-14BU | T&G (CQ) | none | 10.81 | 2.58 | 11.23 | on_speed 48%, slightly_slow 45%, slow 7% |
| 20260915-210346 | Ghost-72 TT | T-45 | Arrested — DCS/LQM wire 3; Rust estimate unavailable | C : _LULIM_  _TMRDIC_  _LOIC_  _PIC_  _PPPIC_  _LOAR_  3PTSIW  WIRE# 3 (EGIW) | 8.58 | 0.90 | 9.24 | fast 4%, slightly_fast 10%, on_speed 44%, slightly_slow 22%, slow 20% |
| 20260915-210427 | Justice | F-14BU | Arrested — DCS/LQM wire 2; Rust estimate unavailable | --- : _SLOX_  _TMRDIC_  LOAR  WIRE# 2 EGIW | 10.88 | 9.37 | 12.99 | fast 1%, slightly_fast 6%, on_speed 34%, slightly_slow 11%, slow 48% |
| 20260915-211027 | Justice | F-14BU | Waveoff/Go-around — initiator unknown | none | 6.30 | 4.85 | 7.32 | fast 100% |
| 20260915-211558 | Justice | F-14BU | T&G (CQ) | none | 10.26 | 6.31 | 11.80 | fast 31%, slightly_fast 8%, on_speed 16%, slightly_slow 17%, slow 28% |
| 20260915-211726 | Ghost-72 TT | F-14BU | Wire #1 (Rust estimate) | none | 8.09 | 0.35 | 9.48 | fast 86%, slightly_fast 14% |
| 20260915-211813 | Justice | F-14BU | T&G (CQ) | none | 10.57 | 3.60 | 11.43 | fast 4%, slightly_fast 10%, on_speed 58%, slightly_slow 14%, slow 15% |
| 20260915-212128 | Justice | F-14BU | Waveoff/Go-around — initiator unknown | none | 6.80 | 6.03 | 8.91 | fast 98%, slightly_fast 2% |
| 20260915-212358 | Justice | F-14BU | Arrested — wire 1 (DCS/LQM + Rust) | C : WX  _DRX_  _LURX_  _SLOX_  (LURIM)  _DRIM_  WIRE# 1 _EGIW_ | 9.67 | 5.66 | 11.11 | fast 38%, slightly_fast 19%, on_speed 32%, slightly_slow 3%, slow 9% |
| 20260915-212652 | Ghost-72 TT | F-14BU | Bolter | none | 5.14 | 0.96 | 7.71 | fast 100% |
| 20260915-212947 | Ghost-72 TT | F-14BU | Bolter | none | 5.74 | 4.50 | 7.02 | fast 100% |
| 20260915-213147 | Ghost-72 TT | F-14BU | Waveoff/Go-around — initiator unknown | WO  LULX  _FX_   _LOIC_  _PIC_  _PPPIC_  _LULIM_  _TMRDIC_  WO(AFU)IC | 7.18 | 6.16 | 9.18 | fast 93%, slightly_fast 1%, on_speed 5%, slightly_slow 1% |
| 20260915-213416 | Ghost-72 TT | F-14BU | Arrested — wire 4 (DCS/LQM + Rust) | C : 3PTSIW  WIRE# 4 (EGIW) | 5.97 | 4.15 | 9.06 | fast 91%, slightly_fast 3%, on_speed 7% |
| 20260915-213959 | Justice | F-14BU | T&G (CQ) | none | 10.48 | 8.64 | 11.05 | fast 3%, slightly_fast 6%, on_speed 74%, slightly_slow 13%, slow 3% |
| 20260915-214303 | Justice | F-14BU | T&G (CQ) | none | 10.09 | 7.73 | 10.71 | fast 6%, slightly_fast 19%, on_speed 64%, slightly_slow 10%, slow 1% |
| 20260915-214546 | Justice | F-14BU | Arrested — DCS/LQM wire 2; Rust estimate unavailable | --- : _SLOX_  _TMRDAR_  WIRE# 2 _EGIW_ | 9.87 | 7.63 | 11.14 | fast 27%, slightly_fast 20%, on_speed 39%, slightly_slow 5%, slow 9% |

## Per type: indexer lamp thresholds and the on-speed band

### T-45

- LSO minus true AoA over all matched groove samples: n=1801 median=-0.00 mean=0.01 p10=-0.32 p90=0.36
- True AoA while the indexer showed each state (groove samples of all passes):
  - fast: n=2268 median=7.33 mean=7.06 p10=6.43 p90=7.86
  - slightly_fast: n=309 median=8.08 mean=8.10 p10=8.02 p90=8.23
  - on_speed: n=288 median=8.45 mean=8.47 p10=8.28 p90=8.70
  - slightly_slow: n=143 median=8.85 mean=8.86 p10=8.78 p90=8.97
  - slow: n=179 median=9.42 mean=9.44 p10=9.11 p90=9.87
  - off: n=244 median=-1.31 mean=-1.29 p10=-2.31 p90=-0.34
- Lamp switch thresholds measured over the whole flight (true AoA at the switch):
  - fast_max: 8.00 deg from 82 switches (towards slow 8.02 (n=42), towards fast 7.99 (n=40))
  - slightly_fast_max: 8.25 deg from 113 switches (towards slow 8.27 (n=56), towards fast 8.24 (n=57))
  - on_speed_max: 8.75 deg from 116 switches (towards slow 8.77 (n=57), towards fast 8.73 (n=59))
  - slightly_slow_max: 9.00 deg from 92 switches (towards slow 9.02 (n=45), towards fast 8.99 (n=47))
  - other state changes seen: fast->on_speed n=1 median 8.38, on_speed->slow n=1 median 9.40
- Band in src/data.rs: fast <= 8.0, slightly fast <= 8.25, on speed < 8.75, slightly slow < 9.0, slow >= 9.0
- Measured band (rounded to 0.1 deg): fast <= 8.0, slightly fast <= 8.3, on speed < 8.7, slightly slow < 9.0, slow >= 9.0

### F-14BU

- LSO minus true AoA over all matched groove samples: n=6439 median=0.00 mean=0.03 p10=-0.41 p90=0.50
- True AoA while the indexer showed each state (groove samples of all passes):
  - fast: n=4463 median=6.82 mean=6.80 p10=4.94 p90=8.85
  - slightly_fast: n=339 median=9.65 mean=9.66 p10=9.46 p90=9.90
  - on_speed: n=850 median=10.39 mean=10.39 p10=10.03 p90=10.74
  - slightly_slow: n=250 median=11.03 mean=11.05 p10=10.83 p90=11.27
  - slow: n=318 median=11.89 mean=12.05 p10=11.35 p90=13.00
  - off: n=2152 median=8.61 mean=7.21 p10=0.27 p90=11.13
- Lamp switch thresholds measured over the whole flight (true AoA at the switch):
  - fast_max: 9.46 deg from 153 switches (towards slow 9.51 (n=83), towards fast 9.39 (n=70))
  - slightly_fast_max: 9.93 deg from 159 switches (towards slow 9.98 (n=84), towards fast 9.84 (n=75))
  - on_speed_max: 10.84 deg from 111 switches (towards slow 10.89 (n=59), towards fast 10.72 (n=52))
  - slightly_slow_max: 11.30 deg from 74 switches (towards slow 11.38 (n=38), towards fast 11.19 (n=36))
  - other state changes seen: on_speed->fast n=1 median 8.77, on_speed->slow n=1 median 12.03
- Band in src/data.rs: fast <= 9.45, slightly fast <= 9.95, on speed < 10.8, slightly slow < 11.25, slow >= 11.25
- Measured band (rounded to 0.1 deg): fast <= 9.5, slightly fast <= 9.9, on speed < 10.8, slightly slow < 11.3, slow >= 11.3

## Per pass: how each band rates the LSO's own groove samples

| report | pilot | type | indexer (cockpit) | code band | measured band |
|---|---|---|---|---|---|
| 20260915-204159 | Ghost-72 TT | T-45 | fast 77%, slightly_fast 8%, on_speed 8%, slightly_slow 6%, slow 1% | fast 80%, slightly_fast 7%, on_speed 7%, slightly_slow 6%, slow 1% | fast 80%, slightly_fast 8%, on_speed 5%, slightly_slow 7%, slow 1% |
| 20260915-204519 | Ghost-72 TT | T-45 | fast 79%, slightly_fast 18%, on_speed 3% | fast 71%, slightly_fast 19%, on_speed 9%, slightly_slow 0% | fast 71%, slightly_fast 21%, on_speed 8%, slightly_slow 0% |
| 20260915-204736 | Ghost-72 TT | T-45 | fast 80%, slightly_fast 8%, on_speed 2%, slightly_slow 1%, slow 9% | fast 92%, slightly_fast 8%, slightly_slow 0% | fast 92%, slightly_fast 8%, slightly_slow 0% |
| 20260915-204957 | Ghost-72 TT | T-45 | fast 98%, slightly_fast 1%, on_speed 1% | fast 97%, slightly_fast 3% | fast 97%, slightly_fast 3% |
| 20260915-205643 | Justice | F-14BU | fast 37%, slightly_fast 25%, on_speed 21%, slightly_slow 10%, slow 8% | fast 41%, slightly_fast 19%, on_speed 23%, slightly_slow 14%, slow 3% | fast 44%, slightly_fast 15%, on_speed 23%, slightly_slow 15%, slow 2% |
| 20260915-205948 | Justice | F-14BU | fast 92%, slightly_fast 2%, on_speed 2%, slightly_slow 5% | fast 83%, slightly_fast 2%, on_speed 8%, slightly_slow 7%, slow 0% | fast 83%, slightly_fast 1%, on_speed 8%, slightly_slow 7%, slow 0% |
| 20260915-210053 | Ghost-72 TT | T-45 | fast 81%, slightly_fast 17%, on_speed 2% | fast 74%, slightly_fast 13%, on_speed 12% | fast 74%, slightly_fast 18%, on_speed 8% |
| 20260915-210206 | Justice | F-14BU | on_speed 48%, slightly_slow 45%, slow 7% | fast 6%, slightly_fast 4%, on_speed 39%, slightly_slow 35%, slow 15% | fast 6%, slightly_fast 3%, on_speed 41%, slightly_slow 38%, slow 13% |
| 20260915-210346 | Ghost-72 TT | T-45 | fast 4%, slightly_fast 10%, on_speed 44%, slightly_slow 22%, slow 20% | fast 13%, slightly_fast 15%, on_speed 32%, slightly_slow 17%, slow 22% | fast 13%, slightly_fast 20%, on_speed 22%, slightly_slow 23%, slow 22% |
| 20260915-210427 | Justice | F-14BU | fast 1%, slightly_fast 6%, on_speed 34%, slightly_slow 11%, slow 48% | fast 6%, slightly_fast 4%, on_speed 27%, slightly_slow 16%, slow 47% | fast 7%, slightly_fast 4%, on_speed 27%, slightly_slow 16%, slow 46% |
| 20260915-211027 | Justice | F-14BU | fast 100% | fast 100% | fast 100% |
| 20260915-211558 | Justice | F-14BU | fast 31%, slightly_fast 8%, on_speed 16%, slightly_slow 17%, slow 28% | fast 14%, slightly_fast 16%, on_speed 15%, slightly_slow 15%, slow 40% | fast 15%, slightly_fast 14%, on_speed 16%, slightly_slow 19%, slow 37% |
| 20260915-211726 | Ghost-72 TT | F-14BU | fast 86%, slightly_fast 14% | fast 88%, slightly_fast 12%, slightly_slow 0% | fast 90%, slightly_fast 10%, slightly_slow 0% |
| 20260915-211813 | Justice | F-14BU | fast 4%, slightly_fast 10%, on_speed 58%, slightly_slow 14%, slow 15% | fast 10%, slightly_fast 10%, on_speed 45%, slightly_slow 8%, slow 27% | fast 11%, slightly_fast 7%, on_speed 46%, slightly_slow 10%, slow 25% |
| 20260915-212128 | Justice | F-14BU | fast 98%, slightly_fast 2% | fast 95%, slightly_fast 4%, on_speed 0% | fast 96%, slightly_fast 4%, on_speed 0% |
| 20260915-212358 | Justice | F-14BU | fast 38%, slightly_fast 19%, on_speed 32%, slightly_slow 3%, slow 9% | fast 35%, slightly_fast 26%, on_speed 18%, slightly_slow 9%, slow 12% | fast 41%, slightly_fast 18%, on_speed 20%, slightly_slow 10%, slow 11% |
| 20260915-212652 | Ghost-72 TT | F-14BU | fast 100% | fast 100% | fast 100% |
| 20260915-212947 | Ghost-72 TT | F-14BU | fast 100% | fast 100% | fast 100% |
| 20260915-213147 | Ghost-72 TT | F-14BU | fast 93%, slightly_fast 1%, on_speed 5%, slightly_slow 1% | fast 89%, slightly_fast 5%, on_speed 2%, slightly_slow 2%, slow 2% | fast 91%, slightly_fast 4%, on_speed 2%, slightly_slow 2%, slow 1% |
| 20260915-213416 | Ghost-72 TT | F-14BU | fast 91%, slightly_fast 3%, on_speed 7% | fast 91%, slightly_fast 2%, on_speed 4%, slightly_slow 4% | fast 91%, slightly_fast 1%, on_speed 4%, slightly_slow 4% |
| 20260915-213959 | Justice | F-14BU | fast 3%, slightly_fast 6%, on_speed 74%, slightly_slow 13%, slow 3% | fast 5%, slightly_fast 11%, on_speed 47%, slightly_slow 27%, slow 9% | fast 7%, slightly_fast 8%, on_speed 48%, slightly_slow 30%, slow 7% |
| 20260915-214303 | Justice | F-14BU | fast 6%, slightly_fast 19%, on_speed 64%, slightly_slow 10%, slow 1% | fast 13%, slightly_fast 27%, on_speed 50%, slightly_slow 7%, slow 2% | fast 15%, slightly_fast 22%, on_speed 54%, slightly_slow 8%, slow 1% |
| 20260915-214546 | Justice | F-14BU | fast 27%, slightly_fast 20%, on_speed 39%, slightly_slow 5%, slow 9% | fast 24%, slightly_fast 26%, on_speed 31%, slightly_slow 6%, slow 12% | fast 25%, slightly_fast 24%, on_speed 32%, slightly_slow 6%, slow 12% |

## Per type: agreement between the cockpit indexer and the LSO rating (groove samples)

### T-45

**code band, LSO AoA raw**: exact agreement 76%, within one step 95%

| cockpit shows \ LSO rates | fast | slightly_fast | on_speed | slightly_slow | slow |
|---|---|---|---|---|---|
| fast (n=1240) | 91% | 6% | 3% | 0% | 0% |
| slightly_fast (n=214) | 43% | 35% | 21% | 0% | 0% |
| on_speed (n=176) | 14% | 21% | 40% | 20% | 5% |
| slightly_slow (n=76) | 0% | 3% | 28% | 43% | 26% |
| slow (n=55) | 16% | 0% | 7% | 5% | 71% |

**code band, LSO AoA smoothed over 0.5 s**: exact agreement 77%, within one step 96%

| cockpit shows \ LSO rates | fast | slightly_fast | on_speed | slightly_slow | slow |
|---|---|---|---|---|---|
| fast (n=1240) | 92% | 6% | 2% | 0% | 0% |
| slightly_fast (n=214) | 43% | 34% | 23% | 0% | 0% |
| on_speed (n=176) | 14% | 18% | 49% | 17% | 3% |
| slightly_slow (n=76) | 0% | 3% | 34% | 38% | 25% |
| slow (n=55) | 15% | 2% | 11% | 7% | 65% |

**measured band, LSO AoA raw**: exact agreement 76%, within one step 96%

| cockpit shows \ LSO rates | fast | slightly_fast | on_speed | slightly_slow | slow |
|---|---|---|---|---|---|
| fast (n=1240) | 91% | 7% | 2% | 0% | 0% |
| slightly_fast (n=214) | 43% | 40% | 16% | 0% | 0% |
| on_speed (n=176) | 14% | 27% | 28% | 26% | 5% |
| slightly_slow (n=76) | 0% | 5% | 17% | 51% | 26% |
| slow (n=55) | 16% | 0% | 4% | 9% | 71% |

**measured band, LSO AoA smoothed over 0.5 s**: exact agreement 77%, within one step 96%

| cockpit shows \ LSO rates | fast | slightly_fast | on_speed | slightly_slow | slow |
|---|---|---|---|---|---|
| fast (n=1240) | 92% | 6% | 2% | 0% | 0% |
| slightly_fast (n=214) | 43% | 39% | 18% | 0% | 0% |
| on_speed (n=176) | 14% | 25% | 34% | 25% | 3% |
| slightly_slow (n=76) | 0% | 3% | 24% | 49% | 25% |
| slow (n=55) | 15% | 2% | 7% | 11% | 65% |

### F-14BU

**code band, LSO AoA raw**: exact agreement 81%, within one step 97%

| cockpit shows \ LSO rates | fast | slightly_fast | on_speed | slightly_slow | slow |
|---|---|---|---|---|---|
| fast (n=2902) | 95% | 4% | 0% | 0% | 0% |
| slightly_fast (n=372) | 29% | 56% | 15% | 0% | 0% |
| on_speed (n=899) | 4% | 11% | 60% | 20% | 5% |
| slightly_slow (n=269) | 0% | 6% | 26% | 29% | 39% |
| slow (n=343) | 0% | 0% | 4% | 14% | 81% |

**code band, LSO AoA smoothed over 0.5 s**: exact agreement 81%, within one step 98%

| cockpit shows \ LSO rates | fast | slightly_fast | on_speed | slightly_slow | slow |
|---|---|---|---|---|---|
| fast (n=2902) | 96% | 4% | 0% | 0% | 0% |
| slightly_fast (n=372) | 28% | 56% | 15% | 0% | 0% |
| on_speed (n=899) | 3% | 12% | 60% | 21% | 4% |
| slightly_slow (n=269) | 0% | 6% | 23% | 34% | 37% |
| slow (n=343) | 0% | 0% | 5% | 13% | 81% |

**measured band, LSO AoA raw**: exact agreement 81%, within one step 97%

| cockpit shows \ LSO rates | fast | slightly_fast | on_speed | slightly_slow | slow |
|---|---|---|---|---|---|
| fast (n=2902) | 96% | 3% | 0% | 0% | 0% |
| slightly_fast (n=372) | 38% | 44% | 17% | 0% | 0% |
| on_speed (n=899) | 5% | 9% | 62% | 21% | 4% |
| slightly_slow (n=269) | 0% | 4% | 27% | 36% | 32% |
| slow (n=343) | 0% | 0% | 4% | 16% | 79% |

**measured band, LSO AoA smoothed over 0.5 s**: exact agreement 82%, within one step 97%

| cockpit shows \ LSO rates | fast | slightly_fast | on_speed | slightly_slow | slow |
|---|---|---|---|---|---|
| fast (n=2902) | 96% | 3% | 0% | 0% | 0% |
| slightly_fast (n=372) | 36% | 47% | 17% | 0% | 0% |
| on_speed (n=899) | 5% | 8% | 63% | 21% | 4% |
| slightly_slow (n=269) | 0% | 4% | 25% | 38% | 33% |
| slow (n=343) | 0% | 0% | 5% | 15% | 80% |

