//! Fit-to-length (spec section 4.6). Deterministic, no LLM.
//!
//! [`fit`] proposes cuts as pending `LengthCut` suggestions; it never changes
//! the variant. The user reviews and accepts each cut.

use std::collections::BTreeMap;

use crate::{
    Cv, Id, IdSource, PageCountError, PageCounter, ResolvedCv, ResolvedEntry, ResolvedSection,
    Suggestion, SuggestionAction, SuggestionKind, SuggestionSource, SuggestionStatus,
    SuggestionTarget, Variant,
};

/// Per-item relevance to a job, 0..=100, from AI relevance scoring (spec section 11.5).
/// Missing items count as unscored.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct RelevanceScores {
    pub entries: BTreeMap<Id, u8>,
    pub bullets: BTreeMap<Id, u8>,
    pub skills: BTreeMap<Id, u8>,
}

/// Outcome of [`fit`].
#[derive(Clone, Debug, PartialEq)]
pub enum FitResult {
    /// The CV fits in `pages` pages once the `cuts` are accepted. `cuts` is
    /// empty when the CV already fits.
    Fits { pages: u32, cuts: Vec<Suggestion> },
    /// Even with every possible cut the CV needs `pages` pages.
    CannotFit { pages: u32 },
}

/// Most render-and-cut rounds [`fit`] runs before giving up.
pub const MAX_ITERATIONS: usize = 200;
/// Entries scoring below this are collapsed, rather than left as a bare
/// heading, when their last bullet is cut.
pub const COLLAPSE_BELOW: u8 = 40;
/// Priority added to the bullets of the most recent experience entry.
pub const RECENT_BONUS: i32 = 30;
/// Priority added to the first two bullets of every entry.
pub const LEAD_BONUS: i32 = 15;
/// Base priority of a bullet without a relevance score.
const UNSCORED: i32 = 50;

/// Proposes the cuts that make `cv` with `variant` fit in `page_target` pages.
///
/// Each round resolves the variant with the cuts so far, counts its pages,
/// and cuts the lowest-priority visible bullet (spec section 4.6). Locked
/// entries are never collapsed, though their bullets may be cut. A page
/// target of 0 counts as 1. Errors from `counter` are returned as they are.
pub fn fit(
    cv: &Cv,
    variant: &Variant,
    scores: Option<&RelevanceScores>,
    page_target: u8,
    counter: &dyn PageCounter,
    ids: &mut dyn IdSource,
) -> Result<FitResult, PageCountError> {
    let target = u32::from(page_target.max(1));
    let mut trial = variant.clone();
    let mut cuts = Vec::new();
    for _ in 0..MAX_ITERATIONS {
        let resolved = cv.resolve(Some(&trial));
        let pages = counter.pages(&resolved)?;
        if pages <= target {
            return Ok(FitResult::Fits { pages, cuts });
        }
        let Some(cut) = next_cut(&resolved, scores) else {
            return Ok(FitResult::CannotFit { pages });
        };
        let suggestion = match cut {
            Cut::Bullet { id, text } => {
                trial.hidden.insert(id.clone());
                length_cut(
                    ids,
                    SuggestionTarget::Bullet { id },
                    Some(text),
                    SuggestionAction::Hide,
                    target,
                )
            }
            Cut::Collapse { id } => {
                trial.collapsed.insert(id.clone());
                length_cut(
                    ids,
                    SuggestionTarget::Entry { id },
                    None,
                    SuggestionAction::Collapse,
                    target,
                )
            }
        };
        cuts.push(suggestion);
    }
    let pages = counter.pages(&cv.resolve(Some(&trial)))?;
    Ok(if pages <= target {
        FitResult::Fits { pages, cuts }
    } else {
        FitResult::CannotFit { pages }
    })
}

enum Cut {
    Bullet { id: Id, text: String },
    Collapse { id: Id },
}

