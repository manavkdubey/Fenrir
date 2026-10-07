use super::*;
use bevy::time::TimeUpdateStrategy;
use space_shooter::observability::Summary;

const TARGET_BODIES: usize = MAX_ASTEROIDS * 3 / 4;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
struct Options {
    seed: u64,
    ticks: usize,
    repeats: usize,
    warmup: usize,
}

impl Default for Options {
    fn default() -> Self {
        Self {
            seed: 42,
            ticks: 600,
            repeats: 5,
            warmup: 120,
        }
    }
}

fn parse(args: &[String]) -> Result<Options, String> {
    let mut options = Options::default();
    let mut args = args.iter();
    while let Some(flag) = args.next() {
        let value = args
            .next()
            .ok_or_else(|| format!("missing value for {flag}"))?;
        let value: u64 = value
            .parse()
            .map_err(|_| format!("invalid integer for {flag}"))?;
        match flag.as_str() {
            "--seed" => options.seed = value,
            "--ticks" if (10..=10_000).contains(&value) => options.ticks = value as usize,
            "--repeats" if (1..=20).contains(&value) => options.repeats = value as usize,
            "--warmup" if value <= 1_000 => options.warmup = value as usize,
            _ => return Err(format!("unknown or out-of-range option: {flag}")),
        }
    }
    Ok(options)
}

pub fn command(args: &[String]) -> Result<bool, String> {
    if args.is_empty() {
        return Ok(false);
    }
    if args == ["--help"] || args == ["-h"] {
        println!(
            "Fenrir: run without arguments to play.\n--benchmark [--seed N] [--ticks 10..10000] [--repeats 1..20] [--warmup 0..1000]"
        );
        return Ok(true);
    }
    if args[0] != "--benchmark" {
        return Err("use --help for supported arguments".into());
    }
    let options = parse(&args[1..])?;
    let cpu = std::fs::read_to_string("/proc/cpuinfo")
        .ok()
        .and_then(|text| {
            text.lines().find_map(|line| {
                line.strip_prefix("model name").and_then(|value| {
                    value
                        .split_once(':')
                        .map(|(_, name)| name.trim().to_owned())
                })
            })
        })
        .unwrap_or_else(|| "unavailable".into());
    let compiler = std::process::Command::new("rustc")
        .arg("--version")
        .output()
        .ok()
        .filter(|output| output.status.success())
        .map(|output| String::from_utf8_lossy(&output.stdout).trim().to_owned())
        .unwrap_or_else(|| "unavailable".into());
    println!(
        "headless dense scenario; build={} os={} arch={} threads={} cpu={cpu}",
        if cfg!(debug_assertions) {
            "debug"
        } else {
            "release"
        },
        std::env::consts::OS,
        std::env::consts::ARCH,
        std::thread::available_parallelism().map_or(1, usize::from)
    );
    println!("compiler={compiler}");
    println!(
        "seed={} warmup={} ticks={} repeats={} fixed_hz=60",
        options.seed, options.warmup, options.ticks, options.repeats
    );
    println!(
        "tick wall includes fixture refill and App::update; sim span excludes fixture refill; rendering/GPU/asset loading excluded; probes enabled"
    );
    let mut reference = None;
    let mut aggregate = Vec::new();
    for repeat in 0..options.repeats {
        let result = run(options)?;
        let outcome = (result.digest, result.work, result.peaks);
        if let Some(expected) = reference {
            if outcome != expected {
                return Err(format!(
                    "non-reproducible outcome at repeat {}: expected {expected:?}, got {outcome:?}",
                    repeat + 1
                ));
            }
        } else {
            reference = Some(outcome);
        }
        println!(
            "repeat={} digest={:016x} post_tick_peaks={:?} rss_snapshot_kib={}",
            repeat + 1,
            result.digest,
            result.peaks,
            resident_kib().map_or_else(|| "unavailable".into(), |value| value.to_string())
        );
        print_summary("tick_wall_ms", &result.wall);
        print_summary("sim_span_ms", &result.simulation);
        print_summary("collision_ms", &result.collision);
        print_summary("explosion_ms", &result.explosion);
        println!("work={:?}", result.work);
        aggregate.extend(result.simulation);
    }
    print_summary("aggregate_sim_span_ms", &aggregate);
    println!("all repeated outcome digests, work totals, and post-tick peaks match");
    Ok(true)
}

fn print_summary(label: &str, samples: &[f64]) {
    let summary =
        Summary::from_samples(samples.iter().copied()).expect("validated benchmark has samples");
    println!(
        "{label}: n={} mean={:.6} p50={:.6} p95={:.6} p99={:.6}",
        summary.count, summary.mean, summary.p50, summary.p95, summary.p99
    );
}

