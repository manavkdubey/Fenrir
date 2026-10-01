use std::{collections::HashSet, time::Duration};

use bevy::{input::InputSystems, prelude::*, window::PrimaryWindow};
use rand::{RngExt, SeedableRng, rngs::StdRng};
use space_shooter::state::GameState;

const PLAYER_SPEED: f32 = 240.0;
const BULLET_SPEED: f32 = 500.0;
const HIT_RADIUS: f32 = 30.0;
const MAX_BULLETS: usize = 128;
const MAX_ASTEROIDS: usize = 128;
const RUN_SEED: u64 = 0xF3_2026;

#[derive(Component)]
struct Player;

#[derive(Component)]
struct Asteroid {
    angular_velocity: f32,
}

#[derive(Component)]
struct Bullet {
    velocity: Vec2,
    previous: Vec2,
    remaining: f32,
}

#[derive(Component)]
struct Health(u32);

#[derive(Component)]
struct GameEntity;

#[derive(Component)]
struct Hud;

#[derive(Resource)]
struct GameAssets {
    player: Handle<Image>,
    asteroid: Handle<Image>,
    bullet: Handle<Image>,
}

impl FromWorld for GameAssets {
    fn from_world(world: &mut World) -> Self {
        let assets = world.resource::<AssetServer>();
        Self {
            player: assets.load("fighter.png"),
            asteroid: assets.load("asteroid.png"),
            bullet: assets.load("bullet.png"),
        }
    }
}

#[derive(Resource, Default)]
struct Controls {
    movement: Vec2,
    aim: Option<Vec2>,
    cursor: Option<Vec2>,
    firing: bool,
}

#[derive(Resource)]
struct Arena {
    min: Vec2,
    max: Vec2,
}

impl Default for Arena {
    fn default() -> Self {
        Self {
            min: Vec2::new(-604.0, -324.0),
            max: Vec2::new(604.0, 324.0),
        }
    }
}

#[derive(Resource)]
struct Run {
    spawn: Timer,
    fire: Timer,
    score: u32,
    rng: StdRng,
}

impl Default for Run {
    fn default() -> Self {
        Self {
            spawn: Timer::from_seconds(1.0, TimerMode::Repeating),
            fire: Timer::from_seconds(0.15, TimerMode::Once),
            score: 0,
            rng: StdRng::seed_from_u64(RUN_SEED),
        }
    }
}

fn main() {
    let mut app = App::new();
    app.add_plugins(DefaultPlugins.set(WindowPlugin {
        primary_window: Some(Window {
            resolution: (1280, 720).into(),
            title: "Fenrir: Ragnarök Engine".into(),
            present_mode: bevy::window::PresentMode::AutoVsync,
            ..default()
        }),
        ..default()
    }))
    .insert_resource(ClearColor(Color::srgb(0.015, 0.02, 0.04)))
    .add_systems(Startup, setup_view)
    .add_systems(PreUpdate, sample_controls.after(InputSystems))
    .add_systems(Update, (handle_state_input, update_hud));
    configure_gameplay(&mut app);
    app.run();
}

fn configure_gameplay(app: &mut App) {
    let mut virtual_time = Time::<Virtual>::default();
    // At 60 Hz this limits catch-up to six ticks after a stalled render frame.
    virtual_time.set_max_delta(Duration::from_millis(100));
    app.insert_resource(virtual_time)
        .insert_resource(Time::<Fixed>::from_hz(60.0))
        .init_resource::<Controls>()
        .init_resource::<Arena>()
        .init_resource::<Run>()
        .init_resource::<GameAssets>()
        .insert_state(GameState::Playing)
        .add_systems(OnEnter(GameState::Playing), setup_run)
        .add_systems(OnEnter(GameState::GameOver), cleanup_run)
        .add_systems(
            FixedUpdate,
            (
                move_player,
                shoot,
                move_bullets,
                spawn_asteroids,
                move_asteroids,
                bullet_collisions,
                player_collisions,
                expire_bullets,
            )
                .chain()
                .run_if(in_state(GameState::Playing))
                .run_if(player_alive),
        );
}

