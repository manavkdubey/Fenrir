use std::io::BufWriter;
use std::slice::Windows;

use bevy::asset::{LoadState, RenderAssetUsages};
use bevy::prelude::*;
use bevy::ui::ContentSize;
use bevy::ui::widget::TextNodeFlags;
use bevy::window::{PrimaryWindow, WindowResolution};
use bevy::{app::Startup, utils::dbg};
use bevy_asset_loader::loading_state::{LoadingState, LoadingStateAppExt};
use bevy_state::prelude::*;
use image::{ImageBuffer, Rgba};
use rand::Rng;
use space_shooter::state::GameState;

#[derive(Component, Default)]
pub struct Player;

#[derive(Bundle, Default)]
struct PlayerBundle {
    player: Player,
    name: Name,
}
#[derive(Component, Default)]
struct Asteroid {
    vel: Vec3,
    pub angular_velocity: f32,
}
#[derive(Component)]
pub struct PlayerController(pub Gamepad);

#[derive(Component, Default)]
pub struct Bullet {
    speed: f32,
}
#[derive(Resource, Clone)]
pub struct BulletImage(pub Handle<Image>);

#[derive(Resource, Clone)]
pub struct FireRate(Timer);

#[derive(Component)]
struct Health {
    hp: i32,
}

#[derive(Resource, Clone)]
struct SpawnTimer(Timer);

#[derive(Bundle, Default)]
struct BulletBundle {
    bullet: Bullet,
    name: Name,
}
#[derive(Component)]
pub struct GameEntity;

#[derive(SystemSet, Debug, Hash, PartialEq, Eq, Clone)]
struct GameplaySet;

fn main() {
    App::new()
        .add_plugins(DefaultPlugins.set(WindowPlugin {
            primary_window: Some(Window {
                resolution: (1280., 720.).into(),
                title: "Space shooter".into(),
                resizable: true,
                present_mode: bevy::window::PresentMode::Immediate,
                ..default()
            }),
            ..default()
        }))
        .insert_resource(SpawnTimer(Timer::from_seconds(1.0, TimerMode::Repeating)))
        .insert_resource(FireRate(Timer::from_seconds(0.15, TimerMode::Repeating)))
        // .add_loading_state(LoadingState::new(GameState::Loading))
        .add_systems(Startup, (camera, rotate_bullet_on_startup))
        .insert_state::<GameState>(GameState::Playing)
        .add_systems(
            Update,
            (
                move_player,
                shoot,
                move_bullet,
                aim,
                rotate_bullet_on_startup,
                mouse_click_to_world,
                spawn_asteriods,
                move_asteroids,
                bullet_hits_asteroid,
            )
                .run_if(in_state(GameState::Playing))
                .in_set(GameplaySet),
        )
        .add_systems(Last, despawn_asteroid)
        .add_systems(
            OnEnter(GameState::GameOver),
            show_game_over.run_if(in_state(GameState::GameOver)),
        )
        .add_systems(
            Update,
            IntoSystem::into_system(restart_on_o).run_if(in_state(GameState::GameOver)),
        )
        .add_systems(OnEnter(GameState::Playing), (cleanup_game, setup_system))
        .run();
}

fn setup_system(mut commands: Commands, asset_server: Res<AssetServer>) {
    commands
        .spawn(PlayerBundle::default())
        .insert(Player)
        .insert(Name::from("Player"))
        .insert(Sprite {
            image: asset_server.load("fighter.png"),
            ..Default::default()
        })
        .insert(Transform {
            translation: Vec3::new(0.0, 0.0, 1.0),
            scale: Vec3::splat(0.07),
            ..default()
        })
        .insert(GameEntity)
        .insert(Health { hp: 4 });
}
fn camera(mut commands: Commands) {
    commands.spawn(Camera2d::default());
}
fn gamepad_system(gamepads: Query<(Entity, &Gamepad)>) {
    for (entity, gamepad) in &gamepads {
        if gamepad.just_pressed(GamepadButton::South) {
            info!("{:?} just pressed South", entity);
        } else if gamepad.just_released(GamepadButton::South) {
            info!("{:?} just released South", entity);
        }

        let right_trigger = gamepad.get(GamepadButton::RightTrigger2).unwrap();
        if right_trigger.abs() > 0.01 {
            info!("{:?} RightTrigger2 value is {}", entity, right_trigger);
        }

        let left_stick_x = gamepad.get(GamepadAxis::LeftStickX).unwrap();
        if left_stick_x.abs() > 0.01 {
            info!("{:?} LeftStickX value is {}", entity, left_stick_x);
        }
    }
}

pub fn move_player(
    mut query: Query<&mut Transform, With<Player>>,
    gamepads: Query<(Entity, &Gamepad)>,
) {
    for mut transform in query.iter_mut() {
        for (_, gamepad) in &gamepads {
            if let Some(rightx) = gamepad.get(GamepadAxis::RightStickX) {
                transform.translation.x += rightx * 3.0;
            }
            if let Some(righty) = gamepad.get(GamepadAxis::RightStickY) {
                transform.translation.y += righty * 3.0;
            }
        }
    }
}
use bevy::render::render_resource::{Extent3d, TextureDimension, TextureFormat};

