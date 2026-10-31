//! Page layout: one or two columns and which column each block sits in
//! (ADR-084). The order of sections inside a column is the CV's section
//! order with the variant's order applied; the layout never orders sections.

use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

use crate::Id;

#[derive(Serialize, Deserialize, JsonSchema, Clone, Copy, Debug, PartialEq, Eq, Default)]
#[serde(rename_all = "snake_case")]
pub enum Columns {
    #[default]
    One,
    Two,
}

#[derive(Serialize, Deserialize, JsonSchema, Clone, Copy, Debug, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum Column {
    Sidebar,
    Main,
}

/// A block that always sits at the top of its column, before the sections.
/// The photo is drawn with the name.
#[derive(Serialize, Deserialize, JsonSchema, Clone, Copy, Debug, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum TopBlock {
    Name,
    Contact,
    Summary,
}

impl TopBlock {
    /// Every top block, in the default order.
    pub const ALL: [TopBlock; 3] = [TopBlock::Name, TopBlock::Contact, TopBlock::Summary];
}

/// A block that can be placed in a column.
#[derive(Serialize, Deserialize, JsonSchema, Clone, Debug, PartialEq, Eq)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum BlockRef {
    Top { block: TopBlock },
    Section { id: Id },
}

/// How the page is laid out. The default is today's single column.
#[derive(Serialize, Deserialize, JsonSchema, Clone, Debug, PartialEq)]
pub struct Layout {
    pub columns: Columns,
    /// Width of the sidebar as a share of the text width.
    pub sidebar_width_pct: u8,
    /// Order of the top blocks; each appears once.
    pub top: Vec<TopBlock>,
    /// Blocks in the sidebar. Everything else is in the main column. Kept
    /// in one-column mode so switching back restores the sidebar.
    pub sidebar: Vec<BlockRef>,
}

impl Default for Layout {
    fn default() -> Self {
        Self {
            columns: Columns::One,
            sidebar_width_pct: 32,
            top: TopBlock::ALL.to_vec(),
            sidebar: Vec::new(),
        }
    }
}

impl Layout {
    pub const SIDEBAR_WIDTH_PCT: (u8, u8) = (25, 40);

    /// The column `block` is placed in, ignoring the column count.
    pub fn placement(&self, block: &BlockRef) -> Column {
        if self.sidebar.contains(block) {
            Column::Sidebar
        } else {
            Column::Main
        }
    }

    /// The column `block` is drawn in: always Main with one column.
    pub fn column_of(&self, block: &BlockRef) -> Column {
        match self.columns {
            Columns::One => Column::Main,
            Columns::Two => self.placement(block),
        }
    }

    /// Places `block` in `column`. The name and the contact details move
    /// together: PDF text is read one column at a time, so contact details
    /// in the other column could land after a long column and miss the ATS
    /// check for contacts at the top of page 1. Returns whether anything
    /// changed.
    pub fn set_column(&mut self, block: BlockRef, column: Column) -> bool {
        if let BlockRef::Top {
            block: TopBlock::Name | TopBlock::Contact,
        } = block
        {
            let name = self.set_one(
                BlockRef::Top {
                    block: TopBlock::Name,
                },
                column,
            );
            let contact = self.set_one(
                BlockRef::Top {
                    block: TopBlock::Contact,
                },
                column,
            );
            return name || contact;
        }
        self.set_one(block, column)
    }

    fn set_one(&mut self, block: BlockRef, column: Column) -> bool {
        let present = self.sidebar.contains(&block);
        match column {
            Column::Sidebar if !present => {
                self.sidebar.push(block);
                true
            }
            Column::Main if present => {
                self.sidebar.retain(|b| *b != block);
                true
            }
            _ => false,
        }
    }

    /// The top blocks drawn in `column`, in order.
    pub fn top_in(&self, column: Column) -> Vec<TopBlock> {
        self.top
            .iter()
            .copied()
            .filter(|&b| self.column_of(&BlockRef::Top { block: b }) == column)
            .collect()
    }