fn player_alive(players: Query<&Health, With<Player>>) -> bool {
    players.single().is_ok_and(|health| health.0 > 0)
}

fn setup_view(mut commands: Commands, asset_server: Res<AssetServer>) {
    commands.spawn(Camera2d);
    commands.spawn((
        Text::default(),
        TextFont {
            font: asset_server.load("fonts/FiraSans-Bold.ttf").into(),
            font_size: 22.0.into(),
            ..default()
        },
        Node {
            position_type: PositionType::Absolute,
            top: px(16),
            left: px(20),
            ..default()
        },
        Hud,
    ));
}

fn setup_run(
    mut commands: Commands,
    assets: Res<GameAssets>,
    players: Query<Entity, With<Player>>,
    mut run: ResMut<Run>,
) {
    // Resuming from pause keeps the existing world and timers.
    if !players.is_empty() {
        return;
    }
    *run = Run::default();
    commands.spawn((
        Player,
        Health(4),
        GameEntity,
        Name::new("Player"),
        Sprite::from_image(assets.player.clone()),
        Transform::from_xyz(0.0, 0.0, 1.0).with_scale(Vec3::splat(0.07)),
    ));
}

fn cleanup_run(mut commands: Commands, entities: Query<Entity, With<GameEntity>>) {
    for entity in &entities {
        commands.entity(entity).despawn();
    }
}

fn sample_controls(
    keys: Res<ButtonInput<KeyCode>>,
    mouse: Res<ButtonInput<MouseButton>>,
    gamepads: Query<(Entity, &Gamepad)>,
    windows: Query<&Window, With<PrimaryWindow>>,
    cameras: Query<(&Camera, &GlobalTransform), With<Camera2d>>,
    mut controls: ResMut<Controls>,
    mut arena: ResMut<Arena>,
) {
    *controls = Controls::default();
    let Ok(window) = windows.single() else { return };
    if !window.focused {
        return;
    }
    let axis = |positive: [KeyCode; 2], negative: [KeyCode; 2]| {
        f32::from(keys.any_pressed(positive)) - f32::from(keys.any_pressed(negative))
    };
    controls.movement = Vec2::new(
        axis(
            [KeyCode::KeyD, KeyCode::ArrowRight],
            [KeyCode::KeyA, KeyCode::ArrowLeft],
        ),
        axis(
            [KeyCode::KeyW, KeyCode::ArrowUp],
            [KeyCode::KeyS, KeyCode::ArrowDown],
        ),
    );
    controls.firing = mouse.pressed(MouseButton::Left) || keys.pressed(KeyCode::Space);
    // One controller owns the ship; extra devices cannot multiply speed or timer ticks.
    if let Some((_, gamepad)) = gamepads.iter().min_by_key(|(entity, _)| entity.to_bits()) {
        let movement = gamepad.right_stick();
        if movement.length_squared() > 0.01 {
            controls.movement += movement;
        }
        let aim = gamepad.left_stick();
        if aim.length_squared() > 0.01 {
            controls.aim = Some(aim);
        }
        controls.firing |= gamepad.pressed(GamepadButton::RightTrigger2);
    }
    controls.movement = controls.movement.clamp_length_max(1.0);
    if let Ok((camera, transform)) = cameras.single() {
        controls.cursor = window
            .cursor_position()
            .and_then(|cursor| camera.viewport_to_world_2d(transform, cursor).ok());
        if let (Ok(a), Ok(b)) = (
            camera.viewport_to_world_2d(transform, Vec2::ZERO),
            camera.viewport_to_world_2d(transform, Vec2::new(window.width(), window.height())),
        ) {
            let margin = Vec2::splat(36.0).min((b - a).abs() * 0.5);
            arena.min = a.min(b) + margin;
            arena.max = a.max(b) - margin;
        }
    }
}

