//! Fresh ULIDs for new entities (spec section 4.1).

use cv_core::{Id, IdSource};
use ulid::Ulid;

/// Random ULIDs from the system clock and the OS random source.
#[derive(Clone, Copy, Debug, Default)]
pub struct UlidIds;

impl IdSource for UlidIds {
    fn next_id(&mut self) -> Id {
        // `Ulid` always prints 26 valid Crockford characters.
        Id::parse(&Ulid::generate().to_string()).unwrap_or_else(|e| unreachable!("{e}"))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn ids_are_valid_and_distinct() {
        let mut ids = UlidIds;
        let a = ids.next_id();
        let b = ids.next_id();
        assert_ne!(a, b);
        assert_eq!(a.as_str().len(), 26);
    }
}
