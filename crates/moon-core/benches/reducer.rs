#[cfg(not(windows))]
use criterion::{Criterion, criterion_group, criterion_main};
#[cfg(not(windows))]
use moon_core::{
    command::Command,
    config::Config,
    geometry::{Direction, Rect},
    ids::{MonitorId, WindowId},
    world::{Event, Monitor, WindowInfo, World, reduce},
};
#[cfg(not(windows))]
use std::hint::black_box;
#[cfg(not(windows))]
fn benchmark(c: &mut Criterion) {
    let mut world = World::new(
        Config::default(),
        vec![Monitor {
            id: MonitorId(1),
            name: "benchmark".into(),
            bounds: Rect::new(0, 0, 3840, 2160),
            work_area: Rect::new(0, 0, 3840, 2160),
            scale: 1.25,
        }],
    )
    .unwrap();
    for id in 1..=50 {
        world = reduce(
            &world,
            Event::WindowAdded(WindowInfo {
                id: WindowId(id),
                process_id: id as u32,
                process: "testwin".into(),
                class: "Test".into(),
                title: format!("Window {id}"),
                rect: Rect::new(0, 0, 600, 400),
                monitor: MonitorId(1),
            }),
        )
        .unwrap()
        .world;
    }
    c.bench_function("reduce_and_layout_50_windows", |b| {
        b.iter(|| {
            black_box(
                reduce(black_box(&world), Event::Command(Command::Focus(Direction::Left))).unwrap(),
            )
        })
    });
}
#[cfg(not(windows))]
criterion_group!(benches, benchmark);
#[cfg(not(windows))]
criterion_main!(benches);

#[cfg(windows)]
fn main() {
    eprintln!("Core performance benchmarks run natively on Linux.");
}
