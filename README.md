# Fenrir

[![Crates.io](https://img.shields.io/crates/v/fenrir)](https://crates.io/crates/fenrir) [![License: MIT](https://img.shields.io/badge/License-MIT-yellow.svg)](LICENSE)


A fast-paced, Norse-inspired 2D space shooter built in Rust using the [Bevy game engine](https://bevyengine.org/) and [`bevy_state`](https://crates.io/crates/bevy_state) for state management. Pilot your ship through an asteroid field, blast incoming rocks, and prove yourself worthy of Valhalla!

---

## 🚀 Features

- **Norse Flavor**
  Every cannon blast echoes the roar of Ragnarök under the watchful gaze of Fenrir.
- **Procedural Asteroid Spawning**
  Asteroids spawn around you and home in on your position with increasing speed.
- **Gamepad-Only Controls**
  Fully mapped to your controller—no keyboard/mouse required.
- **Collision & Health**
  Asteroids collide with your ship (4 HP), trigger a Game Over when depleted.
- **Bullet Rotation**
  Bullets are rotated on load (90° CCW) via a custom startup system.
- **State Machine**
  Loading → Menu (future) → Playing → GameOver → Cleanup, courtesy of `bevy_state`.
- **Score Tracking & Restart**
  Destroy asteroids for points, then press the East button (B) to restart on Game Over.

---

## 📥 Installation

1. **Prerequisites**
   - Rust 1.70+
   - GPU with OpenGL 3.2+ support
2. **Clone & Build**
   ```bash
   git clone https://github.com/manavkdubey/Fenrir.git
   cd Fenrir
   cargo build --release
````

3. **Run**

   ```bash
   cargo run --release
   ```

---

## 📁 Project Structure

```
Fenrir/
├── assets/               # Sprites, fonts, sounds
├── src/
│   ├── main.rs           # App setup, state & system registration
│   ├── ship.rs           # Player bundle & components
│   ├── asteroid.rs       # Asteroid component
│   ├── systems/
│   │   ├── camera.rs           # camera()
│   │   ├── rotate_bullet.rs    # rotate_bullet_on_startup()
│   │   ├── input.rs            # move_player(), aim(), shoot()
│   │   ├── spawn.rs            # spawn_asteriods()
│   │   ├── movement.rs         # move_bullet(), move_asteroids()
│   │   ├── collision.rs        # bullet_hits_asteroid(), despawn_asteroid()
│   │   ├── ui.rs               # show_game_over(), cleanup_game(), restart_on_o()
│   │   └── debug.rs            # mouse_click_to_world(), gamepad_system()
│   └── state.rs          # GameState enum & commands
├── Cargo.toml
└── README.md
```

---

## 🎮 Controls

**Gamepad Only**

* **Move & Aim:** Left analog stick (X/Y)
* **Shoot:** Right trigger (GamepadButton::RightTrigger2)
* **Restart (Game Over):** East button (GamepadButton::East)
* **Quit:** Close the window

---

## 🔧 Configuration

Tweak spawn rates and speeds in `src/systems/spawn.rs`:

```rust
const ASTEROID_SPAWN_INTERVAL: f32 = 1.0;   // seconds between spawns
const ASTEROID_SPEED: f32         = 100.0; // pixels/sec
```

And fire rate in `main.rs`:

```rust
.insert_resource(FireRate(Timer::from_seconds(0.15, TimerMode::Repeating)))
```

---

## 🛠️ Development

* **Debug run:** `cargo run`
* **Release build:** `cargo run --release`
* **Format:** `cargo fmt`
* **Lint:** `cargo clippy -- -D warnings`

---

## 🗺️ Roadmap

* 🎵 Add sound effects & music
* 💥 Particle/explosion systems
* 📊 High-score persistence
* ➕ Power-ups (shields, rapid-fire)
* 🎮 Full menu & pause screen

---

## 🤝 Contributing

1. Fork the repo
2. Create a branch (`git checkout -b feature/YourFeature`)
3. Commit your changes (`git commit -m "feat: …"`)
4. Push & open a PR

---

## 📄 License

Released under the **MIT License**. See [LICENSE](LICENSE) for details.

---

*“Let none escape his jaws.”*
–– Inspired by Fenrir of Norse myth