fn handle_state_input(
    keys: Res<ButtonInput<KeyCode>>,
    gamepads: Query<(Entity, &Gamepad)>,
    state: Res<State<GameState>>,
    mut next: ResMut<NextState<GameState>>,
    mut time: ResMut<Time<Virtual>>,
) {
    let gamepad = gamepads
        .iter()
        .min_by_key(|(entity, _)| entity.to_bits())
        .map(|(_, pad)| pad);
    let pause = keys.just_pressed(KeyCode::Escape)
        || gamepad.is_some_and(|pad| pad.just_pressed(GamepadButton::Start));
    let restart = keys.just_pressed(KeyCode::KeyR)
        || keys.just_pressed(KeyCode::Enter)
        || gamepad.is_some_and(|pad| pad.just_pressed(GamepadButton::East));
    match state.get() {
        GameState::Playing if pause => {
            time.pause();
            next.set(GameState::Paused);
        }
        GameState::Paused if pause => {
            time.unpause();
            next.set(GameState::Playing);
        }
        GameState::GameOver if restart => {
            time.unpause();
            next.set(GameState::Playing);
        }
        _ => {}
    }
}

fn move_player(
    mut players: Query<&mut Transform, With<Player>>,
    controls: Res<Controls>,
    arena: Res<Arena>,
    time: Res<Time<Fixed>>,
) {
    for mut transform in &mut players {
        transform.translation += (controls.movement * PLAYER_SPEED * time.delta_secs()).extend(0.0);
        transform.translation = transform
            .translation
            .truncate()
            .clamp(arena.min, arena.max)
            .extend(1.0);
        let aim = controls.aim.or_else(|| {
            controls
                .cursor
                .map(|p| p - transform.translation.truncate())
        });
        if let Some(direction) = aim.filter(|d| d.length_squared() > 0.01) {
            transform.rotation = Quat::from_rotation_z(Vec2::Y.angle_to(direction));
        }
    }
}

fn shoot(
    mut commands: Commands,
    controls: Res<Controls>,
    players: Query<&Transform, With<Player>>,
    bullets: Query<Entity, With<Bullet>>,
    assets: Res<GameAssets>,
    mut run: ResMut<Run>,
    time: Res<Time<Fixed>>,
) {
    run.fire.tick(time.delta());
    if !controls.firing || !run.fire.is_finished() || bullets.iter().count() >= MAX_BULLETS {
        return;
    }
    let Ok(player) = players.single() else { return };
    run.fire.reset();
    let position = player.translation.truncate();
    commands.spawn((
        Bullet {
            velocity: player.local_y().truncate() * BULLET_SPEED,
            previous: position,
            remaining: 3.0,
        },
        GameEntity,
        Sprite::from_image(assets.bullet.clone()),
        Transform {
            translation: position.extend(0.5),
            rotation: player.rotation * Quat::from_rotation_z(std::f32::consts::FRAC_PI_2),
            scale: Vec3::splat(0.05),
        },
    ));
}

fn move_bullets(mut bullets: Query<(&mut Transform, &mut Bullet)>, time: Res<Time<Fixed>>) {
    for (mut transform, mut bullet) in &mut bullets {
        bullet.previous = transform.translation.truncate();
        transform.translation += (bullet.velocity * time.delta_secs()).extend(0.0);
        bullet.remaining -= time.delta_secs();
    }
}

fn spawn_asteroids(
    mut commands: Commands,
    players: Query<&Transform, With<Player>>,
    asteroids: Query<Entity, With<Asteroid>>,
    assets: Res<GameAssets>,
    mut run: ResMut<Run>,
    time: Res<Time<Fixed>>,
) {
    if !run.spawn.tick(time.delta()).just_finished() || asteroids.iter().count() >= MAX_ASTEROIDS {
        return;
    }
    let Ok(player) = players.single() else { return };
    let angle = run.rng.random_range(0.0..std::f32::consts::TAU);
    let position = player.translation.truncate() + Vec2::from_angle(angle) * 650.0;
    commands.spawn((
        Asteroid {
            angular_velocity: run.rng.random_range(-2.0..2.0),
        },
        GameEntity,
        Sprite::from_image(assets.asteroid.clone()),
        Transform::from_translation(position.extend(0.0)).with_scale(Vec3::splat(0.07)),
    ));
}

