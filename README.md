# Fenrir: Ragnarök Engine

A Rust/Bevy 2D space shooter being developed into a destructible-asteroid action game.
The current foundation includes movement, aiming, shooting, homing asteroids, health,
score, pause, and clean restart. Localized destruction and the gravity weapon are planned;
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

Bullets live for three seconds. There are hard caps of 128 bullets and 128 asteroids.
Projectile collision uses a swept segment against circles and consumes each bullet
on its nearest available hit. A target can score only once per collision pass.
Collision is still a simple O(bullets × asteroids) pass; optimization needs measured
scenarios before introducing a spatial index.

```sh
cargo fmt --check
cargo clippy --locked --all-targets -- -D warnings
cargo test --locked
cargo build --release --locked
```

Headless tests cover swept-hit edge cases, simultaneous hits, frame-rate-independent
movement, projectile expiry, entity caps, pause/resume, finite asteroid motion, and
50 death/restart cycles. Actual rendering and physical input require a desktop smoke
test. No performance benchmark or FPS claim has been established.

`src/main.rs` contains the current gameplay systems and their tests. `src/state.rs`
defines the run states and is exported by `src/lib.rs`. The implementation will be
split into focused modules when the destruction slice needs those boundaries.

## Next milestones

The next milestones are localized asteroid destruction, a gravity weapon, bounded
chain reactions, and reproducible performance scenarios. Those features remain
subject to their design review. The project is not yet a finished portfolio release.

## License

Code is available under the [MIT license](LICENSE). The repository contains existing
art and font assets; a complete asset provenance audit remains part of release work.
