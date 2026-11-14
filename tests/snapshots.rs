// Integration tests: unwrap and expect report failures here.
#![allow(clippy::unwrap_used, clippy::expect_used)]
//! The snapshot saved with `S`: asked before writing, private, never followed through a
//! link, compared with on the next run, and left alone when Den cannot use it.
use crossterm::event::{KeyCode, KeyEvent, KeyModifiers};
use den::{
    app::App,
    areas::toolchains,
    inventory::Inventory,
    snapshot,
    tui::{presentation::Destination, ui},
};
use ratatui::{Terminal, backend::TestBackend};
use std::{
    fs,
    path::{Path, PathBuf},
    sync::atomic::{AtomicBool, AtomicU64, Ordering},
};

static NEXT: AtomicU64 = AtomicU64::new(0);
struct Temp(PathBuf);
impl Temp {
    fn new() -> Self {
        let p = std::env::temp_dir().join(format!(
            "den-snapshots-{}-{}",
            std::process::id(),
            NEXT.fetch_add(1, Ordering::Relaxed)
        ));
        fs::create_dir_all(&p).unwrap();
        Self(p)
    }
    /// The snapshot, in a folder Den creates.
    fn file(&self) -> PathBuf {
        self.0.join("state/den/snapshot.json")
    }
}
impl Drop for Temp {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.0);
    }
}

fn key(a: &mut App, code: KeyCode) {
    a.handle_key(KeyEvent::new(code, KeyModifiers::NONE));
}
fn draw(a: &mut App) -> String {
    let (w, h) = (160, 34);
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
fn read() -> toolchains::Inventory {
    let context = toolchains::Context::fixture(Path::new("fixtures/toolchains")).unwrap();
    toolchains::discover(&context, &AtomicBool::new(false)).unwrap()
}
/// A session that has read Toolchains once, with the snapshot at `path`.
fn session(path: Option<&Path>, first: toolchains::Inventory) -> App {
    let mut a = App::new(
        Inventory {
            schema_version: 1,
            resources: vec![],
        },
        "snapshot test",
    );
    if let Some(path) = path {
        a.use_snapshot(path.to_owned());
    }
    let context = toolchains::Context::fixture(Path::new("fixtures/toolchains")).unwrap();
    a.toolchains.configure(context);
    a.toolchains.finish(Ok(first));
    a.toolchains.requested = false;
    a.navigate(Destination::Rust);
    a
}
fn save(a: &mut App) -> String {
    key(a, KeyCode::Char('S'));
    let screen = draw(a);
    assert!(
        screen.contains("Save a snapshot? Enter:save Esc:cancel"),
        "{screen}"
    );
    key(a, KeyCode::Enter);
    draw(a)
}

#[test]
fn a_saved_snapshot_is_compared_with_on_the_next_run() {
    let t = Temp::new();
    let mut a = session(Some(&t.file()), read());
    let screen = save(&mut a);
    assert!(screen.contains("Saved a snapshot of 1 area to"), "{screen}");
    let file = snapshot::load(&t.file()).unwrap().unwrap();
    assert_eq!(file.version, snapshot::FILE_VERSION);
    assert_eq!(file.areas.keys().collect::<Vec<_>>(), ["Toolchains"]);
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        let mode = |p: &Path| fs::metadata(p).unwrap().permissions().mode() & 0o777;
        assert_eq!(mode(&t.file()), 0o600);
        assert_eq!(mode(t.file().parent().unwrap()), 0o700);
    }

    // Next run: a crate is gone before Den starts, so only the snapshot knows it.
    let mut now = read();
    let gone = now.rust.pop().unwrap();
    let mut b = session(Some(&t.file()), now);
    let screen = draw(&mut b);
    assert!(screen.contains("1 change since the snapshot"), "{screen}");
    assert!(!screen.contains("since Den started"), "{screen}");
    // d: since the snapshot, then since Den started, then every row.
    key(&mut b, KeyCode::Char('d'));
    let screen = draw(&mut b);
    assert!(
        screen.contains("Changes 1 · since the snapshot just now"),
        "{screen}"
    );
    assert!(screen.contains(&format!("− {}", gone.name)), "{screen}");
    key(&mut b, KeyCode::Char('d'));
    let screen = draw(&mut b);
    assert!(screen.contains("Changes 0 · since Den started"), "{screen}");
    key(&mut b, KeyCode::Char('d'));
    assert!(!draw(&mut b).contains("Changes "));

    // Saving again makes this reading the snapshot.
    key(&mut b, KeyCode::Char('d'));
    save(&mut b);
    assert!(draw(&mut b).contains("Changes 0 · since the snapshot just now"));
}