fn rotate_image_90_ccw(original: &Image) -> Image {
    assert_eq!(
        original.texture_descriptor.format,
        TextureFormat::Rgba8UnormSrgb
    );

    let width = original.width() as usize;
    let height = original.height() as usize;
    let bytes_per_pixel = 4;
    let mut new_data = vec![0; original.data.len()];

    for y in 0..height {
        for x in 0..width {
            let src_index = (y * width + x) * bytes_per_pixel;
            let dst_x = y;
            let dst_y = width - x - 1;
            let dst_index = (dst_y * height + dst_x) * bytes_per_pixel;

            new_data[dst_index..dst_index + bytes_per_pixel]
                .copy_from_slice(&original.data[src_index..src_index + bytes_per_pixel]);
        }
    }

    let new_size = Extent3d {
        width: height as u32,
        height: width as u32,
        depth_or_array_layers: 1,
    };

    Image::new(
        new_size,
        TextureDimension::D2,
        new_data,
        TextureFormat::Rgba8UnormSrgb,
        RenderAssetUsages::RENDER_WORLD | RenderAssetUsages::MAIN_WORLD,
    )
}
pub fn rotate_bullet_on_startup(
    asset_server: Res<AssetServer>,
    mut images: ResMut<Assets<Image>>,
    mut commands: Commands,
    mut has_run: Local<bool>,
    mut bullet_handle: Local<Option<Handle<Image>>>,
) {
    if *has_run {
        return;
    }

    if bullet_handle.is_none() {
        // Load it once
        *bullet_handle = Some(asset_server.load("bullet.png"));
        return;
    }

    let handle = bullet_handle.as_ref().unwrap();

    match asset_server.get_load_state(handle) {
        Some(LoadState::Loaded) => {
            dbg!("image loaded!");
            if let Some(original) = images.get(handle) {
                let rotated = rotate_image_90_ccw(original);

                let rotated_handle = images.add(rotated);
                commands.insert_resource(BulletImage(rotated_handle));
                *has_run = true;
            }
        }
        Some(state) => {
            dbg!(state);
        }
        None => {
            dbg!("load state unknown");
        }
    }
}

pub fn aim(mut player: Query<&mut Transform, With<Player>>, gamepads: Query<&Gamepad>) {
    for mut player_transform in player.iter_mut() {
        // You can use the first connected gamepad, or handle multiple if needed
        for gamepad in gamepads.iter() {
            let x = gamepad.get(GamepadAxis::LeftStickX).unwrap_or(0.0);
            let y = gamepad.get(GamepadAxis::LeftStickY).unwrap_or(0.0);

            let input_dir = Vec2::new(x, y);
            if input_dir.length_squared() > 0.01 {
                let player_dir = player_transform.local_y().truncate();
                let angle = player_dir.angle_between(input_dir);
                player_transform.rotate_z(angle);
            }
        }
    }
}
pub fn shoot(
    mut commands: Commands,
    gamepads: Query<&Gamepad>,
    bullet_image: Option<Res<BulletImage>>,
    player: Query<&Transform, With<Player>>,
    mut rate_timer: ResMut<FireRate>,
    time: Res<Time>,
) {
    let Some(bullet_image) = bullet_image else {
        dbg("No img");
        return;
    };

    for transform in player.iter() {
        for gamepad in gamepads.iter() {
            if rate_timer.0.tick(time.delta()).just_finished()
                && gamepad.pressed(GamepadButton::RightTrigger2)
            {
                let player_trans = transform.translation;
                let rotation = transform.rotation;
                let mut transform = Transform::from_xyz(player_trans.x, player_trans.y, -1f32);
                transform.rotation = rotation;
                transform.scale = Vec3::splat(0.05);

                commands
                    .spawn(Bullet::default())
                    .insert(Bullet { speed: 500f32 })
                    .insert(Name::from("Bullet"))
                    .insert(Sprite {
                        image: bullet_image.0.clone(),
                        ..Default::default()
                    })
                    .insert(transform)
                    .insert(GameEntity);
            }
        }
    }
}

fn mouse_click_to_world(
    windows: Query<&Window>,
    camera_q: Query<(&Camera, &GlobalTransform)>,
    mouse: Res<ButtonInput<MouseButton>>,
) {
    if mouse.just_pressed(MouseButton::Left) {
        let window = windows.single();
        if let Some(cursor_pos) = window.cursor_position() {
            let (camera, camera_transform) = camera_q.single();

            // Convert screen position (UI coords) to world space
            if let Some(world_pos) = camera.viewport_to_world(camera_transform, cursor_pos).ok() {
                let world_translation = world_pos.origin.truncate(); // Vec2 (x, y)
                dbg!(world_translation);
            }
        }
    }
}

pub fn move_bullet(mut query: Query<(&mut Transform, &Bullet)>, time: Res<Time>) {
    for (mut transform, bullet) in query.iter_mut() {
        let direction = transform.local_y();
        transform.translation += direction * bullet.speed * time.delta_secs();
    }
}

