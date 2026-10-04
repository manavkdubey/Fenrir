use std::time::Duration;

use bevy::{
    asset::RenderAssetUsages,
    image::ImageSampler,
    input::InputSystems,
    prelude::*,
    render::render_resource::{Extent3d, TextureDimension, TextureFormat},
    window::PrimaryWindow,
};
use rand::{RngExt, SeedableRng, rngs::StdRng};
use space_shooter::{
    asteroid::{GRID_SIZE, MaterialGrid},
    gravity::{ACTIVE_DURATION, DEPLOY_RANGE, FIELD_RADIUS, GravityPhase, GravityWell},
    state::GameState,
};

const PLAYER_SPEED: f32 = 240.0;
const BULLET_SPEED: f32 = 500.0;
const ASTEROID_SCALE: f32 = 2.0;
const CRATER_RADIUS: f32 = 4.0;
const PLAYER_RADIUS: f32 = 14.0;
const MAX_BULLETS: usize = 128;
const MAX_ASTEROIDS: usize = 128;
const MAX_FRAGMENTS: usize = 64;
const MAX_NEW_FRAGMENTS: usize = 8;
const MIN_FRAGMENT_CELLS: u32 = 4;
const FRAGMENT_LIFETIME: f32 = 8.0;
const MAX_FRAGMENT_SPEED: f32 = 400.0;
const RUN_SEED: u64 = 0xF3_2026;

#[derive(Component)]
struct Player;

#[derive(Component)]
struct Asteroid {
    angular_velocity: f32,
    material: MaterialGrid,
    image: Handle<Image>,
    center: Vec2,
    velocity: Vec2,
    fragment_lifetime: Option<f32>,
    fracture_pending: bool,
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

#[derive(Clone, Copy, PartialEq, Eq)]
struct HudSnapshot {
    health: u32,
    score: u32,
    state: GameState,
    gravity_label: &'static str,
    gravity_seconds: u32,
}

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
    gravity_requested: bool,
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
    .add_systems(Update, (update_hud, draw_gravity));
    configure_gameplay(&mut app);
    configure_input(&mut app);
    app.run();
}

fn configure_input(app: &mut App) {
    app.add_systems(
        PreUpdate,
        (sample_controls, handle_state_input)
            .chain()
            .after(InputSystems),
    );
}

