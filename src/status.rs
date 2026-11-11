//! A row's state in Den's shared words, and what follows it, such as
//! `Problem · cluster 'gone' is not defined`. Counting and colouring use the kind, never
//! the text, so a typo cannot turn a problem into something else.
use std::fmt;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Kind {
    Ready,
    Problem,
    Off,
    /// Present, but nothing selects it (a toolchain that is not the default).
    Installed,
}
impl Kind {
    pub fn label(self) -> &'static str {
        match self {
            Self::Ready => "Ready",
            Self::Problem => "Problem",
            Self::Off => "Off",
            Self::Installed => "Installed",
        }
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Status {
    pub kind: Kind,
    pub detail: String,
}
impl Status {
    pub fn new(kind: Kind, detail: impl Into<String>) -> Self {
        Self {
            kind,
            detail: detail.into(),
        }
    }
    pub fn ready(detail: impl Into<String>) -> Self {
        Self::new(Kind::Ready, detail)
    }
    pub fn problem(detail: impl Into<String>) -> Self {
        Self::new(Kind::Problem, detail)
    }
    pub fn off(detail: impl Into<String>) -> Self {
        Self::new(Kind::Off, detail)
    }
    pub fn installed() -> Self {
        Self::new(Kind::Installed, "")
    }
    pub fn is_problem(&self) -> bool {
        self.kind == Kind::Problem
    }
}
impl fmt::Display for Status {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        if self.detail.is_empty() {
            f.write_str(self.kind.label())
        } else {
            write!(f, "{} · {}", self.kind.label(), self.detail)
        }
    }
}
/// Compares the displayed text, as tests and searches see it.
impl PartialEq<&str> for Status {
    fn eq(&self, other: &&str) -> bool {
        let label = self.kind.label();
        if self.detail.is_empty() {
            *other == label
        } else {
            other
                .strip_prefix(label)
                .and_then(|r| r.strip_prefix(" · "))
                == Some(self.detail.as_str())
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn displays_as_kind_then_detail() {
        assert_eq!(
            Status::problem("x is missing").to_string(),
            "Problem · x is missing"
        );
        assert_eq!(Status::installed().to_string(), "Installed");
        assert!(Status::problem("y").is_problem() && !Status::ready("y").is_problem());
        assert!(Status::ready("default") == "Ready · default");
        assert!(Status::installed() == "Installed");
        assert!(Status::ready("default") != "Ready · defaults");
        assert!(Status::off("x") != "Ready · x");
    }
}
