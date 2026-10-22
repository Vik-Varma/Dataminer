//! Release version numbers and picking a git tag for a target version.
//!
//! Both Mojang repositories tag releases `v<version>`: `bedrock-protocol-docs`
//! as `v1.26.51`, `bedrock-samples` with a build number as `v1.26.50.4` (and
//! sometimes zero-padded, `v1.26.40.05`). Preview tags carry a suffix and are
//! never picked here.

use std::fmt;

/// A dotted numeric release version with at least three parts.
#[derive(Clone, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub struct Version(Vec<u32>);

impl Version {
    /// Parse `1.26.51`, `v1.26.50.4` or `v1.26.40.05`. Anything else, including
    /// preview tags, is `None`.
    pub fn parse(s: &str) -> Option<Self> {
        let s = s.strip_prefix('v').unwrap_or(s);
        let parts = s
            .split('.')
            .map(|p| p.parse().ok())
            .collect::<Option<Vec<u32>>>()?;
        (parts.len() >= 3).then_some(Self(parts))
    }

    /// The game version without the build number: `1.26.50.4` → `1.26.50`.
    pub fn game(&self) -> Version {
        Self(self.0[..3].to_vec())
    }

    /// The release train: hotfixes `1.26.50`, `1.26.51` and `1.26.52` share one.
    fn train(&self) -> (u32, u32, u32) {
        (self.0[0], self.0[1], self.0[2] / 10)
    }
}

impl fmt::Display for Version {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let parts: Vec<String> = self.0.iter().map(u32::to_string).collect();
        f.write_str(&parts.join("."))
    }
}

/// How a tag was chosen for the target.
#[derive(Debug, PartialEq, Eq)]
pub enum Pick {
    /// The tag is the target's game version (any build of it).
    Exact(String),
    /// No tag for the target; this is the newest earlier tag in the same
    /// release train. It needs evidence (a matching protocol) before use.
    SameTrain(String),
    /// Nothing usable. Holds the nearest release tags for the error message.
    None(Vec<String>),
}

/// Pick the tag for `target` from `tags`, preferring the highest build.
pub fn pick(tags: &[String], target: &Version) -> Pick {
    let release: Vec<(Version, &String)> = tags
        .iter()
        .filter_map(|t| Version::parse(t).map(|v| (v, t)))
        .collect();
    let target_game = target.game();

    let exact = release
        .iter()
        .filter(|(v, _)| {
            if target.0.len() > 3 {
                v == target
            } else {
                v.game() == target_game
            }
        })
        .max_by(|a, b| a.0.cmp(&b.0));
    if let Some((_, t)) = exact {
        return Pick::Exact((*t).clone());
    }

    let same_train = release
        .iter()
        .filter(|(v, _)| v.train() == target.train() && v.game() < target_game)
        .max_by(|a, b| a.0.cmp(&b.0));
    if let Some((_, t)) = same_train {
        return Pick::SameTrain((*t).clone());
    }

    let below = release
        .iter()
        .filter(|(v, _)| v < target)
        .max_by(|a, b| a.0.cmp(&b.0));
    let above = release
        .iter()
        .filter(|(v, _)| v > target)
        .min_by(|a, b| a.0.cmp(&b.0));
    Pick::None(
        below
            .into_iter()
            .chain(above)
            .map(|(_, t)| (*t).clone())
            .collect(),
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    fn tags(list: &[&str]) -> Vec<String> {
        list.iter().map(|s| s.to_string()).collect()
    }

    fn v(s: &str) -> Version {
        Version::parse(s).unwrap()
    }

    #[test]
    fn parses_release_tags_only() {
        assert_eq!(v("v1.26.40.05"), v("1.26.40.5"));
        assert_eq!(v("v1.26.50.4").game(), v("1.26.50"));
        assert!(Version::parse("v1.26.50-preview.22").is_none());
        assert!(Version::parse("v1.26.50.27-preview").is_none());
        assert!(Version::parse("1.26").is_none());
    }

    #[test]
    fn orders_numerically() {
        assert!(v("1.26.100") > v("1.26.99"));
        assert!(v("1.26.50.10") > v("1.26.50.4"));
    }

    #[test]
    fn exact_prefers_highest_build() {
        let t = tags(&["v1.26.50", "v1.26.50.4", "v1.26.50.27-preview", "v1.26.51"]);
        assert_eq!(pick(&t, &v("1.26.50")), Pick::Exact("v1.26.50.4".into()));
        assert_eq!(pick(&t, &v("1.26.51")), Pick::Exact("v1.26.51".into()));
        assert_eq!(pick(&t, &v("1.26.50.4")), Pick::Exact("v1.26.50.4".into()));
    }

    #[test]
    fn falls_back_within_the_train_only() {
        let t = tags(&["v1.26.40.05", "v1.26.50.4", "v1.26.60-preview.21"]);
        assert_eq!(
            pick(&t, &v("1.26.51")),
            Pick::SameTrain("v1.26.50.4".into())
        );
        assert_eq!(
            pick(&t, &v("1.26.60")),
            Pick::None(vec!["v1.26.50.4".into()])
        );
        assert_eq!(
            pick(&t, &v("1.26.45")),
            Pick::SameTrain("v1.26.40.05".into())
        );
    }

    #[test]
    fn never_falls_forward() {
        let t = tags(&["v1.26.51", "v1.26.60"]);
        assert_eq!(pick(&t, &v("1.26.50")), Pick::None(vec!["v1.26.51".into()]));
    }
}