fn configure_gameplay(app: &mut App) {
    let mut virtual_time = Time::<Virtual>::default();
    // At 60 Hz this limits catch-up to six ticks after a stalled render frame.
    virtual_time.set_max_delta(Duration::from_millis(100));
    app.insert_resource(virtual_time)
        .insert_resource(Time::<Fixed>::from_hz(60.0))
        .init_resource::<Controls>()
        .init_resource::<GravityWell>()
        .init_resource::<Arena>()
        .init_resource::<Run>()
        .init_resource::<Assets<Image>>()
        .init_resource::<GameAssets>()
        .insert_state(GameState::Playing)
        .add_systems(OnEnter(GameState::Playing), setup_run)
        .add_systems(OnEnter(GameState::GameOver), cleanup_run)
        .add_systems(
            FixedUpdate,
            (
                move_player,
                update_gravity,
                shoot,
                move_bullets,
                spawn_asteroids,
                move_asteroids,
                expire_fragments,
                bullet_collisions,
                split_asteroids,
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
    mut gravity: ResMut<GravityWell>,
    mut controls: ResMut<Controls>,
) {
    // Resuming from pause keeps the existing world and timers.
    if !players.is_empty() {
        return;
    }
    *run = Run::default();
    *gravity = GravityWell::default();
    controls.gravity_requested = false;
    commands.spawn((
        Player,
        Health(4),
        GameEntity,
        Name::new("Player"),
        Sprite::from_image(assets.player.clone()),
        Transform::from_xyz(0.0, 0.0, 1.0).with_scale(Vec3::splat(0.07)),
    ));
}

fn cleanup_run(
    mut commands: Commands,
    entities: Query<(Entity, Option<&Asteroid>), With<GameEntity>>,
    mut images: ResMut<Assets<Image>>,
    mut gravity: ResMut<GravityWell>,
    mut controls: ResMut<Controls>,
) {
    *gravity = GravityWell::default();
    controls.gravity_requested = false;
    for (entity, asteroid) in &entities {
        if let Some(asteroid) = asteroid {
            images.remove(asteroid.image.id());
        }
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
    let pending = controls.gravity_requested;
    *controls = Controls {
        gravity_requested: pending,
        ..default()
    };
    let Ok(window) = windows.single() else {
        controls.gravity_requested = false;
        return;
    };
    if !window.focused {
        controls.gravity_requested = false;
        return;
    }
    controls.gravity_requested |= keys.just_pressed(KeyCode::KeyQ);
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
        controls.gravity_requested |= gamepad.just_pressed(GamepadButton::LeftTrigger2);
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
    mut controls: ResMut<Controls>,
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
    if *state.get() != GameState::Playing || pause {
        controls.gravity_requested = false;
    }
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

fn update_gravity(
    mut gravity: ResMut<GravityWell>,
    mut controls: ResMut<Controls>,
    players: Query<&Transform, With<Player>>,
    time: Res<Time<Fixed>>,
) {
    gravity.tick(time.delta_secs());
    if !std::mem::take(&mut controls.gravity_requested) {
        return;
    }
    let Ok(player) = players.single() else { return };
    let origin = player.translation.truncate();
    let target = if let Some(direction) = controls.aim {
        origin + direction.normalize_or_zero() * DEPLOY_RANGE
    } else {
        controls
            .cursor
            .unwrap_or(origin + player.local_y().truncate() * 200.0)
    };
    gravity.activate(origin, target);
}

fn draw_gravity(gravity: Res<GravityWell>, mut gizmos: Gizmos) {
    if let GravityPhase::Active { center, remaining } = gravity.phase() {
        let pose = Isometry2d::from_translation(center);
        gizmos
            .circle_2d(pose, FIELD_RADIUS, Color::srgb(0.35, 0.6, 1.0))
            .resolution(64);
        gizmos
            .circle_2d(
                pose,
                FIELD_RADIUS * (remaining / ACTIVE_DURATION),
                Color::srgba(0.5, 0.35, 1.0, 0.55),
            )
            .resolution(64);
        gizmos.cross_2d(pose, 8.0, Color::WHITE);
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

fn sample_asteroid(source: &Image) -> Option<(MaterialGrid, Image)> {
    let mut pixels = Vec::with_capacity(GRID_SIZE * GRID_SIZE * 4);
    let mut rows = [0; GRID_SIZE];
    for (y, row) in rows.iter_mut().enumerate() {
        for x in 0..GRID_SIZE {
            let sx = ((x as f32 + 0.5) * source.width() as f32 / GRID_SIZE as f32) as u32;
            let sy = ((y as f32 + 0.5) * source.height() as f32 / GRID_SIZE as f32) as u32;
            let mut color = source.get_color_at(sx, sy).ok()?.to_srgba().to_u8_array();
            if color[3] >= 128 {
                *row |= 1 << x;
            } else {
                color[3] = 0;
            }
            pixels.extend_from_slice(&color);
        }
    }
    let material = MaterialGrid::from_rows(rows);
    if material.is_empty() {
        return None;
    }
    Some((material, surface_image(pixels)))
}

fn surface_image(pixels: Vec<u8>) -> Image {
    let mut image = Image::new(
        Extent3d {
            width: GRID_SIZE as u32,
            height: GRID_SIZE as u32,
            depth_or_array_layers: 1,
        },
        TextureDimension::D2,
        pixels,
        TextureFormat::Rgba8UnormSrgb,
        RenderAssetUsages::MAIN_WORLD | RenderAssetUsages::RENDER_WORLD,
    );
    image.sampler = ImageSampler::nearest();
    image
}

fn spawn_asteroids(
    mut commands: Commands,
    players: Query<&Transform, With<Player>>,
    asteroids: Query<Entity, With<Asteroid>>,
    textures: (Res<GameAssets>, ResMut<Assets<Image>>),
    mut run: ResMut<Run>,
    time: Res<Time<Fixed>>,
) {
    if !run.spawn.tick(time.delta()).just_finished() || asteroids.iter().count() >= MAX_ASTEROIDS {
        return;
    }
    let Ok(player) = players.single() else { return };
    let (assets, mut images) = textures;
    let Some((material, image)) = images.get(&assets.asteroid).and_then(sample_asteroid) else {
        return;
    };
    let image = images.add(image);
    let angle = run.rng.random_range(0.0..std::f32::consts::TAU);
    let position = player.translation.truncate() + Vec2::from_angle(angle) * 650.0;
    commands.spawn((
        Asteroid {
            angular_velocity: run.rng.random_range(-2.0..2.0),
            center: material.centroid(),
            velocity: Vec2::ZERO,
            fragment_lifetime: None,
            fracture_pending: false,
            material,
            image: image.clone(),
        },
        GameEntity,
        Sprite::from_image(image),
        Transform::from_translation(position.extend(0.0)).with_scale(Vec3::splat(ASTEROID_SCALE)),
    ));
}

fn asteroid_local(transform: &Transform, point: Vec2) -> Vec2 {
    (transform.rotation.inverse() * (point - transform.translation.truncate()).extend(0.0))
        .truncate()
        / transform.scale.truncate()
}

fn asteroid_hit(
    transform: &Transform,
    material: &MaterialGrid,
    start: Vec2,
    end: Vec2,
) -> Option<f32> {
    let radius = GRID_SIZE as f32 * 0.5 * std::f32::consts::SQRT_2 * transform.scale.x;
    segment_circle_hit(start, end, transform.translation.truncate(), radius)?;
    material.ray_hit(
        asteroid_local(transform, start),
        asteroid_local(transform, end),
    )
}

fn mask_surface(material: &MaterialGrid, image: &mut Image) {
    let Some(pixels) = image.data.as_mut() else {
        return;
    };
    for (index, pixel) in pixels.as_chunks_mut::<4>().0.iter_mut().enumerate() {
        if !material.occupied(index % GRID_SIZE, index / GRID_SIZE) {
            pixel[3] = 0;
        }
    }
}

fn damage_surface(asteroid: &Asteroid, images: &mut Assets<Image>) {
    if let Some(mut image) = images.get_mut(&asteroid.image) {
        mask_surface(&asteroid.material, &mut image);
    }
}

fn despawn_asteroid(
    commands: &mut Commands,
    images: &mut Assets<Image>,
    entity: Entity,
    asteroid: &Asteroid,
) {
    images.remove(asteroid.image.id());
    commands.entity(entity).despawn();
}

fn move_asteroids(
    players: Query<&Transform, (With<Player>, Without<Asteroid>)>,
    mut asteroids: Query<(&mut Transform, &mut Asteroid), Without<Player>>,
    time: Res<Time<Fixed>>,
    gravity: Res<GravityWell>,
) {
    let player = players
        .single()
        .ok()
        .map(|transform| transform.translation.truncate());
    for (mut transform, mut asteroid) in &mut asteroids {
        let center = transform
            .transform_point(asteroid.center.extend(0.0))
            .truncate();
        if let Some(lifetime) = asteroid.fragment_lifetime.as_mut() {
            *lifetime -= time.delta_secs();
            if *lifetime > 0.0 {
                asteroid.velocity = (asteroid.velocity
                    + gravity.acceleration(center) * time.delta_secs())
                .clamp_length_max(MAX_FRAGMENT_SPEED);
            }
        } else if let Some(player) = player {
            asteroid.velocity = (player - center).normalize_or_zero() * 100.0;
        }
        let next_center = center + asteroid.velocity * time.delta_secs();
        transform.rotate_z(asteroid.angular_velocity * time.delta_secs());
        let offset = (transform.rotation
            * (asteroid.center * transform.scale.truncate()).extend(0.0))
        .truncate();
        transform.translation = (next_center - offset).extend(transform.translation.z);
    }
}

fn expire_fragments(
    mut commands: Commands,
    asteroids: Query<(Entity, &Asteroid)>,
    mut images: ResMut<Assets<Image>>,
) {
    for (entity, asteroid) in &asteroids {
        if asteroid
            .fragment_lifetime
            .is_some_and(|remaining| remaining <= 0.0)
        {
            despawn_asteroid(&mut commands, &mut images, entity, asteroid);
        }
    }
}

fn split_asteroids(
    mut commands: Commands,
    mut asteroids: Query<(&Transform, &mut Asteroid)>,
    mut images: ResMut<Assets<Image>>,
) {
    let body_count = asteroids.iter().count();
    let fragment_count = asteroids
        .iter()
        .filter(|(_, body)| body.fragment_lifetime.is_some())
        .count();
    let mut budget = MAX_FRAGMENTS
        .saturating_sub(fragment_count)
        .min(MAX_ASTEROIDS.saturating_sub(body_count));
    for (transform, mut asteroid) in &mut asteroids {
        if !asteroid.fracture_pending {
            continue;
        }
        asteroid.fracture_pending = false;
        let mut parts = asteroid.material.components().into_iter();
        let Some(largest) = parts.next() else {
            continue;
        };
        let previous_center = asteroid.center;
        let velocity = asteroid.velocity;
        let angular_velocity = asteroid.angular_velocity;
        let inherited_velocity = |center: Vec2| {
            let offset = (transform.rotation
                * ((center - previous_center) * transform.scale.truncate()).extend(0.0))
            .truncate();
            velocity + offset.perp() * angular_velocity
        };
        asteroid.center = largest.centroid();
        asteroid.velocity = inherited_velocity(asteroid.center);
        if asteroid.fragment_lifetime.is_some() {
            asteroid.velocity = asteroid.velocity.clamp_length_max(MAX_FRAGMENT_SPEED);
        }
        asteroid.material = largest;
        if parts.len() == 0 {
            continue;
        }
        let source = if budget > 0 {
            images.get(&asteroid.image).cloned()
        } else {
            None
        };
        for material in parts.take(MAX_NEW_FRAGMENTS) {
            if budget == 0 || material.cell_count() < MIN_FRAGMENT_CELLS {
                break;
            }
            let Some(source) = source.as_ref() else { break };
            let center = material.centroid();
            let outward = (transform.rotation * (center - previous_center).extend(0.0))
                .truncate()
                .normalize_or_zero();
            let velocity =
                (inherited_velocity(center) + outward * 35.0).clamp_length_max(MAX_FRAGMENT_SPEED);
            let mut image = source.clone();
            mask_surface(&material, &mut image);
            let image = images.add(image);
            commands.spawn((
                Asteroid {
                    material,
                    center,
                    velocity,
                    image: image.clone(),
                    angular_velocity,
                    fragment_lifetime: Some(
                        asteroid.fragment_lifetime.unwrap_or(FRAGMENT_LIFETIME),
                    ),
                    fracture_pending: false,
                },
                GameEntity,
                Sprite::from_image(image),
                *transform,
            ));
            budget -= 1;
        }
        damage_surface(&asteroid, &mut images);
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
    mut asteroids: Query<(Entity, &Transform, &mut Asteroid)>,
    mut images: ResMut<Assets<Image>>,
    mut run: ResMut<Run>,
) {
    for (bullet_entity, transform, bullet) in &bullets {
        let start = bullet.previous;
        let end = transform.translation.truncate();
        let hit = asteroids
            .iter()
            .filter_map(|(entity, transform, asteroid)| {
                asteroid_hit(transform, &asteroid.material, start, end).map(|t| (entity, t))
            })
            .min_by(|(ea, ta), (eb, tb)| {
                ta.total_cmp(tb)
                    .then_with(|| ea.to_bits().cmp(&eb.to_bits()))
            });
        if let Some((entity, t)) = hit {
            let Ok((_, transform, mut asteroid)) = asteroids.get_mut(entity) else {
                continue;
            };
            let impact = asteroid_local(transform, start.lerp(end, t));
            asteroid.material.remove_circle(impact, CRATER_RADIUS);
            asteroid.fracture_pending = true;
            commands.entity(bullet_entity).despawn();
            if asteroid.material.is_empty() {
                despawn_asteroid(&mut commands, &mut images, entity, &asteroid);
                if asteroid.fragment_lifetime.is_none() {
                    run.score += 1;
                }
            } else {
                damage_surface(&asteroid, &mut images);
            }
        }
    }
}

fn player_collisions(
    mut commands: Commands,
    mut players: Query<(&Transform, &mut Health), With<Player>>,
    asteroids: Query<(Entity, &Transform, &Asteroid)>,
    mut images: ResMut<Assets<Image>>,
    mut next: ResMut<NextState<GameState>>,
) {
    let Ok((player, mut health)) = players.single_mut() else {
        return;
    };
    if health.0 == 0 {
        return;
    }
    for (entity, transform, asteroid) in &asteroids {
        let center = asteroid_local(transform, player.translation.truncate());
        if asteroid
            .material
            .overlaps_circle(center, PLAYER_RADIUS / transform.scale.x)
        {
            despawn_asteroid(&mut commands, &mut images, entity, asteroid);
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
    gravity: Res<GravityWell>,
    mut previous: Local<Option<HudSnapshot>>,
) {
    let Ok(mut text) = hud.single_mut() else {
        return;
    };
    let hp = players.single().map_or(0, |health| health.0);
    let (gravity_label, seconds) = match gravity.phase() {
        GravityPhase::Ready => ("ready", 0),
        GravityPhase::Active { remaining, .. } => ("pulling", remaining.ceil() as u32),
        GravityPhase::Cooldown { remaining } => ("cooldown", remaining.ceil() as u32),
    };
    let snapshot = HudSnapshot {
        health: hp,
        score: run.score,
        state: *state.get(),
        gravity_label,
        gravity_seconds: seconds,
    };
    if *previous == Some(snapshot) {
        return;
    }
    *previous = Some(snapshot);
    let status = match state.get() {
        GameState::Playing => {
            "WASD / arrows: move  |  Mouse: aim  |  Click / Space: shoot  |  Q: gravity  |  Esc: pause"
        }
        GameState::Paused => "PAUSED — Esc / controller Start to resume",
        GameState::GameOver => "GAME OVER — R / Enter / controller East to restart",
    };
    let gravity_text = if seconds == 0 {
        gravity_label.to_owned()
    } else {
        format!("{gravity_label} {seconds}s")
    };
    let value = format!(
        "FENRIR   |   Hull {hp}/4   |   Score {}   |   Gravity {gravity_text}\n{status}",
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
        let mut images = Assets::<Image>::default();
        let source = images.add(surface_image(vec![255; GRID_SIZE * GRID_SIZE * 4]));
        app.add_plugins((MinimalPlugins, bevy::state::app::StatesPlugin))
            .insert_resource(images)
            .insert_resource(GameAssets {
                player: Handle::default(),
                asteroid: source,
                bullet: Handle::default(),
            })
            .insert_resource(TimeUpdateStrategy::FixedTimesteps(1));
        configure_gameplay(&mut app);
        app.update();
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
        asteroid_with_material(
            app,
            position,
            MaterialGrid::from_rows([u32::MAX; GRID_SIZE]),
        )
    }

    fn asteroid_with_material(app: &mut App, position: Vec2, material: MaterialGrid) -> Entity {
        let pixels: Vec<u8> = (0..GRID_SIZE * GRID_SIZE)
            .flat_map(|i| {
                [
                    200,
                    200,
                    200,
                    if material.occupied(i % GRID_SIZE, i / GRID_SIZE) {
                        255
                    } else {
                        0
                    },
                ]
            })
            .collect();
        let image = app
            .world_mut()
            .resource_mut::<Assets<Image>>()
            .add(surface_image(pixels));
        app.world_mut()
            .spawn((
                Asteroid {
                    angular_velocity: 0.0,
                    center: material.centroid(),
                    velocity: Vec2::ZERO,
                    fragment_lifetime: None,
                    fracture_pending: false,
                    material,
                    image: image.clone(),
                },
                Sprite::from_image(image),
                Transform::from_translation(position.extend(0.0))
                    .with_scale(Vec3::splat(ASTEROID_SCALE)),
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
    fn gravity_changes_only_in_range_fragment_velocity_and_obeys_speed_cap() {
        let mut app = game();
        let near = asteroid(&mut app, Vec2::new(80.0, 100.0));
        let far = asteroid(&mut app, Vec2::splat(500.0));
        let center = asteroid(&mut app, Vec2::new(0.0, 100.0));
        let fast = asteroid(&mut app, Vec2::new(80.0, 100.0));
        let root = asteroid(&mut app, Vec2::splat(100.0));
        for entity in [near, far, center, fast] {
            app.world_mut()
                .get_mut::<Asteroid>(entity)
                .unwrap()
                .fragment_lifetime = Some(FRAGMENT_LIFETIME);
        }
        app.world_mut().get_mut::<Asteroid>(fast).unwrap().velocity = Vec2::new(-399.0, 0.0);
        app.world_mut()
            .resource_mut::<GravityWell>()
            .activate(Vec2::ZERO, Vec2::new(0.0, 100.0));
        app.world_mut().run_system_once(move_asteroids).unwrap();
        let velocity = |entity| app.world().get::<Asteroid>(entity).unwrap().velocity;
        assert!(velocity(near).x < 0.0 && velocity(near).y == 0.0);
        assert_eq!(velocity(far), Vec2::ZERO);
        assert_eq!(velocity(center), Vec2::ZERO);
        assert!(velocity(fast).length() <= MAX_FRAGMENT_SPEED);
        assert!((velocity(root) - Vec2::splat(-1.0).normalize() * 100.0).length() < 0.001);
    }

    #[test]
    fn activation_is_latched_across_render_frames_and_consumed_once() {
        let mut app = game();
        app.init_resource::<ButtonInput<KeyCode>>()
            .init_resource::<ButtonInput<MouseButton>>();
        app.world_mut().spawn((Window::default(), PrimaryWindow));
        app.world_mut()
            .resource_mut::<ButtonInput<KeyCode>>()
            .press(KeyCode::KeyQ);
        app.world_mut().run_system_once(sample_controls).unwrap();
        assert!(app.world().resource::<Controls>().gravity_requested);
        app.world_mut()
            .resource_mut::<ButtonInput<KeyCode>>()
            .clear();
        app.world_mut().run_system_once(sample_controls).unwrap();
        assert!(app.world().resource::<Controls>().gravity_requested);
        app.world_mut().run_system_once(update_gravity).unwrap();
        assert!(!app.world().resource::<Controls>().gravity_requested);
        assert!(matches!(
            app.world().resource::<GravityWell>().phase(),
            GravityPhase::Active {
                remaining: ACTIVE_DURATION,
                ..
            }
        ));
        app.world_mut().run_system_once(update_gravity).unwrap();
        assert!(
            matches!(app.world().resource::<GravityWell>().phase(), GravityPhase::Active { remaining, .. } if remaining < ACTIVE_DURATION)
        );
    }

    #[test]
    fn simultaneous_pause_and_activation_are_resolved_before_simulation() {
        let mut app = game();
        app.init_resource::<ButtonInput<KeyCode>>()
            .init_resource::<ButtonInput<MouseButton>>();
        app.world_mut().spawn((Window::default(), PrimaryWindow));
        configure_input(&mut app);
        {
            let mut keys = app.world_mut().resource_mut::<ButtonInput<KeyCode>>();
            keys.press(KeyCode::KeyQ);
            keys.press(KeyCode::Escape);
        }
        app.update();
        assert_eq!(
            *app.world().resource::<State<GameState>>().get(),
            GameState::Paused
        );
        assert_eq!(
            app.world().resource::<GravityWell>().phase(),
            GravityPhase::Ready
        );
        assert!(!app.world().resource::<Controls>().gravity_requested);
    }

    #[test]
    fn controller_trigger_deploys_along_its_aim_direction() {
        let mut app = game();
        app.init_resource::<ButtonInput<KeyCode>>()
            .init_resource::<ButtonInput<MouseButton>>();
        app.world_mut().spawn((Window::default(), PrimaryWindow));
        let mut pad = Gamepad::default();
        pad.analog_mut().set(GamepadAxis::LeftStickX, 1.0);
        pad.digital_mut().press(GamepadButton::LeftTrigger2);
        app.world_mut().spawn(pad);
        app.world_mut().run_system_once(sample_controls).unwrap();
        app.world_mut().run_system_once(update_gravity).unwrap();
        assert_eq!(
            app.world().resource::<GravityWell>().phase(),
            GravityPhase::Active {
                center: Vec2::X * DEPLOY_RANGE,
                remaining: ACTIVE_DURATION,
            }
        );
        assert!(!app.world().resource::<Controls>().gravity_requested);
    }

    #[test]
    fn cooldown_requests_are_discarded_and_pause_cannot_queue_activation() {
        let mut app = game();
        app.init_resource::<ButtonInput<KeyCode>>();
        app.world_mut()
            .resource_mut::<GravityWell>()
            .activate(Vec2::ZERO, Vec2::Y * 100.0);
        app.world_mut()
            .resource_mut::<GravityWell>()
            .tick(ACTIVE_DURATION);
        app.world_mut().resource_mut::<Controls>().gravity_requested = true;
        app.world_mut().run_system_once(update_gravity).unwrap();
        assert!(!app.world().resource::<Controls>().gravity_requested);
        app.world_mut()
            .resource_mut::<GravityWell>()
            .tick(space_shooter::gravity::COOLDOWN);
        app.world_mut().run_system_once(update_gravity).unwrap();
        assert_eq!(
            app.world().resource::<GravityWell>().phase(),
            GravityPhase::Ready
        );
        app.world_mut()
            .resource_mut::<NextState<GameState>>()
            .set(GameState::Paused);
        app.update();
        app.world_mut().resource_mut::<Controls>().gravity_requested = true;
        app.world_mut().run_system_once(handle_state_input).unwrap();
        assert!(!app.world().resource::<Controls>().gravity_requested);
    }

    #[test]
    fn gravity_result_is_consistent_across_render_rates() {
        let results: Vec<(Vec2, Vec2)> = [30, 60, 144]
            .into_iter()
            .map(|fps| {
                let mut app = game();
                let piece = asteroid(&mut app, Vec2::new(80.0, 0.0));
                app.world_mut()
                    .get_mut::<Asteroid>(piece)
                    .unwrap()
                    .fragment_lifetime = Some(FRAGMENT_LIFETIME);
                {
                    let mut input = app.world_mut().resource_mut::<Controls>();
                    input.gravity_requested = true;
                    input.cursor = Some(Vec2::new(220.0, 0.0));
                }
                app.insert_resource(TimeUpdateStrategy::ManualDuration(Duration::from_secs_f64(
                    1.0 / f64::from(fps),
                )));
                for _ in 0..fps * 2 {
                    app.update();
                }
                let body = app.world().get::<Asteroid>(piece).unwrap();
                let position = app
                    .world()
                    .get::<Transform>(piece)
                    .unwrap()
                    .transform_point(body.center.extend(0.0))
                    .truncate();
                (position, body.velocity)
            })
            .collect();
        for (position, velocity) in &results {
            assert!(
                position.distance(results[1].0) <= MAX_FRAGMENT_SPEED / 60.0 + 0.1,
                "{results:?}"
            );
            assert!(
                velocity.distance(results[1].1)
                    <= space_shooter::gravity::MAX_ACCELERATION / 60.0 + 0.1,
                "{results:?}"
            );
        }
    }

    fn barbell() -> MaterialGrid {
        let mut rows = [0; GRID_SIZE];
        for row in &mut rows[12..21] {
            *row = (0xFF << 2) | (0xFF << 22);
        }
        rows[16] |= 0xFFF << 10;
        MaterialGrid::from_rows(rows)
    }

    #[test]
    fn projectile_cut_splits_without_duplicating_cells_pixels_or_pose() {
        let mut app = game();
        let rock = asteroid_with_material(&mut app, Vec2::ZERO, barbell());
        let pose = *app.world().get::<Transform>(rock).unwrap();
        bullet(&mut app, Vec2::ZERO, Vec2::ZERO);
        app.world_mut().run_system_once(bullet_collisions).unwrap();
        let damaged = app.world().get::<Asteroid>(rock).unwrap().material.clone();
        app.world_mut().run_system_once(split_asteroids).unwrap();
        assert_eq!(count::<Asteroid>(&mut app), 2);
        let mut bodies = app.world_mut().query::<(&Asteroid, &Transform)>();
        let mut cells = 0;
        let mut fragments = 0;
        for (body, transform) in bodies.iter(app.world()) {
            cells += body.material.cell_count();
            fragments += usize::from(body.fragment_lifetime.is_some());
            assert_eq!(transform.translation, pose.translation);
            assert_eq!(transform.rotation, pose.rotation);
            assert_eq!(body.center, body.material.centroid());
            assert!(body.velocity.is_finite() && body.velocity.length() <= MAX_FRAGMENT_SPEED);
            let image = app
                .world()
                .resource::<Assets<Image>>()
                .get(&body.image)
                .unwrap();
            for (i, pixel) in image
                .data
                .as_ref()
                .unwrap()
                .as_chunks::<4>()
                .0
                .iter()
                .enumerate()
            {
                assert_eq!(
                    pixel[3] > 0,
                    body.material.occupied(i % GRID_SIZE, i / GRID_SIZE)
                );
            }
        }
        assert_eq!(cells, damaged.cell_count());
        assert_eq!(fragments, 1);
        for y in 0..GRID_SIZE {
            for x in 0..GRID_SIZE {
                assert_eq!(
                    bodies
                        .iter(app.world())
                        .filter(|(body, _)| body.material.occupied(x, y))
                        .count(),
                    usize::from(damaged.occupied(x, y))
                );
            }
        }
        assert_eq!(app.world().resource::<Assets<Image>>().len(), 3);
        assert_eq!(app.world().resource::<Run>().score, 0);
    }

    #[test]
    fn fracture_budget_reserves_deferred_spawns_across_all_parents() {
        let mut app = game();
        let mut shape = barbell();
        shape.remove_circle(Vec2::ZERO, CRATER_RADIUS);
        for _ in 0..MAX_FRAGMENTS {
            let rock = asteroid_with_material(&mut app, Vec2::splat(1000.0), shape.clone());
            app.world_mut()
                .get_mut::<Asteroid>(rock)
                .unwrap()
                .fracture_pending = true;
        }
        app.world_mut().run_system_once(split_asteroids).unwrap();
        assert_eq!(count::<Asteroid>(&mut app), MAX_ASTEROIDS);
        let fragments = app
            .world_mut()
            .query::<&Asteroid>()
            .iter(app.world())
            .filter(|body| body.fragment_lifetime.is_some())
            .count();
        assert_eq!(fragments, MAX_FRAGMENTS);
        assert_eq!(
            app.world().resource::<Assets<Image>>().len(),
            MAX_ASTEROIDS + 1
        );
        app.world_mut().run_system_once(split_asteroids).unwrap();
        assert_eq!(count::<Asteroid>(&mut app), MAX_ASTEROIDS);
    }

    #[test]
    fn tiny_and_over_budget_regions_follow_the_debris_policy() {
        let mut app = game();
        let mut rows = [0; GRID_SIZE];
        for y in (0..GRID_SIZE - 1).step_by(3) {
            for x in (0..GRID_SIZE - 1).step_by(3) {
                rows[y] |= 3 << x;
                rows[y + 1] |= 3 << x;
            }
        }
        let rock = asteroid_with_material(&mut app, Vec2::ZERO, MaterialGrid::from_rows(rows));
        app.world_mut()
            .get_mut::<Asteroid>(rock)
            .unwrap()
            .fracture_pending = true;
        app.world_mut().run_system_once(split_asteroids).unwrap();
        assert_eq!(count::<Asteroid>(&mut app), MAX_NEW_FRAGMENTS + 1);
        let cells: u32 = app
            .world_mut()
            .query::<&Asteroid>()
            .iter(app.world())
            .map(|body| body.material.cell_count())
            .sum();
        assert_eq!(cells, (MAX_NEW_FRAGMENTS as u32 + 1) * 4);
        app.world_mut().run_system_once(cleanup_run).unwrap();
        let mut rows = [0; GRID_SIZE];
        rows[0] = 3;
        rows[1] = 3;
        rows[31] = 1 << 31;
        let rock = asteroid_with_material(&mut app, Vec2::ZERO, MaterialGrid::from_rows(rows));
        app.world_mut()
            .get_mut::<Asteroid>(rock)
            .unwrap()
            .fracture_pending = true;
        app.world_mut().run_system_once(split_asteroids).unwrap();
        assert_eq!(count::<Asteroid>(&mut app), 1);
        assert_eq!(
            app.world()
                .get::<Asteroid>(rock)
                .unwrap()
                .material
                .cell_count(),
            4
        );
        assert_eq!(app.world().resource::<Assets<Image>>().len(), 2);
    }

    #[test]
    fn refracturing_preserves_remaining_lifetime_and_expiry_releases_images() {
        let mut app = game();
        let mut material = barbell();
        material.remove_circle(Vec2::ZERO, CRATER_RADIUS);
        let rock = asteroid_with_material(&mut app, Vec2::splat(1000.0), material);
        {
            let mut body = app.world_mut().get_mut::<Asteroid>(rock).unwrap();
            body.fracture_pending = true;
            body.fragment_lifetime = Some(0.001);
        }
        app.world_mut().run_system_once(split_asteroids).unwrap();
        assert_eq!(count::<Asteroid>(&mut app), 2);
        for body in app.world_mut().query::<&Asteroid>().iter(app.world()) {
            assert_eq!(body.fragment_lifetime, Some(0.001));
        }
        app.world_mut().run_system_once(move_asteroids).unwrap();
        app.world_mut().run_system_once(expire_fragments).unwrap();
        assert_eq!(count::<Asteroid>(&mut app), 0);
        assert_eq!(app.world().resource::<Assets<Image>>().len(), 1);
        assert_eq!(app.world().resource::<Run>().score, 0);
    }

    #[test]
    fn expired_fragment_cannot_damage_the_player_in_its_last_tick() {
        let mut app = game();
        let rock = asteroid(&mut app, Vec2::ZERO);
        app.world_mut()
            .get_mut::<Asteroid>(rock)
            .unwrap()
            .fragment_lifetime = Some(0.001);
        app.update();
        assert!(app.world().get_entity(rock).is_err());
        assert_eq!(
            app.world_mut()
                .query::<&Health>()
                .single(app.world())
                .unwrap()
                .0,
            4
        );
        assert_eq!(app.world().resource::<Assets<Image>>().len(), 1);
    }

    #[test]
    fn fragment_rotates_about_its_material_center_and_cannot_farm_score() {
        let mut app = game();
        let mut rows = [0; GRID_SIZE];
        rows[0] = 3;
        rows[1] = 3;
        let rock =
            asteroid_with_material(&mut app, Vec2::splat(1000.0), MaterialGrid::from_rows(rows));
        let center = app.world().get::<Asteroid>(rock).unwrap().center;
        let before = app
            .world()
            .get::<Transform>(rock)
            .unwrap()
            .transform_point(center.extend(0.0));
        {
            let mut body = app.world_mut().get_mut::<Asteroid>(rock).unwrap();
            body.fragment_lifetime = Some(1.0);
            body.velocity = Vec2::new(100.0, 20.0);
            body.angular_velocity = 2.0;
        }
        app.world_mut().run_system_once(move_asteroids).unwrap();
        let after = app
            .world()
            .get::<Transform>(rock)
            .unwrap()
            .transform_point(center.extend(0.0));
        let dt = app.world().resource::<Time<Fixed>>().delta_secs();
        assert!(dt > 0.0);
        assert!((after - before - Vec3::new(100.0, 20.0, 0.0) * dt).length() < 0.001);
        bullet(&mut app, after.truncate(), after.truncate());
        app.world_mut().run_system_once(bullet_collisions).unwrap();
        assert!(app.world().get_entity(rock).is_err());
        assert_eq!(app.world().resource::<Run>().score, 0);
        assert_eq!(app.world().resource::<Assets<Image>>().len(), 1);
    }

    #[test]
    fn damage_pixels_match_collision_and_a_second_shot_crosses_the_hole() {
        let mut app = game();
        let rock = asteroid(&mut app, Vec2::ZERO);
        bullet(&mut app, Vec2::ZERO, Vec2::ZERO);
        app.world_mut().run_system_once(bullet_collisions).unwrap();
        let asteroid = app.world().get::<Asteroid>(rock).unwrap();
        let image = app
            .world()
            .resource::<Assets<Image>>()
            .get(&asteroid.image)
            .unwrap();
        for (i, pixel) in image
            .data
            .as_ref()
            .unwrap()
            .as_chunks::<4>()
            .0
            .iter()
            .enumerate()
        {
            assert_eq!(
                pixel[3] > 0,
                asteroid.material.occupied(i % GRID_SIZE, i / GRID_SIZE)
            );
        }
        let shot = bullet(&mut app, Vec2::ZERO, Vec2::ZERO);
        app.world_mut().run_system_once(bullet_collisions).unwrap();
        assert!(app.world().get_entity(shot).is_ok());
        assert_eq!(app.world().resource::<Run>().score, 0);
    }

    #[test]
    fn rotation_and_scale_map_world_hits_to_the_correct_cell() {
        let mut rows = [0; GRID_SIZE];
        rows[0] = 1;
        let material = MaterialGrid::from_rows(rows);
        let transform = Transform::from_xyz(70.0, -20.0, 0.0)
            .with_rotation(Quat::from_rotation_z(std::f32::consts::FRAC_PI_2))
            .with_scale(Vec3::splat(ASTEROID_SCALE));
        let world = |local: Vec2| transform.transform_point(local.extend(0.0)).truncate();
        assert!(
            asteroid_hit(
                &transform,
                &material,
                world(Vec2::new(-20.0, 15.5)),
                world(Vec2::new(-12.0, 15.5))
            )
            .is_some()
        );
        assert!(
            asteroid_hit(
                &transform,
                &material,
                world(Vec2::new(-20.0, -15.5)),
                world(Vec2::new(-12.0, -15.5))
            )
            .is_none()
        );
    }

    #[test]
    fn player_collision_respects_a_large_material_hole() {
        let mut app = game();
        let mut material = MaterialGrid::from_rows([u32::MAX; GRID_SIZE]);
        material.remove_circle(Vec2::ZERO, 10.0);
        let rock = asteroid_with_material(&mut app, Vec2::ZERO, material);
        app.world_mut().run_system_once(player_collisions).unwrap();
        assert!(app.world().get_entity(rock).is_ok());
        let health = app
            .world_mut()
            .query::<&Health>()
            .single(app.world())
            .unwrap()
            .0;
        assert_eq!(health, 4);
    }

    #[test]
    fn repository_texture_produces_matching_visible_material() {
        let source = Image::from_buffer(
            include_bytes!("../assets/asteroid.png"),
            bevy::image::ImageType::Extension("png"),
            bevy::image::CompressedImageFormats::NONE,
            true,
            ImageSampler::nearest(),
            RenderAssetUsages::MAIN_WORLD,
        )
        .unwrap();
        let (material, image) = sample_asteroid(&source).unwrap();
        assert!(!material.is_empty());
        assert!(!material.occupied(0, 0));
        for (i, pixel) in image
            .data
            .as_ref()
            .unwrap()
            .as_chunks::<4>()
            .0
            .iter()
            .enumerate()
        {
            assert_eq!(
                pixel[3] > 0,
                material.occupied(i % GRID_SIZE, i / GRID_SIZE)
            );
        }
    }

    #[test]
    fn empty_and_missing_image_data_do_not_spawn_material() {
        let transparent = surface_image(vec![0; GRID_SIZE * GRID_SIZE * 4]);
        assert!(sample_asteroid(&transparent).is_none());
        let mut missing = transparent;
        missing.data = None;
        assert!(sample_asteroid(&missing).is_none());
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
        assert!(
            !app.world()
                .get::<Asteroid>(nearer)
                .unwrap()
                .material
                .occupied(0, 16)
        );
        assert!(
            app.world()
                .get::<Asteroid>(farther)
                .unwrap()
                .material
                .occupied(0, 16)
        );
        assert!(app.world().get_entity(shot).is_err());
        assert_eq!(app.world().resource::<Run>().score, 0);
    }

    #[test]
    fn simultaneous_bullets_score_a_target_once() {
        let mut app = game();
        let mut rows = [0; GRID_SIZE];
        rows[16] = 1 << 16;
        asteroid_with_material(&mut app, Vec2::ZERO, MaterialGrid::from_rows(rows));
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
            assert!(
                app.world_mut()
                    .resource_mut::<GravityWell>()
                    .activate(Vec2::ZERO, Vec2::Y * 200.0)
            );
            app.world_mut().resource_mut::<Controls>().firing = true;
            for _ in 0..65 {
                app.update();
            }
            assert!(count::<Bullet>(&mut app) > 0);
            assert!(count::<Asteroid>(&mut app) > 0);
            for i in 0..8 {
                let rock = asteroid(&mut app, Vec2::ZERO);
                if i % 2 == 0 {
                    app.world_mut()
                        .get_mut::<Asteroid>(rock)
                        .unwrap()
                        .fragment_lifetime = Some(FRAGMENT_LIFETIME);
                }
            }
            app.world_mut().resource_mut::<Controls>().gravity_requested = true;
            app.world_mut().run_system_once(player_collisions).unwrap();
            app.update();
            assert_eq!(
                *app.world().resource::<State<GameState>>().get(),
                GameState::GameOver
            );
            assert_eq!(count::<GameEntity>(&mut app), 0);
            assert_eq!(app.world().resource::<Assets<Image>>().len(), 1);
            assert_eq!(
                app.world().resource::<GravityWell>().phase(),
                GravityPhase::Ready
            );
            assert!(!app.world().resource::<Controls>().gravity_requested);
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
        app.world_mut()
            .resource_mut::<GravityWell>()
            .activate(Vec2::ZERO, Vec2::Y * 200.0);
        let phase = app.world().resource::<GravityWell>().phase();
        let fragment = asteroid(&mut app, Vec2::splat(1000.0));
        app.world_mut()
            .get_mut::<Asteroid>(fragment)
            .unwrap()
            .fragment_lifetime = Some(1.0);

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
        assert_eq!(app.world().resource::<GravityWell>().phase(), phase);
        assert_eq!(
            app.world()
                .get::<Asteroid>(fragment)
                .unwrap()
                .fragment_lifetime,
            Some(1.0)
        );
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