fn move_asteroids(
    players: Query<&Transform, (With<Player>, Without<Asteroid>)>,
    mut asteroids: Query<(&mut Transform, &Asteroid), Without<Player>>,
    time: Res<Time<Fixed>>,
) {
    let Ok(player) = players.single() else { return };
    for (mut transform, asteroid) in &mut asteroids {
        let direction = (player.translation - transform.translation)
            .truncate()
            .normalize_or_zero();
        transform.translation += (direction * 100.0 * time.delta_secs()).extend(0.0);
        transform.rotate_z(asteroid.angular_velocity * time.delta_secs());
    }
}

fn segment_circle_hit(start: Vec2, end: Vec2, center: Vec2, radius: f32) -> Option<f32> {
    let offset = start - center;
    let c = offset.length_squared() - radius * radius;
    if c <= 0.0 {
        return Some(0.0);
    }
    let step = end - start;
    let a = step.length_squared();
    if a <= f32::EPSILON {
        return None;
    }
    let b = offset.dot(step);
    let discriminant = b * b - a * c;
    if discriminant < 0.0 {
        return None;
    }
    let t = (-b - discriminant.sqrt()) / a;
    (0.0..=1.0).contains(&t).then_some(t)
}

fn bullet_collisions(
    mut commands: Commands,
    bullets: Query<(Entity, &Transform, &Bullet)>,
    asteroids: Query<(Entity, &Transform), With<Asteroid>>,
    mut run: ResMut<Run>,
) {
    let mut destroyed = HashSet::new();
    for (bullet_entity, transform, bullet) in &bullets {
        let hit = asteroids
            .iter()
            .filter(|(entity, _)| !destroyed.contains(entity))
            .filter_map(|(entity, asteroid)| {
                segment_circle_hit(
                    bullet.previous,
                    transform.translation.truncate(),
                    asteroid.translation.truncate(),
                    HIT_RADIUS,
                )
                .map(|t| (entity, t))
            })
            .min_by(|(ea, ta), (eb, tb)| {
                ta.total_cmp(tb)
                    .then_with(|| ea.to_bits().cmp(&eb.to_bits()))
            });
        if let Some((asteroid, _)) = hit {
            destroyed.insert(asteroid);
            commands.entity(asteroid).despawn();
            commands.entity(bullet_entity).despawn();
            run.score += 1;
        }
    }
}

fn player_collisions(
    mut commands: Commands,
    mut players: Query<(&Transform, &mut Health), With<Player>>,
    asteroids: Query<(Entity, &Transform), With<Asteroid>>,
    mut next: ResMut<NextState<GameState>>,
) {
    let Ok((player, mut health)) = players.single_mut() else {
        return;
    };
    if health.0 == 0 {
        return;
    }
    for (entity, asteroid) in &asteroids {
        if player
            .translation
            .truncate()
            .distance_squared(asteroid.translation.truncate())
            <= HIT_RADIUS * HIT_RADIUS
        {
            commands.entity(entity).despawn();
            health.0 = health.0.saturating_sub(1);
            if health.0 == 0 {
                next.set(GameState::GameOver);
                break;
            }
        }
    }
}

fn expire_bullets(mut commands: Commands, bullets: Query<(Entity, &Bullet)>) {
    for (entity, bullet) in &bullets {
        if bullet.remaining <= 0.0 {
            commands.entity(entity).despawn();
        }
    }
}