/// Picks the lowest-priority visible bullet. Ties go to the bullet latest in
/// the document, so without scores later bullets of older entries go first.
fn next_cut(cv: &ResolvedCv, scores: Option<&RelevanceScores>) -> Option<Cut> {
    let recent = most_recent_experience(cv);
    let mut best: Option<(i32, &ResolvedEntry, usize)> = None;
    for entry in cv.sections.iter().flat_map(entries_of) {
        for (index, bullet) in entry.bullets.iter().enumerate() {
            let mut priority = scores
                .and_then(|s| s.bullets.get(&bullet.id))
                .map_or(UNSCORED, |&s| i32::from(s));
            if recent == Some(&entry.id) {
                priority += RECENT_BONUS;
            }
            if index < 2 {
                priority += LEAD_BONUS;
            }
            // `<=` lets later bullets win ties.
            if best.is_none_or(|(p, _, _)| priority <= p) {
                best = Some((priority, entry, index));
            }
        }
    }
    let (_, entry, index) = best?;
    let entry_score = scores.and_then(|s| s.entries.get(&entry.id)).copied();
    let last_bullet = entry.bullets.len() == 1;
    if last_bullet && !entry.locked && entry_score.is_none_or(|s| s < COLLAPSE_BELOW) {
        return Some(Cut::Collapse {
            id: entry.id.clone(),
        });
    }
    let bullet = &entry.bullets[index];
    Some(Cut::Bullet {
        id: bullet.id.clone(),
        text: bullet.text.clone(),
    })
}

fn entries_of(section: &ResolvedSection) -> &[ResolvedEntry] {
    match section {
        ResolvedSection::Experience(s)
        | ResolvedSection::Education(s)
        | ResolvedSection::Projects(s)
        | ResolvedSection::Certifications(s)
        | ResolvedSection::Custom(s) => &s.entries,
        ResolvedSection::Skills(_) | ResolvedSection::Languages(_) => &[],
    }
}

/// The current experience entry, else the one that started last; the first
/// in document order wins ties.
fn most_recent_experience(cv: &ResolvedCv) -> Option<&Id> {
    let mut best: Option<&ResolvedEntry> = None;
    for section in &cv.sections {
        let ResolvedSection::Experience(s) = section else {
            continue;
        };
        for e in &s.entries {
            let key = |e: &ResolvedEntry| (e.period.current, e.period.start);
            if best.is_none_or(|b| key(e) > key(b)) {
                best = Some(e);
            }
        }
    }
    best.map(|e| &e.id)
}

fn length_cut(
    ids: &mut dyn IdSource,
    target: SuggestionTarget,
    before: Option<String>,
    action: SuggestionAction,
    pages: u32,
) -> Suggestion {
    let unit = if pages == 1 { "page" } else { "pages" };
    Suggestion {
        id: ids.next_id(),
        kind: SuggestionKind::LengthCut,
        target,
        before,
        after: None,
        action: Some(action),
        reason: format!("Removed to fit {pages} {unit}"),
        confidence: 1.0,
        warnings: Vec::new(),
        status: SuggestionStatus::Pending,
        source: SuggestionSource::Tailor,
    }
}

#[cfg(test)]
mod tests {
    use std::cell::Cell;

    use super::*;
    use crate::test_fixtures::sample_cv;
    use crate::test_ids::id;
    use crate::{Bullet, Entry, PartialDate, Period, Section};

    /// One page per `lines_per_page` lines; every entry and bullet is a line.
    struct FakeCounter {
        lines_per_page: u32,
        calls: Cell<u32>,
    }

    impl FakeCounter {
        fn new(lines_per_page: u32) -> Self {
            Self {
                lines_per_page,
                calls: Cell::new(0),
            }
        }
    }

    impl PageCounter for FakeCounter {
        fn pages(&self, cv: &ResolvedCv) -> Result<u32, PageCountError> {
            self.calls.set(self.calls.get() + 1);
            let lines: usize = cv
                .sections
                .iter()
                .flat_map(entries_of)
                .map(|e| 1 + e.bullets.len())
                .sum();
            Ok(u32::try_from(lines)
                .expect("small")
                .div_ceil(self.lines_per_page)
                .max(1))
        }
    }

    struct FailingCounter;

    impl PageCounter for FailingCounter {
        fn pages(&self, _: &ResolvedCv) -> Result<u32, PageCountError> {
            Err(PageCountError("boom".into()))
        }
    }

    struct Ids(u32);

    impl IdSource for Ids {
        fn next_id(&mut self) -> Id {
            self.0 += 1;
            id(self.0)
        }
    }

