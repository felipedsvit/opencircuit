//! Collections kept sorted by ID.
//!
//! Every collection of a document under edit stays in canonical order, so an
//! insertion and the removal that undoes it restore the exact same vector.

/// Where the entity with `id` is, or where it would go.
pub(crate) fn position<T, K: Ord>(
    items: &[T],
    id: &K,
    key: impl Fn(&T) -> K,
) -> Result<usize, usize> {
    items.binary_search_by(|item| key(item).cmp(id))
}

/// Insert keeping the order. `false` when the ID is already present.
pub(crate) fn insert<T, K: Ord>(items: &mut Vec<T>, item: T, key: impl Fn(&T) -> K) -> bool {
    let id = key(&item);
    match position(items, &id, &key) {
        Ok(_) => false,
        Err(at) => {
            items.insert(at, item);
            true
        }
    }
}

/// Remove the entity with `id`, if present.
pub(crate) fn remove<T, K: Ord>(items: &mut Vec<T>, id: &K, key: impl Fn(&T) -> K) -> Option<T> {
    let at = position(items, id, key).ok()?;
    // `at` came from a successful search of this very vector.
    (at < items.len()).then(|| items.remove(at))
}

/// The entity with `id`, mutably.
pub(crate) fn find_mut<'a, T, K: Ord>(
    items: &'a mut [T],
    id: &K,
    key: impl Fn(&T) -> K,
) -> Option<&'a mut T> {
    let at = position(items, id, key).ok()?;
    items.get_mut(at)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn insert_keeps_the_order_and_refuses_a_duplicate() {
        let mut items = vec![1, 3, 5];

        assert!(insert(&mut items, 4, |v| *v));
        assert!(!insert(&mut items, 3, |v| *v));
        assert_eq!(items, [1, 3, 4, 5]);
    }

    #[test]
    fn remove_after_insert_restores_the_vector() {
        let mut items = vec![1, 3, 5];

        insert(&mut items, 2, |v| *v);
        let removed = remove(&mut items, &2, |v| *v);

        assert_eq!((removed, items), (Some(2), vec![1, 3, 5]));
    }

    #[test]
    fn missing_id_is_neither_removed_nor_found() {
        let mut items = vec![1, 3];

        assert_eq!(remove(&mut items, &2, |v| *v), None);
        assert_eq!(find_mut(&mut items, &2, |v| *v), None);
    }
}
