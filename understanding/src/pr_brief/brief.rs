//! The brief: what the writer returns, as the window shows it.

use serde::{Deserialize, Serialize};
/// A decision the agent declared, for Daniel to accept or overturn. The writer may give a bare sentence.
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
#[serde(from = "RawDecision")]
pub struct Decision {
    /// Assigned by code at the first write.
    pub id: Option<String>,
    pub text: String,
    #[serde(default)]
    pub flags: Vec<String>,
}

#[derive(Deserialize)]
#[serde(untagged)]
enum RawDecision {
    Text(String),
    Full {
        #[serde(default)]
        id: Option<String>,
        #[serde(default)]
        text: String,
        #[serde(default)]
        flags: Vec<String>,
    },
}

impl From<RawDecision> for Decision {
    fn from(raw: RawDecision) -> Self {
        match raw {
            RawDecision::Text(text) => Decision {
                id: None,
                text,
                flags: vec![],
            },
            RawDecision::Full { id, text, flags } => Decision { id, text, flags },
        }
    }
}

#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
pub struct Cite {
    #[serde(default)]
    pub from: String,
    #[serde(default)]
    pub quote: String,
}

#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
pub struct RiskItem {
    #[serde(default)]
    pub part: String,
    #[serde(default)]
    pub trigger: String,
    #[serde(default)]
    pub check: String,
}

/// A link of a light's path and whether it runs backwards.
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
pub struct Hop(pub String, #[serde(default)] pub bool);

/// A figure's item; the kinds use the fields they need.
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
pub struct FigItem {
    #[serde(default)]
    pub label: String,
    #[serde(default)]
    pub tone: Option<String>,
    #[serde(default)]
    pub bad: Option<bool>,
}

#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
pub struct Figure {
    #[serde(default)]
    pub kind: String,
    #[serde(default)]
    pub frame: Option<String>,
    #[serde(default)]
    pub items: Vec<FigItem>,
    #[serde(default)]
    pub before: Vec<String>,
    #[serde(default)]
    pub after: Vec<String>,
}

/// The one check of a brief: `pick` (a case inside one part) or `order` (the parts of a flow, in order).
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
pub struct BriefCheck {
    #[serde(default)]
    pub kind: String,
    #[serde(default)]
    pub q: String,
    #[serde(default)]
    pub opts: Vec<String>,
    /// A pick's index, or an order's part ids.
    #[serde(default)]
    pub key: serde_json::Value,
    #[serde(default)]
    pub yes: String,
    #[serde(default)]
    pub no: String,
    #[serde(default)]
    pub flow: Option<String>,
    #[serde(default)]
    pub test: Option<String>,
}

#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
pub struct Step {
    /// Where, Before, Now, Must stay true, Could break, Not in the plan or Your turn.
    #[serde(default)]
    pub k: String,
    #[serde(default)]
    pub h: String,
    #[serde(default)]
    pub b: String,
    #[serde(default)]
    pub on: Vec<String>,
    #[serde(default)]
    pub hops: Vec<Hop>,
    #[serde(default)]
    pub law: Option<String>,
    /// The tagged test, changed in this PR, that covers the step's law.
    #[serde(default)]
    pub covered_by: Option<String>,
    #[serde(default)]
    pub list: Vec<String>,
    #[serde(default)]
    pub risks: Vec<RiskItem>,
    #[serde(default)]
    pub fig: Option<Figure>,
    #[serde(default)]
    pub check: Option<BriefCheck>,
    #[serde(default)]
    pub cites: Vec<Cite>,
    /// What the code checks (and, later, the claim check) found wrong with this step.
    #[serde(default)]
    pub flags: Vec<String>,
}

#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
pub struct Chapter {
    #[serde(default)]
    pub behaviour: String,
    #[serde(default)]
    pub parts: Vec<String>,
    #[serde(default)]
    pub title: String,
    #[serde(default)]
    pub steps: Vec<Step>,
    #[serde(default)]
    pub flags: Vec<String>,
    /// The chapter's story in one sentence (#397), on a chapter that lights a Core part only; Daniel's explain-back is compared with
    /// it. Empty on any other chapter, and in a brief written before it existed.
    #[serde(default)]
    pub gist: String,
    /// What the code checks and the claim check found wrong with the gist; a retry can rewrite it.
    #[serde(default)]
    pub gist_flags: Vec<String>,
}

#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
pub struct Also {
    #[serde(default)]
    pub part: String,
    #[serde(default)]
    pub line: String,
    #[serde(default)]
    pub flags: Vec<String>,
}

/// One of the PR's changed files, as Also touched sorts it (#477, "Also touched by kind").
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
pub struct TouchedFile {
    pub path: String,
    /// Lines added and removed.
    #[serde(default)]
    pub lines: usize,
    /// The part that owns it, none when it is ignored or owned by no part.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub part: Option<String>,
    /// The map ignores it. The page tags it "ignored" in the Tests, Config/build and Docs groups.
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    pub ignored: bool,
    /// The laws its `@law:` tags name, sorted, each once.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub laws: Vec<String>,
}

/// Also touched's groups, in the order they show; a file goes in the first that matches.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum AlsoKind {
    /// Owned by no part and not ignored: the PR's map check failed.
    #[default]
    NoPart,
    Tests,
    Config,
    Docs,
    /// Ignored, and of none of the kinds above.
    Ignored,
    Other,
}