fn resident_kib() -> Option<u64> {
    std::fs::read_to_string("/proc/self/status")
        .ok()?
        .lines()
        .find_map(|line| {
            line.strip_prefix("VmRSS:")
                .and_then(|rest| rest.split_whitespace().next()?.parse().ok())
        })
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
struct Peaks {
    bodies: usize,
    fragments: usize,
    bullets: usize,
    images: usize,
}

struct ResultSamples {
    digest: u64,
    work: WorkCounts,
    peaks: Peaks,
    wall: Vec<f64>,
    simulation: Vec<f64>,
    collision: Vec<f64>,
    explosion: Vec<f64>,
}

struct Fixture {
    rng: StdRng,
    material: MaterialGrid,
    image: Image,
    spawned: usize,
}

fn prepare(seed: u64) -> Result<(App, Fixture), String> {
    let source = Image::from_buffer(
        include_bytes!("../assets/asteroid.png"),
        bevy::image::ImageType::Extension("png"),
        bevy::image::CompressedImageFormats::NONE,
        true,
        ImageSampler::nearest(),
        RenderAssetUsages::MAIN_WORLD,
    )
    .map_err(|error| error.to_string())?;
    let (material, image) = sample_asteroid(&source).ok_or("asteroid texture has no material")?;
    let mut images = Assets::<Image>::default();
    let source = images.add(source);
    let mut app = App::new();
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
    for mut health in app
        .world_mut()
        .query::<&mut Health>()
        .iter_mut(app.world_mut())
    {
        health.0 = u32::MAX;
    }
    {
        let mut run = app.world_mut().resource_mut::<Run>();
        run.rng = StdRng::seed_from_u64(seed);
        run.spawn = Timer::from_seconds(1_000_000.0, TimerMode::Repeating);
    }
    Ok((
        app,
        Fixture {
            rng: StdRng::seed_from_u64(seed),
            material,
            image,
            spawned: 0,
        },
    ))
}

impl Fixture {
    fn step(&mut self, app: &mut App, tick: usize) {
        let player = app
            .world_mut()
            .query_filtered::<&Transform, With<Player>>()
            .single(app.world())
            .expect("benchmark player exists")
            .translation
            .truncate();
        let bodies = app
            .world_mut()
            .query::<&Asteroid>()
            .iter(app.world())
            .count();
        for _ in bodies..TARGET_BODIES {
            let angle = self.rng.random_range(0.0..std::f32::consts::TAU);
            let position = player + Vec2::from_angle(angle) * self.rng.random_range(90.0..600.0);
            let mut material = self.material.clone();
            let cut = self.spawned.is_multiple_of(8);
            if cut {
                for y in -16..=16 {
                    material.remove_circle(Vec2::new(0.0, y as f32), 2.0);
                }
            }
            let mut image = self.image.clone();
            mask_surface(&material, &mut image);
            let image = app.world_mut().resource_mut::<Assets<Image>>().add(image);
            app.world_mut().spawn((
                Asteroid {
                    center: material.centroid(),
                    material,
                    image: image.clone(),
                    angular_velocity: self.rng.random_range(-2.0..2.0),
                    velocity: Vec2::ZERO,
                    fragment_lifetime: None,
                    fracture_pending: cut,
                },
                GameEntity,
                Sprite::from_image(image),
                Transform::from_translation(position.extend(0.0))
                    .with_scale(Vec3::splat(ASTEROID_SCALE)),
            ));
            self.spawned += 1;
        }
        let angle = tick as f32 * 0.035;
        let bullets = app.world_mut().query::<&Bullet>().iter(app.world()).count();
        let bullet_image = app.world().resource::<GameAssets>().bullet.clone();
        for _ in bullets..MAX_BULLETS {
            let direction = Vec2::from_angle(angle + self.rng.random_range(-1.0..1.0));
            app.world_mut().spawn((
                Bullet {
                    velocity: direction * BULLET_SPEED,
                    previous: player,
                    remaining: BULLET_LIFETIME,
                },
                GameEntity,
                Sprite::from_image(bullet_image.clone()),
                Transform::from_translation(player.extend(0.5))
                    .with_rotation(Quat::from_rotation_z(
                        Vec2::Y.angle_to(direction) + std::f32::consts::FRAC_PI_2,
                    ))
                    .with_scale(Vec3::splat(0.05)),
            ));
        }
        let ready = app.world().resource::<GravityWell>().phase() == GravityPhase::Ready;
        let mut controls = app.world_mut().resource_mut::<Controls>();
        controls.movement = Vec2::from_angle(tick as f32 * 0.017);
        controls.aim = Some(Vec2::from_angle(angle));
        controls.firing = true;
        controls.gravity_requested = ready && tick.is_multiple_of(30);
    }
}

fn run(options: Options) -> Result<ResultSamples, String> {
    let (mut app, mut fixture) = prepare(options.seed)?;
    for tick in 0..options.warmup {
        fixture.step(&mut app, tick);
        app.update();
    }
    let mut result = ResultSamples {
        digest: 0,
        work: WorkCounts::default(),
        peaks: Peaks::default(),
        wall: Vec::new(),
        simulation: Vec::new(),
        collision: Vec::new(),
        explosion: Vec::new(),
    };
    for tick in options.warmup..options.warmup + options.ticks {
        let completed = app.world().resource::<Performance>().completed_ticks;
        let started = Instant::now();
        fixture.step(&mut app, tick);
        app.update();
        let elapsed = started.elapsed().as_secs_f64() * 1000.0;
        let performance = app.world().resource::<Performance>();
        if performance.completed_ticks != completed + 1 {
            return Err("benchmark did not execute exactly one simulation tick".into());
        }
        let sample = performance.current;
        result.wall.push(elapsed);
        result.simulation.push(sample.simulation_ms);
        result.collision.push(sample.collision_ms);
        result.explosion.push(sample.explosion_ms);
        result.work.accumulate(sample.work);
        let bodies: Vec<_> = app
            .world_mut()
            .query::<&Asteroid>()
            .iter(app.world())
            .collect();
        result.peaks.bodies = result.peaks.bodies.max(bodies.len());
        result.peaks.fragments = result.peaks.fragments.max(
            bodies
                .iter()
                .filter(|body| body.fragment_lifetime.is_some())
                .count(),
        );
        result.peaks.bullets = result
            .peaks
            .bullets
            .max(app.world_mut().query::<&Bullet>().iter(app.world()).count());
        result.peaks.images = result
            .peaks
            .images
            .max(app.world().resource::<Assets<Image>>().len());
    }
    result.digest = digest(&mut app);
    Ok(result)
}

fn digest(app: &mut App) -> u64 {
    let mut records = Vec::new();
    for (transform, body) in app
        .world_mut()
        .query::<(&Transform, &Asteroid)>()
        .iter(app.world())
    {
        let mut record = vec![
            0,
            transform.translation.x.to_bits(),
            transform.translation.y.to_bits(),
            transform.rotation.z.to_bits(),
            transform.rotation.w.to_bits(),
            body.velocity.x.to_bits(),
            body.velocity.y.to_bits(),
            body.center.x.to_bits(),
            body.center.y.to_bits(),
            body.angular_velocity.to_bits(),
            u32::from(body.fragment_lifetime.is_some()),
            body.fragment_lifetime.unwrap_or(0.0).to_bits(),
            u32::from(body.fracture_pending),
        ];
        for y in 0..GRID_SIZE {
            record.push((0..GRID_SIZE).fold(0, |row, x| {
                row | (u32::from(body.material.occupied(x, y)) << x)
            }));
        }
        records.push(record);
    }
    for (transform, bullet) in app
        .world_mut()
        .query::<(&Transform, &Bullet)>()
        .iter(app.world())
    {
        records.push(vec![
            1,
            transform.translation.x.to_bits(),
            transform.translation.y.to_bits(),
            bullet.velocity.x.to_bits(),
            bullet.velocity.y.to_bits(),
            bullet.previous.x.to_bits(),
            bullet.previous.y.to_bits(),
            bullet.remaining.to_bits(),
        ]);
    }
    for (transform, health) in app
        .world_mut()
        .query::<(&Transform, &Health)>()
        .iter(app.world())
    {
        records.push(vec![
            2,
            transform.translation.x.to_bits(),
            transform.translation.y.to_bits(),
            health.0,
        ]);
    }
    records.push(vec![
        3,
        app.world().resource::<Run>().score,
        app.world().resource::<Explosions>().pending_count() as u32,
    ]);
    records.push(match app.world().resource::<GravityWell>().phase() {
        GravityPhase::Ready => vec![4, 0],
        GravityPhase::Active { center, remaining } => vec![
            4,
            1,
            center.x.to_bits(),
            center.y.to_bits(),
            remaining.to_bits(),
        ],
        GravityPhase::Cooldown { remaining } => vec![4, 2, remaining.to_bits()],
    });
    records.sort();
    let mut hash = 0xcbf29ce484222325_u64;
    for record in records {
        for value in std::iter::once(record.len() as u32).chain(record) {
            for byte in value.to_le_bytes() {
                hash = (hash ^ u64::from(byte)).wrapping_mul(0x100000001b3);
            }
        }
    }
    hash
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn command_limits_reject_unbounded_or_incomplete_requests() {
        let strings = |args: &[&str]| args.iter().map(|s| (*s).to_owned()).collect::<Vec<_>>();
        assert_eq!(parse(&[]).unwrap(), Options::default());
        assert!(parse(&strings(&["--ticks", "0"])).is_err());
        assert!(parse(&strings(&["--repeats", "1000"])).is_err());
        assert!(parse(&strings(&["--seed"])).is_err());
        assert!(parse(&strings(&["--seed", "no"])).is_err());
    }

    #[test]
    fn scripted_dense_workload_repeats_with_identical_outcomes_and_counts() {
        let options = Options {
            seed: 9,
            ticks: 30,
            repeats: 2,
            warmup: 2,
        };
        let first = run(options).unwrap();
        let second = run(options).unwrap();
        assert_eq!(
            (first.digest, first.work, first.peaks),
            (second.digest, second.work, second.peaks)
        );
        assert_eq!(first.wall.len(), options.ticks);
        assert!(first.work.bullet_pairs > 0);
        assert!(
            first.peaks.bodies <= MAX_ASTEROIDS
                && first.peaks.fragments <= MAX_FRAGMENTS
                && first.peaks.bullets <= MAX_BULLETS
        );
    }
}