    /// Experience: entry 4 (current, bullets 5, 41, 42) and entry 9 (2015-2018,
    /// bullets 10, 11, 12). 8 lines.
    fn cv() -> Cv {
        let mut cv = sample_cv();
        let Section::Experience(s) = &mut cv.sections[0] else {
            panic!("experience")
        };
        s.entries[0].bullets.extend([41, 42].map(|n| Bullet {
            id: id(n),
            text: format!("Recent bullet {n}"),
        }));
        s.entries.push(Entry {
            id: id(9),
            title: "Old job".into(),
            org: "Initech".into(),
            location: None,
            period: Period {
                start: Some(PartialDate {
                    year: 2015,
                    month: None,
                }),
                end: Some(PartialDate {
                    year: 2018,
                    month: None,
                }),
                current: false,
            },
            bullets: [10, 11, 12]
                .map(|n| Bullet {
                    id: id(n),
                    text: format!("Old bullet {n}"),
                })
                .into(),
            description: None,
            url: None,
            locked: false,
        });
        cv
    }

    fn variant() -> Variant {
        Variant::empty(id(100), id(1), "V")
    }

    fn targets(result: &FitResult) -> Vec<(SuggestionTarget, Option<SuggestionAction>)> {
        match result {
            FitResult::Fits { cuts, .. } => cuts
                .iter()
                .map(|c| (c.target.clone(), c.action.clone()))
                .collect(),
            FitResult::CannotFit { .. } => panic!("expected Fits, got {result:?}"),
        }
    }

    fn bullet(n: u32) -> (SuggestionTarget, Option<SuggestionAction>) {
        (
            SuggestionTarget::Bullet { id: id(n) },
            Some(SuggestionAction::Hide),
        )
    }

    fn collapse(n: u32) -> (SuggestionTarget, Option<SuggestionAction>) {
        (
            SuggestionTarget::Entry { id: id(n) },
            Some(SuggestionAction::Collapse),
        )
    }

    #[test]
    fn already_fitting_cv_needs_no_cuts() {
        let counter = FakeCounter::new(10);
        let result = fit(&cv(), &variant(), None, 1, &counter, &mut Ids(1000)).expect("fit");
        assert_eq!(
            result,
            FitResult::Fits {
                pages: 1,
                cuts: vec![]
            }
        );
        assert_eq!(counter.calls.get(), 1);
    }

    #[test]
    fn without_scores_later_bullets_of_older_entries_go_first() {
        // 8 lines, 7 per page: one cut needed.
        let result = fit(
            &cv(),
            &variant(),
            None,
            1,
            &FakeCounter::new(7),
            &mut Ids(1000),
        )
        .expect("fit");
        assert_eq!(targets(&result), [bullet(12)]);

        // 5 per page: three cuts. Bullet 12 (old, third) has priority 50; the
        // old entry's lead bullets 10 and 11 (65) go next, latest first, and
        // the old entry collapses when its last bullet goes. The recent
        // entry's bullets (80 and 95) stay.
        let result = fit(
            &cv(),
            &variant(),
            None,
            1,
            &FakeCounter::new(5),
            &mut Ids(1000),
        )
        .expect("fit");
        assert_eq!(targets(&result), [bullet(12), bullet(11), collapse(9)]);
    }

    #[test]
    fn cuts_are_pending_length_cut_suggestions_with_fresh_ids() {
        let result = fit(
            &cv(),
            &variant(),
            None,
            1,
            &FakeCounter::new(7),
            &mut Ids(1000),
        )
        .expect("fit");
        let FitResult::Fits { pages, cuts } = result else {
            panic!("fits")
        };
        assert_eq!(pages, 1);
        let cut = &cuts[0];
        assert_eq!(cut.id, id(1001));
        assert_eq!(cut.kind, SuggestionKind::LengthCut);
        assert_eq!(cut.status, SuggestionStatus::Pending);
        assert_eq!(cut.reason, "Removed to fit 1 page");
        assert_eq!(cut.before.as_deref(), Some("Old bullet 12"));
    }

