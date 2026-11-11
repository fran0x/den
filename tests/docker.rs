use crossterm::event::{KeyCode, KeyEvent, KeyModifiers};
use den::{
    app::{App, Module},
    docker::{self, COMMANDS, Section},
    inventory::Inventory,
    presentation::Destination,
    ui,
};
use ratatui::{Terminal, backend::TestBackend};
use std::{path::Path, sync::atomic::AtomicBool};

const FIXTURE: &str = "fixtures/docker";

fn collect() -> docker::Inventory {
    let context = docker::Context::fixture(Path::new(FIXTURE)).unwrap();
    docker::discover(&context, &AtomicBool::new(false)).unwrap()
}
fn app() -> App {
    let mut a = App::new(
        Inventory {
            schema_version: 1,
            resources: vec![],
        },
        "isolated Docker fixture",
    );
    a.docker
        .configure(docker::Context::fixture(Path::new(FIXTURE)).unwrap());
    a.docker.finish(Ok(collect()));
    a.docker.requested = false;
    a.navigate(Destination::Containers);
    a
}
fn key(a: &mut App, code: KeyCode) {
    a.handle_key(KeyEvent::new(code, KeyModifiers::NONE));
}
fn draw(a: &mut App, w: u16, h: u16) -> String {
    let mut t = Terminal::new(TestBackend::new(w, h)).unwrap();
    t.draw(|f| ui::render(f, a)).unwrap();
    (0..h)
        .map(|y| {
            (0..w)
                .map(|x| t.backend().buffer()[(x, y)].symbol())
                .collect::<String>()
        })
        .collect::<Vec<_>>()
        .join("\n")
}
fn cells(rows: &[docker::Row]) -> Vec<(&str, &str, &str)> {
    rows.iter()
        .map(|r| (r.name.as_str(), r.state.as_str(), r.status.as_str()))
        .collect()
}

#[test]
fn den_runs_only_fixed_listing_commands() {
    let args: Vec<_> = COMMANDS.iter().map(|(_, a)| a.join(" ")).collect();
    assert_eq!(
        args,
        [
            "container ls --all --no-trunc --format {{json .}}",
            "image ls --no-trunc --format {{json .}}",
            "volume ls --format {{json .}}",
            "system df --format {{json .}}",
        ]
    );
}

#[test]
fn containers_images_and_volumes_say_what_uses_what() {
    let i = collect();
    assert_eq!(
        cells(&i.containers),
        [
            ("web-nginx-1", "Running", "nginx:latest · web"),
            ("web-db-1", "Stopped", "postgres:16 · web"),
            (
                "scratch",
                "Paused",
                "sha256:dddddddddddd4444444444444444444444444444444444444444444444444444"
            ),
        ]
    );
    assert_eq!(i.running, 1);
    assert_eq!(
        cells(&i.images),
        [
            ("nginx:latest", "192MB", "1 container"),
            ("postgres:16", "438MB", "1 container"),
            // Used by ID from the paused container.
            ("<none> dddddddddddd", "77MB", "1 container"),
            ("redis:7", "138MB", "Unused"),
            ("<none> 888888888888", "12MB", "Unused · dangling"),
        ]
    );
    assert_eq!(
        cells(&i.volumes),
        [
            ("web_data", "local", "1 container"),
            ("web_db", "local", "1 container"),
            ("0123456789ab… (anonymous)", "local", "Unused"),
        ]
    );
    assert!(i.images[0].detail.contains("Used by: web-nginx-1"));
    assert_eq!(i.disk.len(), 4);
}

#[test]
fn setup_keeps_context_registries_and_helper_but_no_logins() {
    let setup = collect().setup.join("\n");
    assert!(setup.contains("Context: desktop-linux"));
    assert!(setup.contains("Registries with saved logins: ghcr.io"));
    assert!(setup.contains("Credential helper: docker-credential-desktop"));
    assert!(!setup.contains("PRIVATE-"));
}

#[test]
fn private_values_never_reach_the_inventory_or_the_screen() {
    let i = collect();
    for r in i.containers.iter().chain(&i.images).chain(&i.volumes) {
        let text = format!("{} {} {} {}", r.name, r.state, r.status, r.detail);
        assert!(!text.contains("PRIVATE-"), "{text}");
    }
    let mut a = app();
    for (w, h) in [(80, 24), (120, 32), (160, 45)] {
        for _ in 0..3 {
            key(&mut a, KeyCode::Home);
            for _ in 0..a.docker.rows().len() {
                loop {
                    assert!(!draw(&mut a, w, h).contains("PRIVATE-"));
                    if a.docker.panel.offset >= a.docker.panel.max {
                        break;
                    }
                    key(&mut a, KeyCode::PageDown);
                }
                key(&mut a, KeyCode::Down);
            }
            key(&mut a, KeyCode::BackTab);
        }
    }
}