#[test]
fn esc_cancels_and_nothing_is_written() {
    let t = Temp::new();
    let mut a = session(Some(&t.file()), read());
    key(&mut a, KeyCode::Char('S'));
    key(&mut a, KeyCode::Esc);
    assert!(draw(&mut a).contains("Snapshot not saved"));
    assert!(!t.file().exists());
    assert!(!t.file().parent().unwrap().exists(), "not even the folder");
}

#[test]
fn only_live_data_is_saved() {
    let mut a = session(None, read());
    key(&mut a, KeyCode::Char('S'));
    let screen = draw(&mut a);
    assert!(
        screen.contains("Snapshots are saved from live data only"),
        "{screen}"
    );
    assert!(!a.save_prompt);
}

#[test]
fn nothing_to_save_before_any_area_is_read() {
    let t = Temp::new();
    let mut a = App::new(
        Inventory {
            schema_version: 1,
            resources: vec![],
        },
        "snapshot test",
    );
    a.use_snapshot(t.file());
    key(&mut a, KeyCode::Char('S'));
    assert!(draw(&mut a).contains("Nothing to save yet"));
    assert!(!t.file().exists());
}

#[test]
fn areas_not_read_keep_what_the_file_had() {
    let t = Temp::new();
    fs::create_dir_all(t.file().parent().unwrap()).unwrap();
    let docker = r#"{"version":1,"areas":{"Docker":{"taken":1700000000,"entries":[
        {"section":"Volumes","key":"volume:data","name":"data","value":"local"}]}}}"#;
    fs::write(t.file(), docker).unwrap();
    let mut a = session(Some(&t.file()), read());
    save(&mut a);
    let file = snapshot::load(&t.file()).unwrap().unwrap();
    assert_eq!(
        file.areas.keys().collect::<Vec<_>>(),
        ["Docker", "Toolchains"]
    );
    assert_eq!(file.areas["Docker"].taken, 1_700_000_000);
    assert_eq!(file.areas["Docker"].entries[0].key, "volume:data");
}

fn unusable(content: &[u8], why: &str) {
    let t = Temp::new();
    fs::create_dir_all(t.file().parent().unwrap()).unwrap();
    fs::write(t.file(), content).unwrap();
    let mut a = session(Some(&t.file()), read());
    let screen = draw(&mut a);
    assert!(
        screen.contains(&format!("Saved snapshot not used: {why}")),
        "{screen}"
    );
    // Nothing to compare with: d goes straight to since Den started.
    key(&mut a, KeyCode::Char('d'));
    assert!(draw(&mut a).contains("since Den started"));
    assert_eq!(fs::read(t.file()).unwrap(), content, "left as it was");
}

#[test]
fn a_file_den_cannot_read_is_reported_and_left_alone() {
    unusable(b"{not json", "it is not a Den snapshot");
    unusable(br#"{"version":0,"areas":{}}"#, "it is not a Den snapshot");
    unusable(
        br#"{"version":1,"areas":{},"extra":true}"#,
        "it is not a Den snapshot",
    );
    unusable(
        br#"{"version":2,"areas":{}}"#,
        "it was saved by a newer Den",
    );
}

#[test]
fn a_file_over_4_mib_is_not_read() {
    let t = Temp::new();
    fs::create_dir_all(t.file().parent().unwrap()).unwrap();
    fs::write(t.file(), vec![b' '; (4 << 20) + 1]).unwrap();
    assert_eq!(
        snapshot::load(&t.file()).unwrap_err(),
        "it is larger than 4 MiB"
    );
}

#[cfg(unix)]
#[test]
fn a_link_is_never_followed_or_replaced() {
    let t = Temp::new();
    let target = t.0.join("elsewhere.json");
    fs::write(&target, br#"{"version":1,"areas":{}}"#).unwrap();
    fs::create_dir_all(t.file().parent().unwrap()).unwrap();
    std::os::unix::fs::symlink(&target, t.file()).unwrap();
    let mut a = session(Some(&t.file()), read());
    assert!(draw(&mut a).contains("Saved snapshot not used: it is a link"));
    let screen = save(&mut a);
    assert!(
        screen.contains("Snapshot not saved: it is a link"),
        "{screen}"
    );
    assert!(
        fs::symlink_metadata(t.file())
            .unwrap()
            .file_type()
            .is_symlink()
    );
    assert_eq!(
        fs::read(&target).unwrap(),
        br#"{"version":1,"areas":{}}"#,
        "the target is untouched"
    );
    // No temporary file is left behind.
    let left: Vec<_> = fs::read_dir(t.file().parent().unwrap())
        .unwrap()
        .map(|e| e.unwrap().file_name())
        .collect();
    assert_eq!(left.len(), 1, "{left:?}");
}
