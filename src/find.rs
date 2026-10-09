//! State for the find bar: the query and the matching text items on the current page.

use std::collections::HashSet;

use vera_view::scene::{ItemKind, Scene};

#[derive(Default)]
pub struct Find {
    pub open: bool,
    pub query: String,
    pub match_case: bool,
    /// Request keyboard focus for the query field on the next frame.
    pub focus: bool,
    /// Indices into `Scene::items` of matching text, in reading order.
    pub matches: Vec<usize>,
    match_set: HashSet<usize>,
    /// Index into `matches` of the selected match.
    pub current: Option<usize>,
    /// Extra status such as "Wrapped to the start".
    pub message: Option<String>,
    /// The query, case flag and scene generation `matches` was computed for.
    key: Option<(String, bool, u64)>,
}

impl Find {
    pub fn is_active(&self) -> bool {
        self.open && !self.query.trim().is_empty()
    }

    /// A predicate testing text against the query.
    pub fn matcher(&self) -> impl Fn(&str) -> bool + use<> {
        let case = self.match_case;
        let needle = if case { self.query.clone() } else { self.query.to_lowercase() };
        move |text: &str| {
            if case {
                text.contains(&needle)
            } else {
                text.to_lowercase().contains(&needle)
            }
        }
    }

    /// Recompute matches if the query or the scene changed since last time.
    pub fn refresh(&mut self, scene: Option<&Scene>, scene_gen: u64) {
        let key = (self.query.clone(), self.match_case, scene_gen);
        if self.key.as_ref() == Some(&key) {
            return;
        }
        self.key = Some(key);
        self.matches.clear();
        self.match_set.clear();
        self.current = None;
        let (Some(scene), true) = (scene, self.is_active()) else { return };

        let matches = self.matcher();
        let mut found: Vec<(usize, [f32; 2])> = scene
            .items
            .iter()
            .enumerate()
            .filter_map(|(i, item)| match &item.kind {
                ItemKind::Text(t) if matches(&t.plain) => Some((i, t.center)),
                _ => None,
            })
            .collect();
        // Reading order: top to bottom in quarter-inch bands, then left to right.
        found.sort_by(|a, b| {
            let band = |c: [f32; 2]| (-c[1] * 4.0).round();
            band(a.1).total_cmp(&band(b.1)).then(a.1[0].total_cmp(&b.1[0]))
        });
        self.matches = found.into_iter().map(|(i, _)| i).collect();
        self.match_set = self.matches.iter().copied().collect();
    }

    pub fn is_match(&self, item: usize) -> bool {
        self.match_set.contains(&item)
    }

    pub fn current_item(&self) -> Option<usize> {
        self.current.and_then(|c| self.matches.get(c).copied())
    }

    /// Text for the status label next to the query field.
    pub fn status(&self) -> String {
        if !self.is_active() {
            return String::new();
        }
        let mut s = match (self.current, self.matches.len()) {
            (_, 0) => "No matches".to_string(),
            (Some(c), n) => format!("{} of {n} on this page", c + 1),
            (None, n) => format!("{n} on this page"),
        };
        if let Some(m) = &self.message {
            s = format!("{s} · {m}");
        }
        s
    }
}