    /// The column drawn on the left and read first: the one holding the
    /// name. PDF text is read left column first, so the name's column goes
    /// on the left and the text always starts with the name; the sidebar's
    /// side follows from where the name is.
    pub fn first_column(&self) -> Column {
        self.column_of(&BlockRef::Top {
            block: TopBlock::Name,
        })
    }

    /// Moves `block` one place up (`up`) or down within the top order.
    /// Returns false when it is already at that end.
    pub fn shift_top(&mut self, block: TopBlock, up: bool) -> bool {
        let Some(i) = self.top.iter().position(|b| *b == block) else {
            return false;
        };
        let j = if up { i.checked_sub(1) } else { Some(i + 1) };
        match j {
            Some(j) if j < self.top.len() => {
                self.top.swap(i, j);
                true
            }
            _ => false,
        }
    }

    /// Repairs the layout against the CV's sections: every top block once
    /// (missing ones appended in default order), no sidebar entry for a
    /// section that does not exist or for the same block twice, the contact
    /// details in the name's column, and the width clamped. Returns whether
    /// the width had to be clamped.
    pub fn normalize(&mut self, section_ids: &[Id]) -> bool {
        let mut top = Vec::new();
        for b in self.top.iter().chain(TopBlock::ALL.iter()) {
            if !top.contains(b) {
                top.push(*b);
            }
        }
        self.top = top;
        let mut sidebar: Vec<BlockRef> = Vec::new();
        for b in self.sidebar.drain(..) {
            let known = match &b {
                BlockRef::Top { .. } => true,
                BlockRef::Section { id } => section_ids.contains(id),
            };
            if known && !sidebar.contains(&b) {
                sidebar.push(b);
            }
        }
        self.sidebar = sidebar;
        let name = self.placement(&BlockRef::Top {
            block: TopBlock::Name,
        });
        self.set_one(
            BlockRef::Top {
                block: TopBlock::Contact,
            },
            name,
        );
        let (min, max) = Self::SIDEBAR_WIDTH_PCT;
        let width = self.sidebar_width_pct.clamp(min, max);
        let clamped = width != self.sidebar_width_pct;
        self.sidebar_width_pct = width;
        clamped
    }