    #[test]
    fn scores_drive_the_order_and_high_scoring_entries_keep_their_heading() {
        let mut scores = RelevanceScores::default();
        scores.bullets.insert(id(41), 0); // 0 + 30 recent + 15 lead = 45
        scores.bullets.insert(id(10), 90);
        scores.bullets.insert(id(11), 90);
        scores.bullets.insert(id(12), 90);
        let result = fit(
            &cv(),
            &variant(),
            Some(&scores),
            1,
            &FakeCounter::new(7),
            &mut Ids(1000),
        )
        .expect("fit");
        assert_eq!(targets(&result), [bullet(41)]);

        // Old entry with all bullets low but entry relevance 60: its last
        // bullet is hidden, not the entry collapsed.
        let mut scores = RelevanceScores::default();
        for n in [10, 11, 12] {
            scores.bullets.insert(id(n), 0);
        }
        scores.entries.insert(id(9), 60);
        let result = fit(
            &cv(),
            &variant(),
            Some(&scores),
            1,
            &FakeCounter::new(5),
            &mut Ids(1000),
        )
        .expect("fit");
        assert_eq!(targets(&result), [bullet(12), bullet(11), bullet(10)]);
    }

    #[test]
    fn low_relevance_entries_collapse_when_their_last_bullet_goes() {
        let mut scores = RelevanceScores::default();
        for n in [10, 11, 12] {
            scores.bullets.insert(id(n), 0);
        }
        scores.entries.insert(id(9), 10);
        let result = fit(
            &cv(),
            &variant(),
            Some(&scores),
            1,
            &FakeCounter::new(5),
            &mut Ids(1000),
        )
        .expect("fit");
        assert_eq!(targets(&result), [bullet(12), bullet(11), collapse(9)]);
    }

    #[test]
    fn locked_entries_lose_bullets_but_never_collapse() {
        let mut cv = cv();
        if let Section::Experience(s) = &mut cv.sections[0] {
            s.entries[1].locked = true;
        }
        let result = fit(
            &cv,
            &variant(),
            None,
            1,
            &FakeCounter::new(5),
            &mut Ids(1000),
        )
        .expect("fit");
        assert_eq!(targets(&result), [bullet(12), bullet(11), bullet(10)]);
    }

    #[test]
    fn cannot_fit_when_content_runs_out() {
        let result = fit(
            &cv(),
            &variant(),
            None,
            1,
            &FakeCounter::new(1),
            &mut Ids(1000),
        )
        .expect("fit");
        assert_eq!(result, FitResult::CannotFit { pages: 2 });
    }

    #[test]
    fn existing_overlay_is_respected_and_not_modified() {
        let mut v = variant();
        v.hidden.insert(id(12));
        let before = v.clone();
        let result = fit(&cv(), &v, None, 1, &FakeCounter::new(6), &mut Ids(1000)).expect("fit");
        assert_eq!(targets(&result), [bullet(11)]);
        assert_eq!(v, before);
    }

    #[test]
    fn page_target_zero_counts_as_one() {
        let result = fit(
            &cv(),
            &variant(),
            None,
            0,
            &FakeCounter::new(7),
            &mut Ids(1000),
        )
        .expect("fit");
        assert_eq!(targets(&result), [bullet(12)]);
    }

    #[test]
    fn counter_errors_are_returned() {
        let err = fit(&cv(), &variant(), None, 1, &FailingCounter, &mut Ids(1000))
            .expect_err("counter fails");
        assert_eq!(err, PageCountError("boom".into()));
    }

    #[test]
    fn iterations_are_capped() {
        let mut cv = cv();
        if let Section::Experience(s) = &mut cv.sections[0] {
            s.entries[1].bullets = (0..300)
                .map(|n| Bullet {
                    id: id(5000 + n),
                    text: format!("Bullet {n}"),
                })
                .collect();
        }
        let counter = FakeCounter::new(1);
        let result = fit(&cv, &variant(), None, 1, &counter, &mut Ids(10_000)).expect("fit");
        assert!(matches!(result, FitResult::CannotFit { .. }));
        assert_eq!(
            counter.calls.get(),
            u32::try_from(MAX_ITERATIONS).expect("small") + 1
        );
    }

    #[test]
    fn most_recent_prefers_current_then_latest_start() {
        let r = cv().resolve(None);
        assert_eq!(most_recent_experience(&r), Some(&id(4)));
        let mut cv = cv();
        if let Section::Experience(s) = &mut cv.sections[0] {
            s.entries[0].period.current = false;
        }
        assert_eq!(most_recent_experience(&cv.resolve(None)), Some(&id(4)));
    }
}
