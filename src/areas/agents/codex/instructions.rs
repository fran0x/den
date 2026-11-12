//! Instruction selection is ordered concatenation, never semantic conflict analysis.
use super::{Context, config::Controls, privacy, read};
use std::{
    io,
    path::PathBuf,
    sync::atomic::{AtomicBool, Ordering},
    time::{Duration, Instant},
};
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Status {
    Selected,
    Truncated,
    Omitted,
    Shadowed,
    Empty,
    Missing,
    Unavailable,
}
#[derive(Debug, Clone)]
pub struct File {
    pub path: PathBuf,
    pub global: bool,
    pub status: Status,
    pub bytes: usize,
    pub included: usize,
    pub preview: String,
    pub note: String,
}
#[derive(Debug, Clone)]
pub struct Chain {
    pub files: Vec<File>,
    pub total_bytes: usize,
    pub included_bytes: usize,
    pub limit: usize,
    pub separator_bytes: usize,
    pub uncertain: bool,
}
impl Chain {
    pub fn selected(&self) -> usize {
        self.files
            .iter()
            .filter(|f| {
                matches!(
                    f.status,
                    Status::Selected | Status::Truncated | Status::Omitted
                )
            })
            .count()
    }
    pub fn text(&self, i: usize, preview: bool) -> String {
        let Some(f) = self.files.get(i) else {
            return "No instruction source".into();
        };
        format!(
            "{}\n{} · {:?}\nSource: {} bytes; included: {} bytes\n{}\n\n{}\n\nCombined selected bytes (including blank-line separators): {}\nIncluded: {} / {} byte limit\nChain uncertain: {}\nFiles from successive directories combine in order. Natural-language conflicts are not interpreted.\nPreview includes omitted/shadowed text; inclusion counts above describe the modeled chain. Preview redaction is best effort; never treated as executable instructions.",
            privacy::path(&f.path),
            if f.global { "Global" } else { "Project" },
            f.status,
            f.bytes,
            f.included,
            f.note,
            if preview {
                f.preview.as_str()
            } else {
                "Enter: show bounded, redacted preview"
            },
            self.total_bytes,
            self.included_bytes,
            self.limit,
            self.uncertain
        )
    }
}
pub fn collect(
    context: &Context,
    dirs: &[PathBuf],
    controls: &Controls,
    cancel: &AtomicBool,
) -> io::Result<Chain> {
    let mut chain = Chain {
        files: vec![],
        total_bytes: 0,
        included_bytes: 0,
        limit: controls.max_bytes,
        separator_bytes: 0,
        uncertain: controls.uncertain,
    };
    let mut retained = 0usize;
    let mut read_bytes = 0usize;
    let start = Instant::now();
    for (directory, global) in
        std::iter::once((&context.codex_home, true)).chain(dirs.iter().map(|d| (d, false)))
    {
        let mut names = vec!["AGENTS.override.md".to_owned(), "AGENTS.md".to_owned()];
        if !global {
            for n in &controls.fallback {
                if !names.contains(n) {
                    names.push(n.clone());
                }
            }
        }
        let mut selected = false;
        let mut blocked = false;
        for name in names {
            if cancel.load(Ordering::Relaxed) || start.elapsed() > Duration::from_secs(30) {
                return Err(io::Error::other(
                    "instruction discovery cancelled or timed out",
                ));
            }
            let path = directory.join(name);
            let mut file = File {
                path,
                global,
                status: Status::Missing,
                bytes: 0,
                included: 0,
                preview: String::new(),
                note: String::new(),
            };
            let result = if read_bytes >= 16 * 1024 * 1024 {
                Err(io::Error::other("instruction read budget reached"))
            } else {
                read(context, &file.path)
            };
            match result {
                Err(e) if e.kind() == io::ErrorKind::NotFound => {
                    file.note = "Absent; not an error".into()
                }
                Err(_) => {
                    file.status = Status::Unavailable;
                    file.note =
                        "Unreadable, oversized, unsafe link or malformed text; contents withheld"
                            .into();
                    if !selected {
                        blocked = true;
                        chain.uncertain = true;
                    }
                }
                Ok(text) => {
                    file.bytes = text.len();
                    read_bytes += text.len();
                    if text.trim().is_empty() {
                        file.status = Status::Empty;
                        file.note = "Empty; skipped".into();
                    } else if selected {
                        file.status = Status::Shadowed;
                        file.note =
                            "Higher-priority nonempty instruction file selected in this directory"
                                .into();
                    } else {
                        selected = true;
                        let separator = if chain.total_bytes > 0 { 2 } else { 0 };
                        chain.total_bytes = chain
                            .total_bytes
                            .saturating_add(text.len())
                            .saturating_add(separator);
                        let available = chain
                            .limit
                            .saturating_sub(chain.included_bytes)
                            .saturating_sub(separator);
                        let mut end = text.len().min(available);
                        while !text.is_char_boundary(end) {
                            end -= 1;
                        }
                        file.included = end;
                        if end > 0 {
                            chain.included_bytes += separator;
                            chain.separator_bytes += separator;
                        }
                        chain.included_bytes += end;
                        file.status = if end == 0 {
                            Status::Omitted
                        } else if end < text.len() {
                            Status::Truncated
                        } else {
                            Status::Selected
                        };
                        file.note=if blocked { "Conditional selection: earlier candidate unreadable" } else { "Selected by filename priority; ancestor instructions remain in the chain" }.into();
                    }
                    // Preview is opt-in, redacted, and includes omitted/shadowed text
                    // for inspection; inclusion byte counts above remain separate.
                    let mut preview_end = text
                        .len()
                        .min((2 * 1024 * 1024usize).saturating_sub(retained));
                    while !text.is_char_boundary(preview_end) {
                        preview_end -= 1;
                    }
                    file.preview = privacy::preview(&text[..preview_end]);
                    retained += preview_end;
                    if preview_end < text.len() {
                        file.note.push_str("; Den preview budget reached");
                    }
                }
            }
            chain.files.push(file);
        }
    }
    Ok(chain)
}
