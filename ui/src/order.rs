//! Moving an item one place up or down in a list.

/// Swaps the first item matching `is` with its neighbour above (`up`) or
/// below. Returns false, changing nothing, when there is no such item or it
/// is already at that end.
pub fn shift<T>(items: &mut [T], is: impl Fn(&T) -> bool, up: bool) -> bool {
    let Some(i) = items.iter().position(is) else {
        return false;
    };
    let j = if up { i.checked_sub(1) } else { Some(i + 1) };
    match j {
        Some(j) if j < items.len() => {
            items.swap(i, j);
            true
        }
        _ => false,
    }
}

#[cfg(test)]
mod tests {
    use super::shift;

    #[test]
    fn moves_up_and_down_within_bounds() {
        let mut v = vec![1, 2, 3];
        assert!(shift(&mut v, |x| *x == 2, true));
        assert_eq!(v, [2, 1, 3]);
        assert!(shift(&mut v, |x| *x == 2, false));
        assert_eq!(v, [1, 2, 3]);
        assert!(!shift(&mut v, |x| *x == 1, true));
        assert!(!shift(&mut v, |x| *x == 3, false));
        assert!(!shift(&mut v, |x| *x == 9, true));
        assert_eq!(v, [1, 2, 3]);
    }
}
