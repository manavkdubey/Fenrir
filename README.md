# Fenrir: Ragnarök Engine

A Rust/Bevy 2D space shooter being developed into a destructible-asteroid action game.
The current foundation includes movement, aiming, shooting, homing asteroids, health,
score, pause, clean restart, localized asteroid damage, independent fragments, and a
fragment-attracting gravity field. Detonation and bounded chain reactions are planned;
they are not implemented yet.

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
Only complete projectile destruction of the original body's retained region scores
a point. Detached fragments do not award additional points.

The gravity field deploys at the mouse cursor, or 300 units along an active controller
aim stick. With no cursor or stick input it deploys 200 units ahead of the ship.
Deployment is clamped to 300 units from the ship. One field is active at a time:
two seconds of attraction, followed by eight seconds of cooldown. Activation is a
single press, buffered until the next simulation tick; presses during cooldown or
pause are discarded. Death and restart reset the field. The HUD shows its phase
and remaining seconds; an outer circle shows the 220-unit range and a shrinking
circle shows the remaining active time.

Only fragments are pulled. At distance `r < 220`, with offset vector `d` pointing
to the field centre, acceleration is `700 * d / max(r, 24) * (1 - r / 220)` in
world units/second². Force is zero at the centre and outside the range. The soft
core avoids a singularity; the existing 400 units/second fragment speed cap still
applies. Homing asteroids, bullets, and the player retain their existing movement.
Fragment lifetime continues during attraction and freezes on pause. This slice
does not detonate or apply explosion damage.

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
activation, and cleanup over 50 death/restart cycles with active debris and fields.
Actual rendering and physical input require a desktop smoke test. No performance
benchmark or FPS claim has been established.

`src/main.rs` contains the gameplay systems and integration tests. `src/asteroid.rs`
contains the material grid, connected-component partitioning, local collision
geometry, and geometry tests.
`src/gravity.rs` contains the field state machine and bounded force model.
`src/state.rs` defines the run states. The library exports these modules.

## Next milestones

The next milestones are gravity detonation, bounded chain reactions, and
reproducible performance scenarios. Those features remain subject to their design review.
The project is not yet a finished portfolio release.

## License

Code is available under the [MIT license](LICENSE). The repository contains existing
art and font assets; a complete asset provenance audit remains part of release work.
