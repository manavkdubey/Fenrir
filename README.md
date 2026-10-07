# Fenrir: Ragnarök Engine

A Rust/Bevy 2D space shooter being developed into a destructible-asteroid action game.
The current foundation includes movement, aiming, shooting, homing asteroids, health,
score, pause, clean restart, localized asteroid damage, independent fragments, and a
fragment-attracting gravity field with automatic detonation and bounded secondary
chain reactions.

## Run

Use Rust 1.97.1 or newer and a GPU/driver supported by Bevy 0.20 / wgpu.
On Linux, install development libraries for your window system and gamepad support
(for example, Wayland/X11 and libudev). Cargo downloads the Rust dependencies.

```sh
cargo run --release --locked
```

Run from the repository root so the `assets` directory is available.
The game uses Bevy's 2D and UI feature sets; it does not enable the 3D or audio stacks.
When launching a standalone binary, place `assets` next to it or set
`BEVY_ASSET_ROOT` to the directory containing `assets`.

## Controls

| Action | Keyboard / mouse | Gamepad |
| --- | --- | --- |
| Move | WASD or arrow keys | Right stick |
| Aim | Mouse cursor | Left stick |
| Fire | Hold left click or Space | Hold right trigger |
| Gravity field | Q | Left trigger |
| Pause / resume | Escape | Start |
| Restart after death | R or Enter | East face button |
| Performance overlay | F3 | — |

An active gamepad aiming stick takes precedence over mouse aim. One connected
controller owns the ship; additional devices do not multiply movement or fire rate.
Keyboard and controller movement combine with a maximum magnitude of one.

## Simulation and checks

Gameplay runs at 60 fixed ticks per second. Virtual-time catch-up is capped at
100 ms per render frame, limiting work after a stall to at most six ticks. This trades
wall-clock fidelity during a long stall for bounded simulation work.

A run resets its random seed on restart. This makes spawn sequences repeatable for
the same simulation/input history within this build; it does not promise identical
results across rand versions, platforms, or arbitrary ECS insertion orders.

Bullets live for three seconds. There are hard caps of 128 bullets and 128 asteroid
bodies, including fragments.
Each asteroid has a 32×32 occupancy grid and its own 4 KiB RGBA image, sampled from
the existing texture. Shots carve a radius of four cells at the impact point. Pixels
and collision use the same mask; projectiles and the player can pass through gaps
large enough for their collision shapes. Images are updated only after damage and
released when their asteroid is removed or the run ends.

Projectile collision uses a circle broad phase followed by a swept segment against
occupied cells in the asteroid's rotated local coordinates. Each bullet is consumed
on its nearest available material hit. Four-neighbour flood fill partitions damaged
material once per simulation tick. The largest region retains the original body;
other regions become independent fragments. Every retained cell belongs to exactly
one body, and the sprite poses remain unchanged at the instant of splitting.

The original body continues to home toward the player. Fragments inherit linear
and tangential velocity, receive a 35 units/second outward velocity change, and
rotate around their own material centres. Fragment speed is capped at 400 units/second.
They collide with bullets and the player; contacts between asteroid bodies are not
resolved. Homing, separation, and speed caps deliberately add or remove momentum.

A fracture creates at most eight new fragments. Detached regions smaller than four
cells or exceeding the budget are discarded. The largest region is retained even
when smaller than that threshold. At most 64 fragments may be active, sharing the
128-body asteroid cap. Fragments expire after eight simulation seconds;
re-fracturing preserves their remaining lifetime, and pause freezes that lifetime.
Only complete projectile or blast destruction of the original body's retained
region scores a point. Detached fragments do not award additional points.

The gravity field deploys at the mouse cursor, or 300 units along an active controller
aim stick. With no cursor or stick input it deploys 200 units ahead of the ship.
Deployment is clamped to 300 units from the ship. One field is active at a time:
two seconds of attraction, one automatic blast, then eight seconds of cooldown.
Activation is a single press, buffered until the next simulation tick; presses during cooldown or
pause are discarded. Death and restart reset the field. The HUD shows its phase
and remaining seconds; an outer circle shows the 220-unit range and a shrinking
circle shows the remaining active time.

Only fragments are pulled. At distance `r < 220`, with offset vector `d` pointing
to the field centre, acceleration is `700 * d / max(r, 24) * (1 - r / 220)` in
world units/second². Force is zero at the centre and outside the range. The soft
core avoids a singularity; the existing 400 units/second fragment speed cap still
applies. Homing asteroids, bullets, and the player retain their existing movement.
Fragment lifetime continues during attraction and freezes on pause.

