// Integration tests: unwrap and expect report failures here.
#![allow(clippy::unwrap_used, clippy::expect_used)]
//! Where a skill came from: the `npx skills` lock file, the author its SKILL.md names, and
//! the git repository that keeps its folder. The layout mirrors a common setup: skills kept
//! in a dotfiles repository, linked from `~/.agents/skills` and `~/.claude/skills`.
use crossterm::event::{KeyCode, KeyEvent, KeyModifiers};
use den::{
    app::App,
    areas::agents::{
        claude,
        skills::{Scope, Skill},
    },
    inventory::Inventory,
    online::{Answer, Package},
    tui::{presentation::Destination, ui},
};
use ratatui::{Terminal, backend::TestBackend};
use std::{
    fs,
    os::unix::fs::symlink,
    path::{Path, PathBuf},
    sync::atomic::{AtomicBool, AtomicU64, Ordering},
};

static NEXT: AtomicU64 = AtomicU64::new(0);

struct Temp(PathBuf);
impl Temp {
    fn new() -> Self {
        let p = std::env::temp_dir().join(format!(
            "den-skill-origin-{}-{}",
            std::process::id(),
            NEXT.fetch_add(1, Ordering::Relaxed)
        ));
        fs::create_dir_all(p.join("project")).unwrap();
        Self(p)
    }
    fn home(&self) -> PathBuf {
        self.0.join("home")
    }
    fn skill(dir: &Path, name: &str, extra: &str) {
        fs::create_dir_all(dir.join(name)).unwrap();
        fs::write(
            dir.join(name).join("SKILL.md"),
            format!("---\nname: {name}\ndescription: Test skill {name}.\n{extra}---\nBody\n"),
        )
        .unwrap();
    }
    fn git(dir: &Path, config: &str) {
        fs::create_dir_all(dir.join(".git")).unwrap();
        fs::write(dir.join(".git/config"), config).unwrap();
    }
    /// ~/dotfiles/agents/skills holds the skills; ~/.agents/skills and each
    /// ~/.claude/skills/<name> link to it, and the lock file is linked from there too.
    fn dotfiles(&self, lock: &str) {
        let home = self.home();
        let dotfiles = home.join("dotfiles");
        let kept = dotfiles.join("agents/skills");
        Self::git(
            &dotfiles,
            "[core]\n\tbare = false\n[remote \"origin\"]\n\turl = https://fran0x:ghp_secret@github.com/fran0x/dotfiles.git\n",
        );
        Self::skill(&kept, "found", "");
        Self::skill(&kept, "mine", "metadata:\n  author: fran0x\n");
        Self::skill(&kept, "plain", "");
        fs::write(dotfiles.join("agents/.skill-lock.json"), lock).unwrap();
        fs::create_dir_all(home.join(".agents")).unwrap();
        symlink(&kept, home.join(".agents/skills")).unwrap();
        symlink(
            dotfiles.join("agents/.skill-lock.json"),
            home.join(".agents/.skill-lock.json"),
        )
        .unwrap();
        let claude = home.join(".claude/skills");
        fs::create_dir_all(&claude).unwrap();
        for name in ["found", "mine", "plain"] {
            symlink(home.join(".agents/skills").join(name), claude.join(name)).unwrap();
        }
    }
    fn collect(&self) -> claude::Inventory {
        let context = claude::Context::fixture(&self.0).unwrap();
        claude::discover(&context, &AtomicBool::new(false)).unwrap()
    }
}
impl Drop for Temp {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.0);
    }
}
fn skill<'a>(i: &'a claude::Inventory, name: &str) -> &'a Skill {
    i.skills
        .skills
        .iter()
        .find(|s| s.name == name)
        .unwrap_or_else(|| panic!("{name} not listed"))
}
const LOCK: &str = r#"{"version":3,"skills":{
  "found":{"source":"vercel-labs/skills","sourceType":"github","sourceUrl":"https://github.com/vercel-labs/skills.git","skillPath":"skills/found/SKILL.md","skillFolderHash":"76a98a285cb0434f3d39e1a873823556330e398b","installedAt":"2026-08-16T16:29:19.613Z"},
  "elsewhere":{"source":"/Users/someone/private/skills","sourceType":"local","installedAt":"2026-09-01T00:00:00Z"}
},"dismissed":{}}"#;

#[test]
fn origin_comes_from_the_lock_file_the_author_and_the_repository() {
    let t = Temp::new();
    t.dotfiles(LOCK);
    let i = t.collect();

    let found = skill(&i, "found");
    assert_eq!(
        found.origin.installed.as_deref(),
        Some("GitHub vercel-labs/skills, on 2026-08-16")
    );
    let lookup = found.origin.online.as_ref().expect("looked up on GitHub");
    assert_eq!(
        lookup.package.key(),
        "GitHub:vercel-labs/skills/skills/found"
    );
    assert_eq!(lookup.installed, "76a98a285cb0434f3d39e1a873823556330e398b");
    let mine = skill(&i, "mine");
    assert_eq!(mine.author.as_deref(), Some("fran0x"));
    assert_eq!(mine.origin.installed, None, "not in the lock file");
    assert!(mine.origin.online.is_none(), "nothing to ask GitHub about");
    let plain = skill(&i, "plain");
    assert_eq!(
        (plain.author.as_deref(), &plain.origin.installed),
        (None, &None)
    );

    // All three are kept in the dotfiles repository; its remote loses the token.
    for s in [found, mine, plain] {
        let repo = s.origin.repo.as_ref().expect("in a git repository");
        assert!(repo.folder.ends_with("home/dotfiles"), "{:?}", repo.folder);
        assert_eq!(repo.remote.as_deref(), Some("github.com/fran0x/dotfiles"));
    }
    let shown = format!("{:?}", i.skills.skills);
    assert!(!shown.contains("ghp_secret") && !shown.contains("someone/private"));
    assert!(i.skills.coverage.iter().any(|c| c.starts_with("Origin:")));
}

