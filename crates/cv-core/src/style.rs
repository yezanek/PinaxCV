//! Styles: a CV's template, paper, options and layout without its content,
//! so they can be saved to a file and applied to any CV (ADR-084). Sections
//! are named by kind, and custom sections also by title, never by ID.

use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

use crate::{
    BlockRef, Column, Columns, Cv, Layout, Paper, Section, TemplateId, TemplateOptions, TopBlock,
};

/// The kind of a section, as a style names it.
#[derive(Serialize, Deserialize, JsonSchema, Clone, Copy, Debug, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum SectionKind {
    Experience,
    Education,
    Projects,
    Skills,
    Languages,
    Certifications,
    Custom,
}

impl SectionKind {
    pub const ALL: [SectionKind; 7] = [
        SectionKind::Experience,
        SectionKind::Education,
        SectionKind::Projects,
        SectionKind::Skills,
        SectionKind::Languages,
        SectionKind::Certifications,
        SectionKind::Custom,
    ];

    pub fn of(section: &Section) -> Self {
        match section {
            Section::Experience(_) => Self::Experience,
            Section::Education(_) => Self::Education,
            Section::Projects(_) => Self::Projects,
            Section::Skills(_) => Self::Skills,
            Section::Languages(_) => Self::Languages,
            Section::Certifications(_) => Self::Certifications,
            Section::Custom(_) => Self::Custom,
        }
    }

    /// The name used in style files.
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Experience => "experience",
            Self::Education => "education",
            Self::Projects => "projects",
            Self::Skills => "skills",
            Self::Languages => "languages",
            Self::Certifications => "certifications",
            Self::Custom => "custom",
        }
    }
}

/// A block in a style: a top block, or a section named by kind. `title` is
/// set for custom sections only.
#[derive(Serialize, Deserialize, JsonSchema, Clone, Debug, PartialEq, Eq)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum StyleBlock {
    Top {
        block: TopBlock,
    },
    Section {
        section: SectionKind,
        title: Option<String>,
    },
}

/// A block and the column it is placed in.
#[derive(Serialize, Deserialize, JsonSchema, Clone, Debug, PartialEq, Eq)]
pub struct PlacedBlock {
    pub block: StyleBlock,
    pub column: Column,
}

/// Everything about how a CV looks, and nothing it says.
#[derive(Serialize, Deserialize, JsonSchema, Clone, Debug, PartialEq)]
pub struct Style {
    pub template: TemplateId,
    pub paper: Paper,
    pub options: TemplateOptions,
    pub columns: Columns,
    pub sidebar_width_pct: u8,
    /// Top blocks in their order, then sections in the CV's order, each with
    /// its column. One list keeps the order across columns, which a
    /// one-column page shows.
    pub blocks: Vec<PlacedBlock>,
}

/// What [`Cv::apply_style`] could not match.
#[derive(Serialize, Deserialize, JsonSchema, Clone, Debug, Default, PartialEq, Eq)]
pub struct StyleApplied {
    /// Sections the style names that the CV does not have.
    pub unmatched_blocks: Vec<StyleBlock>,
    /// The CV's sections the style does not name; they go to the main
    /// column, after the matched ones.
    pub unplaced_sections: usize,
}

fn same_title(a: &str, b: &str) -> bool {
    a.trim().to_lowercase() == b.trim().to_lowercase()
}

impl Style {
    /// The style of `cv`. Placements are kept in one-column mode, like the
    /// layout keeps them, so a one-column style still carries its sidebar.
    pub fn from_cv(cv: &Cv) -> Self {
        let mut layout = cv.layout.clone();
        let ids: Vec<_> = cv.sections.iter().map(|s| s.id().clone()).collect();
        layout.normalize(&ids);
        let top = layout.top.iter().map(|&block| PlacedBlock {
            block: StyleBlock::Top { block },
            column: layout.placement(&BlockRef::Top { block }),
        });
        let sections = cv.sections.iter().map(|s| PlacedBlock {
            block: StyleBlock::Section {
                section: SectionKind::of(s),
                title: match s {
                    Section::Custom(c) => Some(c.title.clone()),
                    _ => None,
                },
            },
            column: layout.placement(&BlockRef::Section { id: s.id().clone() }),
        });
        Self {
            template: cv.template.clone(),
            paper: cv.paper,
            options: cv.options.clone(),
            columns: layout.columns,
            sidebar_width_pct: layout.sidebar_width_pct,
            blocks: top.chain(sections).collect(),
        }
    }
}