Detonation erases occupied cell centres within 120 world units, including material
on bodies whose origin is outside that radius. A rotated body's grid uses the same
world-space circle. Surviving fragments get an outward velocity change of
`250 * d / max(r, 24) * (1 - r / 120)` for `r < 120`, with the existing speed cap.
Intact bodies keep their homing movement. The player and bullets receive no blast
damage or force.

A gravity detonation starts one explosion chain. A body destroyed by a blast can
schedule a secondary explosion if it had at least 16 occupied cells immediately
before that fatal hit. Projectile kills, expired fragments, and smaller debris do
not trigger explosions. Original bodies score once; detached fragments do not
award extra points. Victims are processed in stable entity order when trigger
slots are limited. Empty material marks a body as dead immediately, preventing
duplicate scoring or triggers while entity despawns are deferred.

Secondary explosions have a 60-unit radius and a lower outward velocity scale of
150 units/second. They wait 80 simulation milliseconds before becoming eligible.
Each chain reserves a maximum of 12 total blasts, including the primary, and stops
after three secondary generations. Across all chains, at most two blasts are
processed per fixed tick. At most four chains with queued work and 48 queued blasts
may be retained;
additional starts or triggers are discarded. Budgets are reserved when events are
queued and are not replenished when events execute.

Projectile hits resolve first, then ready blasts, then splitting, then player
contacts. Blast-created fragments use the existing spawn limits. Primary orange
and secondary red pulses last 0.35 simulation seconds and only present results;
their expanding rings do not repeat damage. Visuals have a separate 32-pulse cap,
which drops the oldest pulse when full. Pause freezes propagation and effects;
death/restart clear queued blasts, chain records, and visual state.

Collision still checks bullet/asteroid pairs, with up to 1,024 cell tests for each
broad-phase candidate. Optimization needs measured scenarios before introducing a
spatial index. This is not continuous collision against a moving/rotating rigid body.

```sh
cargo fmt --check
cargo clippy --locked --all-targets -- -D warnings
cargo test --locked
cargo build --release --locked
```

Headless tests cover swept-hit edge cases, simultaneous hits, frame-rate-independent
movement, projectile expiry, entity caps, pause/resume, finite asteroid motion,
localized damage, rotated grid collision, image/mask agreement, seeded geometry
properties, exact component partitioning, fragment caps and expiry, rotation around
material centres, gravity bounds and cooldown, buffered keyboard/controller
activation, blast-edge damage, projectile/blast overlap, one-shot detonation,
matching detonation outcomes at different render rates, and cleanup over 50
death/restart cycles with active debris, fields, and pending blasts. Tests also
cover chain depth, total/tick/storage caps, overlapping chains, stable trigger
selection after archetype changes, and matching outcomes at 30/60/144 render FPS.
Actual rendering and physical input require a desktop smoke test.

## Performance measurement

Press **F3** for bounded frame/simulation history, collision work, entity counts,
and queued blasts. Run the seeded headless scenario with:

```sh
cargo run --release --locked -- --benchmark --seed 42 --ticks 600 --warmup 120 --repeats 5
```

The runner checks matching outcome snapshots and work counts across repetitions
and reports mean/p50/p95/p99 timings. See [BENCHMARKS.md](BENCHMARKS.md) for the
fixture, clock boundaries, machine/compiler details, and recorded baseline.
Headless timings measure simulation work; rendered frame and GPU timings require
separate measurement. No rendered FPS or optimization speedup is claimed.

`src/main.rs` contains the gameplay systems and integration tests. `src/asteroid.rs`
contains the material grid, connected-component partitioning, local collision
geometry, and geometry tests.
`src/gravity.rs` contains the field state machine, single-use primary detonation
request, and bounded attraction model. `src/explosion.rs` owns the shared blast
queue, chain budgets, pulse lifetimes, and outward velocity model.
`src/observability.rs` owns bounded samples, counters, and percentile calculation;
`src/scenario.rs` drives the headless benchmark through the game systems.
`src/state.rs` defines the run states.

## Next milestones

The next milestones are measured collision improvements and visual/gameplay
polish. Those features remain subject to their design review.
The project is not yet a finished portfolio release.

## License

Code is available under the [MIT license](LICENSE). The repository contains existing
art and font assets; a complete asset provenance audit remains part of release work.