    /// Replaces section IDs after a CV was copied with fresh IDs.
    pub fn remap_sections(&mut self, map: &[(Id, Id)]) {
        for b in &mut self.sidebar {
            if let BlockRef::Section { id } = b
                && let Some((_, new)) = map.iter().find(|(old, _)| old == id)
            {
                *id = new.clone();
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::test_ids::id;

    fn section(n: u32) -> BlockRef {
        BlockRef::Section { id: id(n) }
    }

    const NAME: BlockRef = BlockRef::Top {
        block: TopBlock::Name,
    };

    #[test]
    fn default_is_one_column_with_everything_in_main() {
        let l = Layout::default();
        assert_eq!(l.columns, Columns::One);
        assert_eq!(l.top, TopBlock::ALL);
        assert_eq!(l.column_of(&section(3)), Column::Main);
        assert_eq!(l.first_column(), Column::Main);
        assert_eq!(l.top_in(Column::Main), TopBlock::ALL);
        assert!(l.top_in(Column::Sidebar).is_empty());
    }

    #[test]
    fn placement_applies_only_with_two_columns() {
        let mut l = Layout::default();
        assert!(l.set_column(NAME, Column::Sidebar));
        assert!(!l.set_column(NAME, Column::Sidebar), "already there");
        assert!(l.set_column(section(3), Column::Sidebar));
        assert_eq!(l.placement(&section(3)), Column::Sidebar);
        assert_eq!(l.column_of(&section(3)), Column::Main, "one column");
        assert_eq!(l.first_column(), Column::Main);
        l.columns = Columns::Two;
        assert_eq!(l.column_of(&section(3)), Column::Sidebar);
        assert_eq!(l.first_column(), Column::Sidebar);
        assert_eq!(
            l.top_in(Column::Sidebar),
            [TopBlock::Name, TopBlock::Contact]
        );
        assert_eq!(l.top_in(Column::Main), [TopBlock::Summary]);
        assert!(l.set_column(section(3), Column::Main));
        assert!(!l.set_column(section(3), Column::Main));
        assert_eq!(l.column_of(&section(3)), Column::Main);
    }

    #[test]
    fn top_blocks_shift_within_bounds() {
        let mut l = Layout::default();
        assert!(l.shift_top(TopBlock::Summary, true));
        assert_eq!(
            l.top,
            [TopBlock::Name, TopBlock::Summary, TopBlock::Contact]
        );
        assert!(!l.shift_top(TopBlock::Name, true));
        assert!(!l.shift_top(TopBlock::Contact, false));
    }

    #[test]
    fn normalize_repairs_top_sidebar_and_width() {
        let mut l = Layout {
            columns: Columns::Two,
            sidebar_width_pct: 90,
            top: vec![TopBlock::Summary, TopBlock::Summary],
            sidebar: vec![section(3), section(9), section(3), NAME],
        };
        assert!(l.normalize(&[id(3), id(6)]));
        assert_eq!(
            l.top,
            [TopBlock::Summary, TopBlock::Name, TopBlock::Contact]
        );
        let contact = BlockRef::Top {
            block: TopBlock::Contact,
        };
        assert_eq!(l.sidebar, [section(3), NAME, contact.clone()]);
        assert_eq!(l.sidebar_width_pct, 40);
        assert!(!l.normalize(&[id(3), id(6)]), "idempotent");
        l.sidebar_width_pct = 0;
        assert!(l.normalize(&[]));
        assert_eq!(l.sidebar_width_pct, 25);
        assert_eq!(l.sidebar, [NAME, contact]);
    }

    #[test]
    fn name_and_contact_share_a_column() {
        let name = BlockRef::Top {
            block: TopBlock::Name,
        };
        let contact = BlockRef::Top {
            block: TopBlock::Contact,
        };
        let mut l = Layout {
            columns: Columns::Two,
            ..Layout::default()
        };
        assert!(l.set_column(contact.clone(), Column::Sidebar));
        assert_eq!(l.placement(&name), Column::Sidebar);
        assert!(l.set_column(name.clone(), Column::Main));
        assert_eq!(l.placement(&contact), Column::Main);

        // A stored layout with them apart is repaired to the name's column.
        l.sidebar = vec![contact.clone()];
        l.normalize(&[]);
        assert_eq!(l.placement(&contact), Column::Main);
        l.sidebar = vec![name.clone()];
        l.normalize(&[]);
        assert_eq!(l.placement(&contact), Column::Sidebar);
    }

    #[test]
    fn remap_replaces_section_ids_only() {
        let mut l = Layout {
            sidebar: vec![section(3), NAME, section(6)],
            ..Layout::default()
        };
        l.remap_sections(&[(id(3), id(30)), (id(6), id(60))]);
        assert_eq!(l.sidebar, [section(30), NAME, section(60)]);
    }

    #[test]
    fn serializes_with_snake_case_tags() {
        let l = Layout {
            columns: Columns::Two,
            sidebar: vec![NAME, section(3)],
            ..Layout::default()
        };
        let v = serde_json::to_value(&l).expect("serialize");
        assert_eq!(v["columns"], "two");
        assert_eq!(v["top"][0], "name");
        assert_eq!(v["sidebar"][0]["kind"], "top");
        assert_eq!(v["sidebar"][0]["block"], "name");
        assert_eq!(v["sidebar"][1]["kind"], "section");
        let back: Layout = serde_json::from_value(v).expect("deserialize");
        assert_eq!(back, l);
    }
}