#[test]
fn a_same_named_skill_elsewhere_is_not_the_locked_install() {
    let t = Temp::new();
    t.dotfiles(LOCK);
    // A project skill named like the locked one, in a project repository.
    Temp::git(
        &t.0.join("project"),
        "[remote \"origin\"]\n\turl = git@gitlab.com:team/app.git\n",
    );
    Temp::skill(
        &t.0.join("project/.claude/skills"),
        "found",
        "author: someone\n",
    );
    let i = t.collect();
    let project = i
        .skills
        .skills
        .iter()
        .find(|s| s.name == "found" && s.scope == Scope::Project)
        .unwrap();
    assert_eq!(project.origin.installed, None);
    assert_eq!(
        project.author.as_deref(),
        Some("someone"),
        "top-level author too"
    );
    assert_eq!(
        project
            .origin
            .repo
            .as_ref()
            .and_then(|r| r.remote.as_deref()),
        Some("gitlab.com/team/app")
    );
}

#[test]
fn a_missing_or_malformed_lock_file_gives_no_install_record() {
    for lock in [None, Some("not json"), Some(r#"{"skills":["found"]}"#)] {
        let t = Temp::new();
        t.dotfiles(LOCK);
        let lock_file = t.home().join("dotfiles/agents/.skill-lock.json");
        match lock {
            None => fs::remove_file(&lock_file).unwrap(),
            Some(text) => fs::write(&lock_file, text).unwrap(),
        }
        let i = t.collect();
        assert_eq!(skill(&i, "found").origin.installed, None, "{lock:?}");
        assert!(skill(&i, "found").origin.repo.is_some());
    }
}

#[test]
fn a_skill_that_is_its_own_clone_and_one_outside_any_repository() {
    let t = Temp::new();
    let claude = t.home().join(".claude/skills");
    Temp::skill(&claude, "cloned", "");
    Temp::git(
        &claude.join("cloned"),
        "[remote \"origin\"]\n\turl = git@github.com:someone/cloned.git\n",
    );
    Temp::skill(&claude, "loose", "");
    let i = t.collect();
    let cloned = skill(&i, "cloned");
    let repo = cloned.origin.repo.as_ref().unwrap();
    assert_eq!(Some(&repo.folder), cloned.resolved.as_ref());
    assert_eq!(repo.remote.as_deref(), Some("github.com/someone/cloned"));
    assert_eq!(
        skill(&i, "loose").origin.repo,
        None,
        "no repository above it up to home"
    );
}

fn draw(a: &mut App) -> String {
    let (w, h) = (160, 50);
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

#[test]
fn u_asks_github_whether_an_installed_skill_changed_there() {
    let t = Temp::new();
    t.dotfiles(LOCK);
    let mut a = App::new(
        Inventory {
            schema_version: 1,
            resources: vec![],
        },
        "skill origin test",
    );
    a.online.use_file(t.0.join("state/online.json"));
    a.claude.configure(claude::Context::fixture(&t.0).unwrap());
    a.claude.finish(Ok(t.collect()));
    a.claude.requested = false;
    a.navigate(Destination::Skills);
    let key = a
        .agents
        .rows(&a.agent_sources())
        .into_iter()
        .find(|r| r.name == "found")
        .unwrap()
        .key;
    a.agents_select(&key);
    assert!(draw(&mut a).contains("GitHub: not checked yet; u compares it with GitHub"));

    a.handle_key(KeyEvent::new(KeyCode::Char('U'), KeyModifiers::NONE));
    assert!(
        draw(&mut a).contains("to api.github.com"),
        "names the host first"
    );
    a.handle_key(KeyEvent::new(KeyCode::Enter, KeyModifiers::NONE));
    let asked: Vec<Package> = a.online.request.take().unwrap();
    // Only the skill npx skills installed from GitHub; never the user's own.
    assert_eq!(
        asked.iter().map(Package::key).collect::<Vec<_>>(),
        ["GitHub:vercel-labs/skills/skills/found"]
    );
    a.finish_online(
        Ok(vec![(
            asked[0].clone(),
            Answer::Latest("0123456789abcdef0123456789abcdef01234567".into()),
        )]),
        std::time::SystemTime::now(),
    );
    a.handle_key(KeyEvent::new(KeyCode::Down, KeyModifiers::NONE));
    a.handle_key(KeyEvent::new(KeyCode::Up, KeyModifiers::NONE));
    let screen = draw(&mut a);
    assert!(screen.contains("↑ newer on GitHub"), "{screen}");
    assert!(
        screen.contains("Changed on GitHub since it was installed (0123456789ab)"),
        "{screen}"
    );
    assert!(screen.contains("1 newer online"), "{screen}");
}
