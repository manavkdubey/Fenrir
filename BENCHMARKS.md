# Performance methodology and baseline

Measured on **2026-11-04**, using the observability milestone's source. Git commit
metadata dates do not describe the measurement date.

## Reproduce

```sh
cargo build --release --locked
target/release/space_shooter --benchmark --seed 42 --ticks 600 --warmup 120 --repeats 5
```

Use the same source, lockfile, compiler, seed, warmup, and measured tick count when
comparing changes. Each repetition creates a fresh headless Bevy app, warms it for
120 fixed ticks, and measures 600 more ticks at a simulated 60 Hz. It runs as fast
as the CPU permits. The five measured repetitions provide 3,000 samples.

The fixture embeds the repository's asteroid PNG at build time and uses the game's
movement, gravity, collision, damage, splitting, and explosion systems. Before each
tick, it replenishes bodies to 96 and bullets to 128. Every eighth new body has a
pre-cut channel to exercise splitting. Movement, aiming, and casts are scripted
from seed 42. Automatic spawning is disabled and the player gets `u32::MAX` health,
so density and run duration stay controlled. Projectile sprites and transforms use
the gameplay component layout. There is no renderer, input-device sampling, or GPU
work. Startup and image decoding precede warmup.

## Timing boundaries and counters

- **Tick wall:** fixture refill/script plus `App::update`, excluding report collection.
- **Simulation span:** wall time between the first and last fixed-gameplay probes;
  includes ECS scheduling, deferred commands, and the instrumented gameplay passes.
- **Bullet pass:** the projectile/asteroid collision system, including damage work.
- **Explosion pass:** queue maintenance and ready-blast processing, including damage.

Probes are enabled in every reported run. `Instant` reads, counters, and bounded
sample storage have overhead; an uninstrumented comparison has not been measured.
Percentiles use nearest rank: sorted sample at `ceil(p * n) - 1`. The mean is the
arithmetic mean. Timings include OS scheduling and can vary with desktop load.

Pairs count attempted bullet/body or live blast/body checks. Narrow candidates
count projectile rays admitted by the bounding circle. Projectile cell tests count
occupied-cell box intersections; blast cell tests count occupied-cell distance
tests. They do not count every grid traversal or centroid operation. Work totals
also include processed blasts, accepted secondary triggers, bodies partitioned,
and new fragments. Population peaks are sampled **after** each measured tick.

Each repetition must match a sorted gameplay snapshot digest, work totals, and
post-tick peaks. The digest covers body poses/material/velocity, projectile state,
player state, score, gravity phase, and queued-blast count. This is a regression
signal for the same build/platform/seed. RSS is a Linux process-resident snapshot
after each repetition, including retained allocator and engine memory; it is not
peak memory or the size of game entities.

## Recorded environment

- CPU: AMD Ryzen 7 4800H with Radeon Graphics.
- OS/architecture: Linux x86_64; 16 available logical CPUs reported by Rust.
- Compiler: `rustc 1.99.1 (58cf8f9c3 2026-10-13)`.
- Bevy 0.20.1, rand 0.10.3; stock Cargo release profile and Bevy worker configuration.
- Seed 42; 120 warmup ticks, 600 measured ticks, 5 repetitions.
- Shared desktop; CPU affinity/frequency and background load were not controlled.

## Recorded results

Simulation span, milliseconds:

| Repeat | Mean | p50 | p95 | p99 |
| --- | ---: | ---: | ---: | ---: |
| 1 | 0.585256 | 0.633832 | 1.100029 | 1.227120 |
| 2 | 0.793209 | 0.735481 | 1.105450 | 1.252732 |
| 3 | 0.812360 | 0.756316 | 1.111979 | 1.255352 |
| 4 | 0.685370 | 0.668688 | 1.041750 | 1.186276 |
| 5 | 0.514812 | 0.423324 | 1.013649 | 1.125588 |

Other measured stages, milliseconds, and post-repetition resident snapshots:

| Repeat | Tick wall mean | Bullet pass mean | Explosion pass mean | RSS KiB |
| --- | ---: | ---: | ---: | ---: |
| 1 | 0.794218 | 0.328515 | 0.000474 | 57884 |
| 2 | 1.065947 | 0.457389 | 0.000860 | 60944 |
| 3 | 1.092300 | 0.466077 | 0.000825 | 61100 |
| 4 | 0.886000 | 0.414342 | 0.000685 | 61112 |
| 5 | 0.694099 | 0.291917 | 0.000778 | 61124 |

Aggregate simulation report:

```text
aggregate_sim_span_ms: n=3000 mean=0.678202 p50=0.694361 p95=1.069143 p99=1.224624
```

All repetitions matched digest `8580ec8f7b8abd39` and these work totals:

```text
work=WorkCounts { bullet_pairs: 7410432, narrow_candidates: 26344, projectile_cell_tests: 2041765, blast_pairs: 864, blast_cell_tests: 4135, blasts: 9, secondary_triggers: 8, split_bodies: 479, new_fragments: 29 }
```

Post-tick peaks matched: 99 bodies, 19 fragments, 128 bullets, and 100 CPU images.
This is a dense fixture baseline, not a measured upper bound for every game state.
It establishes neither rendered FPS nor GPU frame time, and there is no optimization
speedup claim. The bullet pass is the largest separately timed pass in this run;
future collision changes should be compared with this command and workload.

## Live overlay

Press **F3** in the game. It keeps at most 240 rendered-frame wall-time samples and
240 simulation-span samples, and refreshes text four times per real second. Frame
wall time comes from Bevy's real clock and includes presentation/scheduling effects;
it is not a GPU timestamp. Simulation counters describe the last completed tick,
which remains visible while paused or after death. The overlay also shows current
bodies/fragments/bullets and queued explosions. Probe collection continues while
the overlay is hidden; toggling it does not change gameplay state.