#[test]
fn docker_is_the_fourth_area_with_three_sections() {
    let mut a = app();
    let screen = draw(&mut a, 120, 32);
    assert!(screen.contains("Docker  › Containers"));
    assert!(screen.contains("3 containers · 1 running"));
    key(&mut a, KeyCode::BackTab);
    assert_eq!(a.docker.section, Section::Images);
    assert!(
        draw(&mut a, 120, 32)
            .contains("5 images · 857MB · 150MB (17%) reclaimable · build cache 1.1GB")
    );
    key(&mut a, KeyCode::Down);
    let selected = a.docker.selected_row().unwrap().key.clone();
    key(&mut a, KeyCode::BackTab);
    assert_eq!(a.destination(), Destination::Volumes);
    key(&mut a, KeyCode::BackTab);
    assert_eq!(a.destination(), Destination::Containers);
    key(&mut a, KeyCode::BackTab);
    for expected in [
        Module::Homebrew,
        Module::Agents,
        Module::Cloud,
        Module::Docker,
    ] {
        key(&mut a, KeyCode::Tab);
        assert_eq!(a.module, expected);
    }
    // Docker keeps its section and selection.
    assert_eq!(a.destination(), Destination::Images);
    assert_eq!(a.docker.selected_row().unwrap().key, selected);
    key(&mut a, KeyCode::Char('/'));
    for c in "unused".chars() {
        key(&mut a, KeyCode::Char(c));
    }
    key(&mut a, KeyCode::Enter);
    assert_eq!(a.docker.rows().len(), 2);
    key(&mut a, KeyCode::Char('q'));
    assert!(a.quit);
}

#[test]
fn unconfigured_failed_and_tiny_screens_render() {
    let mut a = App::new(
        Inventory {
            schema_version: 1,
            resources: vec![],
        },
        "snapshot",
    );
    a.navigate(Destination::Containers);
    assert!(draw(&mut a, 80, 24).contains("Docker not collected in this mode"));
    key(&mut a, KeyCode::Char('r'));
    assert!(!a.docker.requested);
    let mut a = app();
    a.docker.finish(Err(std::io::Error::other(
        "Docker daemon not reachable: start Docker, then press r",
    )));
    // A failure keeps the previous results and says why.
    assert_eq!(a.docker.rows().len(), 3);
    assert!(draw(&mut a, 80, 24).contains("Docker daemon not reachable"));
    for (w, h) in [(30, 10), (45, 20)] {
        draw(&mut a, w, h);
    }
}

#[test]
fn running_containers_are_green_even_when_selected() {
    use ratatui::style::Color;
    let mut a = app(); // web-nginx-1, the running container, is selected.
    let mut t = Terminal::new(TestBackend::new(120, 30)).unwrap();
    t.draw(|f| ui::render(f, &mut a)).unwrap();
    let buffer = t.backend().buffer();
    let fg = |word: &str| {
        (0..30)
            .find_map(|y| {
                let line: String = (0..120).map(|x| buffer[(x, y)].symbol()).collect();
                let x = line.find(word)?;
                Some(buffer[(x as u16, y)].fg)
            })
            .unwrap_or_else(|| panic!("{word}"))
    };
    let green = Color::Rgb(166, 227, 161); // The default (Mocha) theme.
    assert_eq!(fg("Running"), green);
    assert_ne!(fg("Stopped"), green);
}

#[test]
fn live_docker_refreshes_while_open_but_fixtures_do_not() {
    use den::docker::state::{REFRESH_EVERY, State};
    use std::time::{Duration, Instant};

    let mut live = State::default();
    live.configure(docker::Context::local());
    // Nothing is scheduled before the first run.
    live.requested = false;
    assert_eq!(live.next_refresh(Instant::now()), None);
    live.finish(Err(std::io::Error::other("Docker daemon not reachable")));
    let done = live.finished_at.unwrap();
    assert!(live.next_refresh(done).unwrap() <= REFRESH_EVERY);
    live.tick(done + Duration::from_secs(1));
    assert!(!live.requested);
    live.tick(done + REFRESH_EVERY);
    assert!(live.requested);
    // Once requested, nothing more is scheduled until that run finishes.
    assert_eq!(live.next_refresh(done + REFRESH_EVERY), None);

    let mut saved = State::default();
    saved.configure(docker::Context::fixture(std::path::Path::new("fixtures/docker")).unwrap());
    saved.requested = false;
    saved.finish(Err(std::io::Error::other("x")));
    assert_eq!(saved.next_refresh(Instant::now() + REFRESH_EVERY), None);
}