fn spawn_asteriods(
    mut commands: Commands,
    asset_server: Res<AssetServer>,
    mut spawn_timer: ResMut<SpawnTimer>,
    time: Res<Time>,
    player: Query<&Transform, With<Player>>,
) {
    if spawn_timer.0.tick(time.delta()).just_finished() {
        let Ok(player_transform) = player.get_single() else {
            return;
        };

        let mut rng = rand::rng();
        // Generate a position outside the exclusion zone
        let x = rng.gen_range(-600.0..600.0);
        let y = rng.gen_range(-350.0..350.0);

        // If it's inside the exclusion zone, mirror it out
        let spawn_x = if (x - player_transform.translation.x).abs() < 200.0 {
            if x.is_sign_positive() {
                x + 200.0
            } else {
                x - 200.0
            }
        } else {
            x
        };

        let spawn_y = if (y - player_transform.translation.y).abs() < 150.0 {
            if y.is_sign_positive() {
                y + 150.0
            } else {
                y - 150.0
            }
        } else {
            y
        };
        let position = Vec3::new(spawn_x, spawn_y, 0.0);
        let direction = (player_transform.translation - position).normalize();
        dbg("Asteoid");
        commands
            .spawn(SpriteBundle {
                sprite: Sprite {
                    image: asset_server.load("asteroid.png"),
                    ..Default::default()
                },
                transform: Transform {
                    translation: position,
                    scale: Vec3::splat(0.07),
                    ..default()
                },
                ..Default::default()
            })
            .insert(Asteroid {
                vel: direction * 100.0,
                angular_velocity: rng.gen_range(-2.0..2.0),
            })
            .insert(GameEntity);
    }
}
fn move_asteroids(
    mut query: ParamSet<(
        Query<(&mut Transform, &Asteroid)>,
        Query<&Transform, With<Player>>,
    )>,
    time: Res<Time>,
) {
    let player_translation = if let Ok(transform) = query.p1().get_single() {
        transform.translation
    } else {
        return;
    };

    let mut binding2 = query.p0();
    for (mut transform, asteroid) in binding2.iter_mut() {
        let direction = (player_translation - transform.translation).normalize();
        let speed = 100.0;
        transform.translation += direction * speed * time.delta_secs();
        transform.rotate_z(asteroid.angular_velocity * time.delta_secs());
    }
}
fn despawn_asteroid(
    asteroids: Query<(Entity, &Transform), With<Asteroid>>,
    players: Query<(Entity, &Transform), With<Player>>,
    mut health: Query<&mut Health>,
    asset_server: Res<AssetServer>,
    mut commands: Commands,
    mut next_state: ResMut<NextState<GameState>>,
) {
    for asteroid in asteroids.iter() {
        let Ok(player_transform) = players.get_single() else {
            return;
        };

        let distance = asteroid
            .1
            .translation
            .distance(player_transform.1.translation);
        if distance < 30.0 {
            commands.entity(asteroid.0).despawn();
            if let Ok(mut health) = health.get_mut(player_transform.0) {
                health.hp -= 1;
                dbg!(health.hp);

                if health.hp <= 0 {
                    commands.entity(player_transform.0).despawn();
                    next_state.set(GameState::GameOver);
                }
            }
        }
    }
}
fn show_game_over(mut commands: Commands, asset_server: Res<AssetServer>) {
    let font_handle = asset_server.load("fonts/FiraSans-Bold.ttf");

    // Text root entity with all required components
    commands
        .spawn((
            Node::default(), // required for layout
            Text::new("GAME OVER\nPress O on your Controller to Restart"),
            TextFont {
                font: font_handle.into(),
                font_size: 48.0,
                ..default()
            },
            TextColor(Color::WHITE),
            TextLayout::default(),
            TextNodeFlags::default(),
            ContentSize::default(),
            Name::new("GameOverText"),
        ))
        .insert(GameEntity);
}

fn restart_on_o(gamepads: Query<&Gamepad>, mut next_state: ResMut<NextState<GameState>>) {
    let Ok(gamepad) = gamepads.get_single() else {
        return;
    };
    if gamepad.just_pressed(GamepadButton::East) {
        next_state.set(GameState::Playing);
    }
}

fn cleanup_game(
    mut commands: Commands,
    entities: Query<Entity, With<GameEntity>>,
    mut spawn_timer: Option<ResMut<SpawnTimer>>,
    mut fire_rate: Option<ResMut<FireRate>>,
) {
    for entity in entities.iter() {
        commands.entity(entity).despawn_recursive();
    }

    if let Some(mut timer) = spawn_timer {
        timer.0.reset();
    }

    if let Some(mut timer) = fire_rate {
        timer.0.reset();
    }
}
fn bullet_hits_asteroid(
    mut commands: Commands,
    asteroids: Query<(Entity, &Transform), With<Asteroid>>,
    bullets: Query<(Entity, &Transform), With<Bullet>>,
) {
    for asteroid in asteroids.iter() {
        for bullet in bullets.iter() {
            let distance = asteroid.1.translation.distance(bullet.1.translation);
            if distance < 30.0 {
                commands.entity(asteroid.0).despawn();
                commands.entity(bullet.0).despawn();
            }
        }
    }
}