impl Cv {
    /// Applies `style`: template, paper, options and columns are copied, and
    /// the style's sections are matched to the CV's, custom sections by
    /// title first, then every remaining block by kind in order. Matched
    /// sections take the style's order and column; the rest follow in their
    /// own order, in the main column. Options are clamped when the CV is
    /// saved, the sidebar width here.
    pub fn apply_style(&mut self, style: &Style) -> StyleApplied {
        self.template = style.template.clone();
        self.paper = style.paper;
        self.options = style.options.clone();

        let wanted: Vec<(SectionKind, Option<&str>, Column)> = style
            .blocks
            .iter()
            .filter_map(|b| match &b.block {
                StyleBlock::Section { section, title } => {
                    Some((*section, title.as_deref(), b.column))
                }
                StyleBlock::Top { .. } => None,
            })
            .collect();
        // matched[i] is the CV section index for wanted[i].
        let mut matched: Vec<Option<usize>> = vec![None; wanted.len()];
        let mut used = vec![false; self.sections.len()];
        let take = |want: &dyn Fn(&Section) -> bool, used: &mut Vec<bool>| {
            let i = self
                .sections
                .iter()
                .enumerate()
                .position(|(i, s)| !used[i] && want(s))?;
            used[i] = true;
            Some(i)
        };
        for (slot, (kind, title, _)) in matched.iter_mut().zip(&wanted) {
            if let (SectionKind::Custom, Some(title)) = (kind, title) {
                *slot = take(
                    &|s| matches!(s, Section::Custom(c) if same_title(&c.title, title)),
                    &mut used,
                );
            }
        }
        for (slot, (kind, _, _)) in matched.iter_mut().zip(&wanted) {
            if slot.is_none() {
                *slot = take(&|s| SectionKind::of(s) == *kind, &mut used);
            }
        }

        let mut layout = Layout {
            columns: style.columns,
            sidebar_width_pct: style.sidebar_width_pct,
            top: Vec::new(),
            sidebar: Vec::new(),
        };
        for b in &style.blocks {
            if let StyleBlock::Top { block } = b.block {
                layout.top.push(block);
                if b.column == Column::Sidebar {
                    layout.sidebar.push(BlockRef::Top { block });
                }
            }
        }
        let mut order: Vec<usize> = Vec::with_capacity(self.sections.len());
        let mut applied = StyleApplied::default();
        for (slot, (kind, title, column)) in matched.iter().zip(&wanted) {
            match slot {
                Some(i) => {
                    order.push(*i);
                    if *column == Column::Sidebar {
                        layout.sidebar.push(BlockRef::Section {
                            id: self.sections[*i].id().clone(),
                        });
                    }
                }
                None => applied.unmatched_blocks.push(StyleBlock::Section {
                    section: *kind,
                    title: title.map(str::to_owned),
                }),
            }
        }
        for (i, was_used) in used.iter().enumerate() {
            if !was_used {
                order.push(i);
                applied.unplaced_sections += 1;
            }
        }
        let mut old: Vec<Option<Section>> = self.sections.drain(..).map(Some).collect();
        self.sections = order.into_iter().filter_map(|i| old[i].take()).collect();
        let ids: Vec<_> = self.sections.iter().map(|s| s.id().clone()).collect();
        layout.normalize(&ids);
        self.layout = layout;
        applied
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::test_fixtures::sample_cv;
    use crate::test_ids::id;
    use crate::{CustomSection, LanguagesSection, SkillsSection};

    fn custom(n: u32, title: &str) -> Section {
        Section::Custom(CustomSection {
            id: id(n),
            title: title.into(),
            entries: Vec::new(),
        })
    }

    fn skills(n: u32) -> Section {
        Section::Skills(SkillsSection {
            id: id(n),
            title_override: None,
            groups: Vec::new(),
        })
    }

    fn languages(n: u32) -> Section {
        Section::Languages(LanguagesSection {
            id: id(n),
            title_override: None,
            items: Vec::new(),
        })
    }

    fn kinds(cv: &Cv) -> Vec<SectionKind> {
        cv.sections.iter().map(SectionKind::of).collect()
    }

    fn section_ids(cv: &Cv) -> Vec<crate::Id> {
        cv.sections.iter().map(|s| s.id().clone()).collect()
    }

    /// Name, contact and languages on the left; summary, work and
    /// education on the right.
    fn two_column(mut cv: Cv) -> Cv {
        cv.layout.columns = Columns::Two;
        cv.layout.sidebar_width_pct = 30;
        cv.layout.sidebar = vec![
            BlockRef::Top {
                block: TopBlock::Name,
            },
            BlockRef::Top {
                block: TopBlock::Contact,
            },
        ];
        let lang = cv
            .sections
            .iter()
            .find(|s| SectionKind::of(s) == SectionKind::Languages)
            .map(|s| s.id().clone());
        if let Some(id) = lang {
            cv.layout.sidebar.push(BlockRef::Section { id });
        }
        cv
    }

    #[test]
    fn a_style_round_trips_onto_its_own_cv() {
        let mut cv = sample_cv();
        cv.sections.push(languages(90));
        cv.options.accent = Some("#2B5CAB".into());
        let cv = two_column(cv);
        let style = Style::from_cv(&cv);
        assert_eq!(style.columns, Columns::Two);
        assert_eq!(style.blocks.len(), 3 + cv.sections.len());

        let mut copy = cv.clone();
        copy.layout = Layout::default();
        copy.options = TemplateOptions::default();
        let applied = copy.apply_style(&style);
        assert_eq!(applied, StyleApplied::default());
        assert_eq!(copy.layout, cv.layout);
        assert_eq!(copy.options, cv.options);
        assert_eq!(section_ids(&copy), section_ids(&cv));
    }

    #[test]
    fn sections_follow_the_style_order_and_unmatched_go_last_in_main() {
        let mut from = sample_cv();
        from.sections = vec![languages(90), skills(91)];
        from.layout.columns = Columns::Two;
        from.layout.sidebar = vec![BlockRef::Section { id: id(90) }];
        let style = Style::from_cv(&from);

        let mut cv = sample_cv();
        cv.sections = vec![
            custom(79, "Awards"),
            skills(80),
            languages(81),
            custom(82, "Talks"),
        ];
        let applied = cv.apply_style(&style);
        assert_eq!(applied.unmatched_blocks, []);
        assert_eq!(applied.unplaced_sections, 2);
        assert_eq!(section_ids(&cv), [id(81), id(80), id(79), id(82)]);
        assert_eq!(
            kinds(&cv)[..2],
            [SectionKind::Languages, SectionKind::Skills]
        );
        assert_eq!(cv.layout.sidebar, [BlockRef::Section { id: id(81) }]);
    }

    #[test]
    fn custom_sections_match_by_title_first_then_in_order() {
        let mut from = sample_cv();
        from.sections = vec![custom(90, "Volunteering"), custom(91, "Talks")];
        from.layout.columns = Columns::Two;
        from.layout.sidebar = vec![BlockRef::Section { id: id(91) }];
        let style = Style::from_cv(&from);

        let mut cv = sample_cv();
        cv.sections = vec![custom(80, "Awards"), custom(81, " talks ")];
        let applied = cv.apply_style(&style);
        // "Volunteering" has no title match, so it takes the first custom
        // section left; "Talks" matched " talks " by title.
        assert_eq!(section_ids(&cv), [id(80), id(81)]);
        assert_eq!(cv.layout.sidebar, [BlockRef::Section { id: id(81) }]);
        assert_eq!(applied, StyleApplied::default());
    }

    #[test]
    fn blocks_the_cv_lacks_are_reported() {
        let mut from = sample_cv();
        from.sections = vec![languages(90), custom(91, "Talks")];
        let style = Style::from_cv(&from);
        let mut cv = sample_cv();
        cv.sections = vec![skills(80)];
        let applied = cv.apply_style(&style);
        assert_eq!(
            applied.unmatched_blocks,
            [
                StyleBlock::Section {
                    section: SectionKind::Languages,
                    title: None
                },
                StyleBlock::Section {
                    section: SectionKind::Custom,
                    title: Some("Talks".into())
                },
            ]
        );
        assert_eq!(applied.unplaced_sections, 1);
        assert_eq!(section_ids(&cv), [id(80)]);
    }

    #[test]
    fn missing_top_blocks_and_width_are_repaired() {
        let style = Style {
            template: TemplateId::default(),
            paper: Paper::Letter,
            options: TemplateOptions::default(),
            columns: Columns::Two,
            sidebar_width_pct: 90,
            blocks: vec![PlacedBlock {
                block: StyleBlock::Top {
                    block: TopBlock::Summary,
                },
                column: Column::Sidebar,
            }],
        };
        let mut cv = sample_cv();
        cv.apply_style(&style);
        assert_eq!(cv.paper, Paper::Letter);
        assert_eq!(cv.layout.sidebar_width_pct, Layout::SIDEBAR_WIDTH_PCT.1);
        assert_eq!(
            cv.layout.top,
            [TopBlock::Summary, TopBlock::Name, TopBlock::Contact]
        );
        assert_eq!(
            cv.layout.sidebar,
            [BlockRef::Top {
                block: TopBlock::Summary
            }]
        );
    }
}
