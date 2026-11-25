//! [`Names`], the collision helper an extractor shares with the others (spec #569, "Unsure"): it registers each name its
//! language declares with the target the name stands for, and a lookup tells one target from a collision.

use std::collections::BTreeMap;

/// The names a language declares, each with every target that declares it.
#[cfg_attr(
    not(test),
    expect(dead_code, reason = "go-imports (#578) is its first user")
)]
#[derive(Debug)]
pub(crate) struct Names<T> {
    targets: BTreeMap<String, Vec<T>>,
}

/// What a name stands for.
#[derive(Debug, PartialEq, Eq)]
pub(crate) enum Lookup<'a, T> {
    /// Nothing declares it.
    Unknown,
    /// One target declares it.
    One(&'a T),
    /// More than one target declares it: every one, in the order they were registered.
    Ambiguous(Vec<&'a T>),
}

#[cfg_attr(
    not(test),
    expect(dead_code, reason = "go-imports (#578) is its first user")
)]
impl<T: PartialEq> Names<T> {
    pub(crate) fn new() -> Names<T> {
        Names {
            targets: BTreeMap::new(),
        }
    }

    /// Registers `target` as declaring `name`; a target registered twice for one name counts once.
    pub(crate) fn register(&mut self, name: &str, target: T) {
        let targets = self.targets.entry(name.to_string()).or_default();
        if !targets.contains(&target) {
            targets.push(target);
        }
    }

    /// The target `name` stands for, or every target when more than one declares it.
    pub(crate) fn lookup(&self, name: &str) -> Lookup<'_, T> {
        match self.targets.get(name).map(Vec::as_slice) {
            None | Some([]) => Lookup::Unknown,
            Some([one]) => Lookup::One(one),
            Some(all) => Lookup::Ambiguous(all.iter().collect()),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::{Lookup, Names};

    #[test]
    fn a_name_one_target_declares_gives_that_target() {
        let mut names = Names::new();
        names.register("example.com/a", "a.go");
        names.register("example.com/a", "a.go");
        names.register("example.com/b", "b.go");
        assert_eq!(names.lookup("example.com/a"), Lookup::One(&"a.go"));
        assert_eq!(names.lookup("example.com/c"), Lookup::Unknown);
    }

    #[test]
    fn a_name_two_targets_declare_is_ambiguous_with_every_candidate() {
        let mut names = Names::new();
        names.register("example.com/x", "b/go.mod");
        names.register("example.com/x", "a/go.mod");
        names.register("example.com/x", "c/go.mod");
        assert_eq!(
            names.lookup("example.com/x"),
            Lookup::Ambiguous(vec![&"b/go.mod", &"a/go.mod", &"c/go.mod"])
        );
    }

    #[test]
    fn a_target_may_be_a_folder() {
        let mut names: Names<std::path::PathBuf> = Names::new();
        names.register("example.com/x", "a/".into());
        names.register("example.com/x", "b/".into());
        assert_eq!(
            names.lookup("example.com/x"),
            Lookup::Ambiguous(vec![
                &std::path::PathBuf::from("a/"),
                &std::path::PathBuf::from("b/")
            ])
        );
    }
}