impl AlsoKind {
    /// The group's name, lowercase.
    pub fn label(self) -> &'static str {
        match self {
            AlsoKind::NoPart => "no part",
            AlsoKind::Tests => "tests",
            AlsoKind::Config => "config/build",
            AlsoKind::Docs => "docs",
            AlsoKind::Ignored => "ignored",
            AlsoKind::Other => "other",
        }
    }
}

/// One group of Also touched, its files sorted by path.
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
pub struct AlsoGroup {
    pub kind: AlsoKind,
    pub files: Vec<TouchedFile>,
    /// The files' lines added and removed, together.
    pub lines: usize,
}

#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
pub struct Brief {
    #[serde(default)]
    pub title: String,
    #[serde(default)]
    pub overview: Vec<Step>,
    #[serde(default)]
    pub chapters: Vec<Chapter>,
    /// The model's lines on touched, unlit parts, retired by #477: read from an older brief, never written now.
    #[serde(default)]
    pub also: Vec<Also>,
    /// Also touched (#477): the PR's changed files that belong to no part a chapter lights, sorted by code into groups.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub also_groups: Vec<AlsoGroup>,
    #[serde(default)]
    pub unseen: Vec<String>,
    #[serde(default)]
    pub decisions: Vec<Decision>,
    /// Problems of the brief as a whole (chapter count, a required part no chapter lights).
    #[serde(default)]
    pub flags: Vec<String>,
}

impl Brief {
    /// Every step, overview first.
    pub fn steps(&self) -> impl Iterator<Item = &Step> {
        self.overview
            .iter()
            .chain(self.chapters.iter().flat_map(|c| &c.steps))
    }

    pub(crate) fn steps_mut(&mut self) -> impl Iterator<Item = &mut Step> {
        self.overview
            .iter_mut()
            .chain(self.chapters.iter_mut().flat_map(|c| &mut c.steps))
    }

    /// How many items carry a flag.
    pub fn flagged(&self) -> usize {
        self.steps().filter(|s| !s.flags.is_empty()).count()
            + self.chapters.iter().filter(|c| !c.flags.is_empty()).count()
            + self
                .chapters
                .iter()
                .filter(|c| !c.gist_flags.is_empty())
                .count()
            + self.also.iter().filter(|a| !a.flags.is_empty()).count()
            + self
                .decisions
                .iter()
                .filter(|d| !d.flags.is_empty())
                .count()
            + usize::from(!self.flags.is_empty())
    }

    /// Every flag, with where it is, for the record.
    pub fn all_flags(&self) -> Vec<String> {
        let mut out: Vec<String> = self.flags.clone();
        for c in &self.chapters {
            out.extend(
                c.flags
                    .iter()
                    .map(|f| format!("chapter {}: {f}", c.behaviour)),
            );
            out.extend(
                c.gist_flags
                    .iter()
                    .map(|f| format!("gist {}: {f}", c.behaviour)),
            );
        }
        for (i, s) in self.overview.iter().enumerate() {
            out.extend(s.flags.iter().map(|f| format!("overview {}: {f}", i + 1)));
        }
        for c in &self.chapters {
            for (i, s) in c.steps.iter().enumerate() {
                out.extend(
                    s.flags
                        .iter()
                        .map(|f| format!("{} {}: {f}", c.behaviour, i + 1)),
                );
            }
        }
        out.extend(
            self.also
                .iter()
                .flat_map(|a| a.flags.iter().map(|f| format!("also {}: {f}", a.part))),
        );
        out.extend(
            self.decisions
                .iter()
                .flat_map(|d| d.flags.iter().map(|f| format!("decision: {f}"))),
        );
        out
    }
}