fn update_hud(
    mut hud: Query<&mut Text, With<Hud>>,
    players: Query<&Health, With<Player>>,
    state: Res<State<GameState>>,
    run: Res<Run>,
    mut previous: Local<Option<(u32, u32, GameState)>>,
) {
    let Ok(mut text) = hud.single_mut() else {
        return;
    };
    let hp = players.single().map_or(0, |health| health.0);
    let snapshot = (hp, run.score, *state.get());
    if *previous == Some(snapshot) {
        return;
    }
    *previous = Some(snapshot);
    let status = match state.get() {
        GameState::Playing => {
            "WASD / arrows: move  |  Mouse: aim  |  Click / Space: shoot  |  Esc: pause"
        }
        GameState::Paused => "PAUSED — Esc / controller Start to resume",
        GameState::GameOver => "GAME OVER — R / Enter / controller East to restart",
    };
    let value = format!(
        "FENRIR   |   Hull {hp}/4   |   Score {}\n{status}",
        run.score
    );
    if text.0 != value {
        text.0 = value;
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use bevy::{ecs::system::RunSystemOnce, time::TimeUpdateStrategy};

    fn game() -> App {
        let mut app = App::new();
        app.add_plugins((MinimalPlugins, bevy::state::app::StatesPlugin))
            .insert_resource(GameAssets {
                player: Handle::default(),
                asteroid: Handle::default(),
                bullet: Handle::default(),
            })
            .insert_resource(TimeUpdateStrategy::FixedTimesteps(1));
        configure_gameplay(&mut app);
        app.update();
        app
    }

    fn count<T: Component>(app: &mut App) -> usize {
        app.world_mut()
            .query_filtered::<Entity, With<T>>()
            .iter(app.world())
            .count()
    }

    fn asteroid(app: &mut App, position: Vec2) -> Entity {
        app.world_mut()
            .spawn((
                Asteroid {
                    angular_velocity: 0.0,
                },
                Transform::from_translation(position.extend(0.0)),
                GameEntity,
            ))
            .id()
    }

    fn bullet(app: &mut App, start: Vec2, end: Vec2) -> Entity {
        app.world_mut()
            .spawn((
                Bullet {
                    previous: start,
                    velocity: Vec2::ZERO,
                    remaining: 1.0,
                },
                Transform::from_translation(end.extend(0.5)),
                GameEntity,
            ))
            .id()
    }

    #[test]
    fn keyboard_diagonals_are_normalized_and_focus_loss_clears_input() {
        let mut app = game();
        app.init_resource::<ButtonInput<KeyCode>>()
            .init_resource::<ButtonInput<MouseButton>>();
        let window = app
            .world_mut()
            .spawn((Window::default(), PrimaryWindow))
            .id();
        {
            let mut keys = app.world_mut().resource_mut::<ButtonInput<KeyCode>>();
            keys.press(KeyCode::KeyW);
            keys.press(KeyCode::KeyD);
        }
        app.world_mut()
            .resource_mut::<ButtonInput<MouseButton>>()
            .press(MouseButton::Left);
        app.world_mut().run_system_once(sample_controls).unwrap();
        let input = app.world().resource::<Controls>();
        assert!((input.movement.length() - 1.0).abs() < 0.0001);
        assert!(input.movement.x > 0.0 && input.movement.y > 0.0 && input.firing);
        app.world_mut().get_mut::<Window>(window).unwrap().focused = false;
        app.world_mut().run_system_once(sample_controls).unwrap();
        let input = app.world().resource::<Controls>();
        assert_eq!(input.movement, Vec2::ZERO);
        assert!(!input.firing);
    }

    #[test]
    fn a_long_frame_runs_no_more_than_six_simulation_ticks() {
        let mut app = game();
        app.world_mut().resource_mut::<Controls>().movement = Vec2::X;
        app.insert_resource(TimeUpdateStrategy::ManualDuration(Duration::from_secs(2)));
        app.update();
        let position = app
            .world_mut()
            .query_filtered::<&Transform, With<Player>>()
            .single(app.world())
            .unwrap()
            .translation
            .x;
        assert!(position > 0.0 && position <= PLAYER_SPEED * 0.1 + 0.01);
    }

    #[test]
    fn keyboard_pause_resume_and_restart_use_the_state_machine() {
        let mut app = game();
        app.init_resource::<ButtonInput<KeyCode>>();
        app.world_mut()
            .resource_mut::<ButtonInput<KeyCode>>()
            .press(KeyCode::Escape);
        app.world_mut().run_system_once(handle_state_input).unwrap();
        app.update();
        assert_eq!(
            *app.world().resource::<State<GameState>>().get(),
            GameState::Paused
        );
        assert!(app.world().resource::<Time<Virtual>>().is_paused());
        app.world_mut().run_system_once(handle_state_input).unwrap();
        app.update();
        assert_eq!(
            *app.world().resource::<State<GameState>>().get(),
            GameState::Playing
        );
        assert!(!app.world().resource::<Time<Virtual>>().is_paused());
        app.world_mut()
            .resource_mut::<NextState<GameState>>()
            .set(GameState::GameOver);
        app.update();
        app.world_mut()
            .resource_mut::<ButtonInput<KeyCode>>()
            .clear();
        app.world_mut()
            .resource_mut::<ButtonInput<KeyCode>>()
            .press(KeyCode::KeyR);
        app.world_mut().run_system_once(handle_state_input).unwrap();
        app.update();
        assert_eq!(
            *app.world().resource::<State<GameState>>().get(),
            GameState::Playing
        );
        assert_eq!(count::<Player>(&mut app), 1);
    }

    #[test]
    fn swept_collision_handles_edges_overlap_and_zero_motion() {
        assert_eq!(
            segment_circle_hit(Vec2::ZERO, Vec2::ZERO, Vec2::ZERO, 1.0),
            Some(0.0)
        );
        assert_eq!(
            segment_circle_hit(Vec2::ZERO, Vec2::ZERO, Vec2::X * 2.0, 1.0),
            None
        );
        assert_eq!(
            segment_circle_hit(Vec2::new(-2.0, 1.0), Vec2::new(2.0, 1.0), Vec2::ZERO, 1.0),
            Some(0.5)
        );
        assert_eq!(
            segment_circle_hit(
                Vec2::new(-100.0, 0.0),
                Vec2::new(100.0, 0.0),
                Vec2::ZERO,
                1.0
            ),
            Some(0.495)
        );
        assert_eq!(
            segment_circle_hit(Vec2::new(-2.0, 1.1), Vec2::new(2.0, 1.1), Vec2::ZERO, 1.0),
            None
        );
    }

    #[test]
    fn one_projectile_hits_only_the_nearest_target() {
        let mut app = game();
        let farther = asteroid(&mut app, Vec2::new(150.0, 0.0));
        let nearer = asteroid(&mut app, Vec2::new(75.0, 0.0));
        let shot = bullet(&mut app, Vec2::ZERO, Vec2::new(200.0, 0.0));
        app.world_mut().run_system_once(bullet_collisions).unwrap();
        assert!(app.world().get_entity(farther).is_ok());
        assert!(app.world().get_entity(nearer).is_err());
        assert!(app.world().get_entity(shot).is_err());
        assert_eq!(app.world().resource::<Run>().score, 1);
    }

    #[test]
    fn simultaneous_bullets_score_a_target_once() {
        let mut app = game();
        asteroid(&mut app, Vec2::ZERO);
        bullet(&mut app, Vec2::ZERO, Vec2::ZERO);
        bullet(&mut app, Vec2::ZERO, Vec2::ZERO);
        app.world_mut().run_system_once(bullet_collisions).unwrap();
        assert_eq!(count::<Asteroid>(&mut app), 0);
        assert_eq!(count::<Bullet>(&mut app), 1);
        assert_eq!(app.world().resource::<Run>().score, 1);
    }

    #[test]
    fn fixed_movement_is_consistent_across_render_rates() {
        let positions: Vec<f32> = [30, 60, 144]
            .into_iter()
            .map(|fps| {
                let mut app = game();
                app.world_mut().resource_mut::<Controls>().movement = Vec2::X;
                app.insert_resource(TimeUpdateStrategy::ManualDuration(Duration::from_secs_f64(
                    1.0 / f64::from(fps),
                )));
                for _ in 0..fps * 2 {
                    app.update();
                }
                app.world_mut()
                    .query_filtered::<&Transform, With<Player>>()
                    .single(app.world())
                    .unwrap()
                    .translation
                    .x
            })
            .collect();
        for position in &positions {
            assert!(
                (position - PLAYER_SPEED * 2.0).abs() <= PLAYER_SPEED / 60.0 + 0.01,
                "{positions:?}"
            );
        }
    }

    #[test]
    fn fifty_death_restart_cycles_reset_entities_timers_and_rng() {
        let mut app = game();
        for _ in 0..50 {
            app.world_mut().resource_mut::<Controls>().firing = true;
            for _ in 0..65 {
                app.update();
            }
            assert!(count::<Bullet>(&mut app) > 0);
            assert!(count::<Asteroid>(&mut app) > 0);
            for _ in 0..8 {
                asteroid(&mut app, Vec2::ZERO);
            }
            app.world_mut().run_system_once(player_collisions).unwrap();
            app.update();
            assert_eq!(
                *app.world().resource::<State<GameState>>().get(),
                GameState::GameOver
            );
            assert_eq!(count::<GameEntity>(&mut app), 0);
            app.world_mut().resource_mut::<Controls>().firing = false;
            app.world_mut()
                .resource_mut::<NextState<GameState>>()
                .set(GameState::Playing);
            app.update();
            assert_eq!(count::<Player>(&mut app), 1);
            assert_eq!(count::<GameEntity>(&mut app), 1);
            assert_eq!(app.world().resource::<Run>().score, 0);
            let hp = app
                .world_mut()
                .query::<&Health>()
                .single(app.world())
                .unwrap()
                .0;
            assert_eq!(hp, 4);
            let sample = app.world_mut().resource_mut::<Run>().rng.random::<u64>();
            assert_eq!(sample, Run::default().rng.random::<u64>());
            assert!(app.world().resource::<Run>().spawn.elapsed_secs() < 0.04);
        }
    }

    #[test]
    fn pause_freezes_simulation_and_resume_keeps_player() {
        let mut app = game();
        let player = app
            .world_mut()
            .query_filtered::<Entity, With<Player>>()
            .single(app.world())
            .unwrap();
        app.world_mut().resource_mut::<Controls>().movement = Vec2::X;
        app.world_mut()
            .resource_mut::<NextState<GameState>>()
            .set(GameState::Paused);
        let elapsed = app.world().resource::<Run>().spawn.elapsed();
        for _ in 0..120 {
            app.update();
        }
        assert_eq!(
            app.world().get::<Transform>(player).unwrap().translation.x,
            0.0
        );
        assert_eq!(app.world().resource::<Run>().spawn.elapsed(), elapsed);
        app.world_mut()
            .resource_mut::<NextState<GameState>>()
            .set(GameState::Playing);
        app.update();
        assert!(app.world().get::<Transform>(player).unwrap().translation.x > 0.0);
        assert_eq!(count::<Player>(&mut app), 1);
    }

    #[test]
    fn missed_bullets_expire_and_spawns_obey_caps() {
        let mut app = game();
        let shot = bullet(&mut app, Vec2::new(2000.0, 0.0), Vec2::new(2000.0, 0.0));
        for _ in 0..65 {
            app.update();
        }
        assert!(app.world().get_entity(shot).is_err());
        for _ in 0..MAX_BULLETS {
            bullet(&mut app, Vec2::ZERO, Vec2::ZERO);
        }
        app.world_mut().resource_mut::<Controls>().firing = true;
        app.world_mut().run_system_once(shoot).unwrap();
        assert_eq!(count::<Bullet>(&mut app), MAX_BULLETS);
        for _ in count::<Asteroid>(&mut app)..MAX_ASTEROIDS {
            asteroid(&mut app, Vec2::splat(1000.0));
        }
        app.world_mut().resource_mut::<Run>().spawn.reset();
        app.world_mut()
            .resource_mut::<Run>()
            .spawn
            .tick(Duration::from_millis(990));
        app.world_mut().run_system_once(spawn_asteroids).unwrap();
        assert_eq!(count::<Asteroid>(&mut app), MAX_ASTEROIDS);
    }

    #[test]
    fn coincident_asteroid_motion_stays_finite_and_planar() {
        let mut app = game();
        let rock = asteroid(&mut app, Vec2::ZERO);
        app.world_mut().run_system_once(move_asteroids).unwrap();
        let transform = app.world().get::<Transform>(rock).unwrap();
        assert!(transform.translation.is_finite());
        assert_eq!(transform.translation, Vec3::ZERO);
    }
}
