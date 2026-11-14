//! Union-find over dense `u32` indices.
//!
//! The representative of a set is always its smallest member. That makes the
//! representative a canonical identity: it does not depend on the order in
//! which unions were applied, so it can be exposed in results.

/// Disjoint sets over `0..len`.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct UnionFind {
    parent: Vec<u32>,
}

impl UnionFind {
    /// `len` singleton sets.
    #[must_use]
    pub fn new(len: u32) -> Self {
        Self {
            parent: (0..len).collect(),
        }
    }

    /// Number of elements.
    #[must_use]
    pub fn len(&self) -> usize {
        self.parent.len()
    }

    /// Whether there is no element.
    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.parent.is_empty()
    }

    /// Split everything back into singleton sets, keeping the allocation.
    pub fn reset(&mut self) {
        for (index, parent) in (0_u32..).zip(self.parent.iter_mut()) {
            *parent = index;
        }
    }

    /// Smallest member of the set containing `element`. An element outside
    /// the range is its own set.
    pub fn find(&mut self, element: u32) -> u32 {
        let mut current = element;
        loop {
            let Some(&parent) = self.parent.get(current as usize) else {
                return current;
            };
            if parent == current {
                return current;
            }
            // Path halving: point at the grandparent while walking up.
            let grandparent = self.parent.get(parent as usize).copied().unwrap_or(parent);
            if let Some(slot) = self.parent.get_mut(current as usize) {
                *slot = grandparent;
            }
            current = grandparent;
        }
    }

    /// Merge the sets containing `a` and `b`.
    pub fn union(&mut self, a: u32, b: u32) {
        let root_a = self.find(a);
        let root_b = self.find(b);
        if root_a == root_b {
            return;
        }
        let (keep, merge) = if root_a < root_b {
            (root_a, root_b)
        } else {
            (root_b, root_a)
        };
        if let Some(slot) = self.parent.get_mut(merge as usize) {
            *slot = keep;
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn representative_is_the_smallest_member_for_any_union_order() {
        let pairs = [(4, 2), (2, 7), (9, 7), (1, 3)];
        let mut forward = UnionFind::new(10);
        let mut backward = UnionFind::new(10);
        for (a, b) in pairs {
            forward.union(a, b);
        }
        for (a, b) in pairs.iter().rev() {
            backward.union(*b, *a);
        }

        let roots = |sets: &mut UnionFind| (0..10).map(|e| sets.find(e)).collect::<Vec<_>>();

        assert_eq!(roots(&mut forward), [0, 1, 2, 1, 2, 5, 6, 2, 8, 2]);
        assert_eq!(roots(&mut backward), roots(&mut forward));
    }

    #[test]
    fn reset_restores_singletons() {
        let mut sets = UnionFind::new(4);
        sets.union(0, 3);
        sets.reset();

        assert_eq!(sets.find(3), 3);
    }

    #[test]
    fn out_of_range_elements_are_their_own_set() {
        let mut sets = UnionFind::new(2);
        sets.union(0, 99);

        assert_eq!(sets.find(99), 99);
        assert_eq!(sets.find(0), 0);
    }

    #[test]
    fn long_chains_terminate() {
        let mut sets = UnionFind::new(10_000);
        for element in (1..10_000).rev() {
            sets.union(element, element - 1);
        }

        assert_eq!(sets.find(9_999), 0);
    }
}
