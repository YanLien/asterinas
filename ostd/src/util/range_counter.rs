// SPDX-License-Identifier: MPL-2.0

//! A data structure that tracks a contiguous range of counters.

use alloc::{boxed::Box, vec::Vec};
use core::ops::Range;

/// A contiguous range of counters.
pub(crate) struct RangeCounter {
    root: Option<Box<SegmentTreeNode>>,
}

impl RangeCounter {
    /// Creates a new [`RangeCounter`].
    ///
    /// The default value for all counters is zero.
    pub(crate) const fn new() -> Self {
        Self { root: None }
    }

    /// Returns the counter at the index.
    #[cfg(ktest)]
    pub(crate) fn get(&self, index: usize) -> usize {
        if index >= ROOT_END {
            return 0;
        }

        self.root
            .as_ref()
            .map_or(0, |root| root.get(ROOT_START, ROOT_END, index))
    }

    /// Adds one new count for all counters in the range.
    ///
    /// Returns ranges that the counter has updated from zero to one.
    ///
    /// # Panics
    ///
    /// Panics if the range has a negative size.
    pub(crate) fn add(&mut self, range: &Range<usize>) -> impl Iterator<Item = Range<usize>> {
        assert!(range.start <= range.end);
        let mut updated_ranges = Vec::new();

        if range.is_empty() {
            return updated_ranges.into_iter();
        }

        let root = self
            .root
            .get_or_insert_with(|| Box::new(SegmentTreeNode::new_zero()));
        root.add_range(ROOT_START, ROOT_END, range, &mut updated_ranges);

        updated_ranges.into_iter()
    }

    /// Removes one count for all counters in the range.
    ///
    /// Returns ranges that the counter has updated from one to zero.
    ///
    /// # Panics
    ///
    /// Panics if
    ///  - the range has a negative size, or
    ///  - the range contains a counter that is already zero.
    pub(crate) fn remove(&mut self, range: &Range<usize>) -> impl Iterator<Item = Range<usize>> {
        assert!(range.start <= range.end);
        let mut updated_ranges = Vec::new();

        if range.is_empty() {
            return updated_ranges.into_iter();
        }

        let root = self.root.as_mut().expect("Removing a zero counter");
        root.remove_range(ROOT_START, ROOT_END, range, &mut updated_ranges);
        if root.is_zero() {
            self.root = None;
        }

        updated_ranges.into_iter()
    }
}

const ROOT_START: usize = 0;
const ROOT_END: usize = usize::MAX;

/// A node in an implicit segment tree over a half-open index range.
///
/// Children are allocated only after an update needs to distinguish the two
/// halves of the node range. This keeps sparse high-address ranges cheap while
/// still making range updates proportional to the tree height and the number of
/// reported transition ranges.
struct SegmentTreeNode {
    /// The minimum counter value in the segment.
    min_count: usize,
    /// The maximum counter value in the segment.
    max_count: usize,
    /// The count delta that applies uniformly below this node.
    lazy_add: usize,
    left: Option<Box<SegmentTreeNode>>,
    right: Option<Box<SegmentTreeNode>>,
}

impl SegmentTreeNode {
    /// Creates a segment whose counters are all zero.
    fn new_zero() -> Self {
        Self::new_uniform(0)
    }

    /// Creates a segment whose counters are all equal to `count`.
    fn new_uniform(count: usize) -> Self {
        Self {
            min_count: count,
            max_count: count,
            lazy_add: count,
            left: None,
            right: None,
        }
    }

    /// Returns the counter value at `index`.
    #[cfg(ktest)]
    fn get(&self, start: usize, end: usize, index: usize) -> usize {
        if self.min_count == self.max_count {
            return self.min_count;
        }

        let mid = mid(start, end);
        let child = if index < mid { &self.left } else { &self.right };
        let child_count = child.as_ref().map_or(0, |child| {
            if index < mid {
                child.get(start, mid, index)
            } else {
                child.get(mid, end, index)
            }
        });
        child_count + self.lazy_add
    }

    /// Increments counters in `range` and records subranges that change from zero to one.
    fn add_range(
        &mut self,
        start: usize,
        end: usize,
        range: &Range<usize>,
        updated_ranges: &mut Vec<Range<usize>>,
    ) {
        if !ranges_overlap(start..end, range) {
            return;
        }
        if range.start <= start && end <= range.end {
            // The caller needs to know which subranges become newly active, so
            // record zero-covered parts before the lazy increment hides them.
            self.collect_zero_ranges(start, end, updated_ranges);
            self.apply_add(1);
            return;
        }

        self.push_down(start, end);
        let mid = mid(start, end);
        if range.start < mid {
            self.left
                .get_or_insert_with(|| Box::new(Self::new_zero()))
                .add_range(start, mid, range, updated_ranges);
        }
        if mid < range.end {
            self.right
                .get_or_insert_with(|| Box::new(Self::new_zero()))
                .add_range(mid, end, range, updated_ranges);
        }
        self.pull_up();
    }

    /// Decrements counters in `range` and records subranges that change from one to zero.
    fn remove_range(
        &mut self,
        start: usize,
        end: usize,
        range: &Range<usize>,
        updated_ranges: &mut Vec<Range<usize>>,
    ) {
        if !ranges_overlap(start..end, range) {
            return;
        }
        if range.start <= start && end <= range.end {
            // Removing must report the pages that become inactive and reject
            // any attempt to decrement an already-zero counter.
            self.collect_one_ranges(start, end, updated_ranges);
            self.decrement_covered_range(start, end);
            return;
        }

        if self.max_count == 0 {
            panic!("Removing a zero counter");
        }
        self.push_down(start, end);
        let mid = mid(start, end);
        if range.start < mid {
            self.left
                .as_mut()
                .expect("Removing a zero counter")
                .remove_range(start, mid, range, updated_ranges);
        }
        if mid < range.end {
            self.right
                .as_mut()
                .expect("Removing a zero counter")
                .remove_range(mid, end, range, updated_ranges);
        }
        self.pull_up();
    }

    /// Collects zero-valued subranges covered by this node.
    fn collect_zero_ranges(
        &mut self,
        start: usize,
        end: usize,
        updated_ranges: &mut Vec<Range<usize>>,
    ) {
        if self.min_count > 0 {
            return;
        }
        if self.max_count == 0 {
            push_updated_range(updated_ranges, start..end);
            return;
        }

        self.push_down(start, end);
        let mid = mid(start, end);
        self.left
            .as_mut()
            .unwrap()
            .collect_zero_ranges(start, mid, updated_ranges);
        self.right
            .as_mut()
            .unwrap()
            .collect_zero_ranges(mid, end, updated_ranges);
    }

    /// Collects one-valued subranges covered by this node.
    fn collect_one_ranges(
        &mut self,
        start: usize,
        end: usize,
        updated_ranges: &mut Vec<Range<usize>>,
    ) {
        if self.min_count == 0 {
            panic!("Removing a zero counter");
        }
        if self.min_count > 1 {
            return;
        }
        if self.max_count == 1 {
            push_updated_range(updated_ranges, start..end);
            return;
        }

        self.push_down(start, end);
        let mid = mid(start, end);
        self.left
            .as_mut()
            .unwrap()
            .collect_one_ranges(start, mid, updated_ranges);
        self.right
            .as_mut()
            .unwrap()
            .collect_one_ranges(mid, end, updated_ranges);
    }

    /// Decrements every counter covered by this node.
    fn decrement_covered_range(&mut self, start: usize, end: usize) {
        if self.min_count == self.max_count {
            self.apply_sub(1);
            return;
        }

        self.push_down(start, end);
        let mid = mid(start, end);
        self.left
            .as_mut()
            .unwrap()
            .decrement_covered_range(start, mid);
        self.right
            .as_mut()
            .unwrap()
            .decrement_covered_range(mid, end);
        self.pull_up();
    }

    /// Applies a uniform positive delta to this node.
    fn apply_add(&mut self, delta: usize) {
        self.min_count = self.min_count.checked_add(delta).expect("Counter overflow");
        self.max_count = self.max_count.checked_add(delta).expect("Counter overflow");
        self.lazy_add = self.lazy_add.checked_add(delta).expect("Counter overflow");
    }

    /// Applies a uniform negative delta to this node.
    fn apply_sub(&mut self, delta: usize) {
        self.min_count = self
            .min_count
            .checked_sub(delta)
            .expect("Removing a zero counter");
        self.max_count = self
            .max_count
            .checked_sub(delta)
            .expect("Removing a zero counter");
        self.lazy_add = self
            .lazy_add
            .checked_sub(delta)
            .expect("Removing a zero counter");
    }

    /// Materializes child nodes and propagates this node's lazy delta to them.
    fn push_down(&mut self, start: usize, end: usize) {
        if end - start == 1 {
            return;
        }

        // New children start at zero and receive `lazy_add` below. This avoids
        // materializing the full tree for untouched address ranges.
        if self.left.is_none() {
            self.left = Some(Box::new(Self::new_zero()));
        }
        if self.right.is_none() {
            self.right = Some(Box::new(Self::new_zero()));
        }

        if self.lazy_add == 0 {
            return;
        }

        let lazy_add = self.lazy_add;
        self.left.as_mut().unwrap().apply_add(lazy_add);
        self.right.as_mut().unwrap().apply_add(lazy_add);
        self.lazy_add = 0;
    }

    /// Refreshes this node's summary from its children.
    fn pull_up(&mut self) {
        let left = self.left.as_ref().unwrap();
        let right = self.right.as_ref().unwrap();
        self.min_count = left.min_count.min(right.min_count);
        self.max_count = left.max_count.max(right.max_count);
        self.lazy_add = 0;

        if self.min_count == self.max_count {
            self.left = None;
            self.right = None;
            self.lazy_add = self.min_count;
        }
    }

    /// Returns whether every counter in this node is zero.
    fn is_zero(&self) -> bool {
        self.min_count == 0 && self.max_count == 0
    }
}

/// Returns the midpoint of a half-open range.
fn mid(start: usize, end: usize) -> usize {
    start + (end - start) / 2
}

/// Returns whether two half-open ranges overlap.
fn ranges_overlap(a: Range<usize>, b: &Range<usize>) -> bool {
    a.start < b.end && b.start < a.end
}

/// Appends a reported transition range and merges it with the previous one if adjacent.
fn push_updated_range(updated_ranges: &mut Vec<Range<usize>>, range: Range<usize>) {
    if range.is_empty() {
        return;
    }

    if let Some(last_range) = updated_ranges.last_mut() {
        if last_range.end == range.start {
            last_range.end = range.end;
            return;
        }
    }

    updated_ranges.push(range);
}

#[cfg(ktest)]
mod test {
    use alloc::{collections::btree_map::BTreeMap, vec};
    use core::hint;

    use super::*;
    use crate::{arch, prelude::*};

    /// A macro to check counter values for multiple ranges.
    ///
    /// # Examples
    ///
    /// ```ignore
    /// check_counter_values!(counter, [15..20, 1], [20..30, 2], [30..35, 1])
    /// ```
    macro_rules! check_counter_values {
        ($counter:expr, $([$range:expr, $expected:expr]),* $(,)?) => {
            $(
                for i in $range {
                    assert_eq!($counter.get(i), $expected,
                        "Counter at index {} should be {}, but got {}",
                        i, $expected, $counter.get(i));
                }
            )*
        };
    }

    #[ktest]
    fn add_remove_range() {
        let mut counter = RangeCounter::new();
        let range = 0..5;

        assert_eq!(counter.add(&range).collect::<Vec<_>>(), vec![range.clone()]);
        check_counter_values!(counter, [range.clone(), 1]);

        assert_eq!(
            counter.remove(&range).collect::<Vec<_>>(),
            vec![range.clone()]
        );
        check_counter_values!(counter, [range, 0]);
    }

    #[ktest]
    fn add_remove_overlapping_beginning() {
        let mut counter = RangeCounter::new();
        let range1 = 10..15;
        let range2 = 3..13;

        assert_eq!(
            counter.add(&range1).collect::<Vec<_>>(),
            vec![range1.clone()]
        );
        assert_eq!(counter.add(&range2).collect::<Vec<_>>(), vec![3..10]);

        check_counter_values!(counter, [3..10, 1], [10..13, 2], [13..15, 1]);

        assert_eq!(counter.remove(&range2).collect::<Vec<_>>(), vec![3..10]);

        check_counter_values!(counter, [3..10, 0], [10..13, 1], [13..15, 1]);
    }

    #[ktest]
    fn add_remove_overlapping_end() {
        let mut counter = RangeCounter::new();
        let range1 = 10..15;
        let range2 = 12..18;

        assert_eq!(
            counter.add(&range1).collect::<Vec<_>>(),
            vec![range1.clone()]
        );
        assert_eq!(counter.add(&range2).collect::<Vec<_>>(), vec![15..18]);

        check_counter_values!(counter, [10..12, 1], [12..15, 2], [15..18, 1]);

        assert_eq!(counter.remove(&range2).collect::<Vec<_>>(), vec![15..18]);

        check_counter_values!(counter, [10..12, 1], [12..15, 1], [15..18, 0]);
    }

    #[ktest]
    fn add_remove_covering() {
        let mut counter = RangeCounter::new();
        let range1 = 20..30;
        let range2 = 15..35;

        assert_eq!(
            counter.add(&range1).collect::<Vec<_>>(),
            vec![range1.clone()]
        );
        assert_eq!(
            counter.add(&range2).collect::<Vec<_>>(),
            vec![15..20, 30..35]
        );

        check_counter_values!(counter, [15..20, 1], [20..30, 2], [30..35, 1]);

        assert_eq!(
            counter.remove(&range2).collect::<Vec<_>>(),
            vec![15..20, 30..35]
        );

        check_counter_values!(counter, [15..20, 0], [20..30, 1], [30..35, 0]);
    }

    #[ktest]
    fn add_remove_partial_overlap() {
        let mut counter = RangeCounter::new();
        let range1 = 5..15;
        let range2 = 10..20;
        let remove_range = 8..12;

        assert_eq!(
            counter.add(&range1).collect::<Vec<_>>(),
            vec![range1.clone()]
        );
        assert_eq!(counter.add(&range2).collect::<Vec<_>>(), vec![15..20]);

        check_counter_values!(counter, [5..10, 1], [10..15, 2], [15..20, 1]);

        assert_eq!(
            counter.remove(&remove_range).collect::<Vec<_>>(),
            vec![8..10]
        );

        check_counter_values!(
            counter,
            [5..8, 1],
            [8..10, 0],
            [10..12, 1],
            [12..15, 2],
            [15..20, 1]
        );
    }

    #[ktest]
    fn add_remove_sparse_large_ranges() {
        let mut counter = RangeCounter::new();
        let range1 = (usize::MAX / 2)..(usize::MAX / 2 + 8);
        let range2 = (usize::MAX / 2 + 4)..(usize::MAX / 2 + 12);

        assert_eq!(
            counter.add(&range1).collect::<Vec<_>>(),
            vec![range1.clone()]
        );
        assert_eq!(
            counter.add(&range2).collect::<Vec<_>>(),
            vec![(usize::MAX / 2 + 8)..(usize::MAX / 2 + 12)]
        );
        assert_eq!(
            counter.remove(&range1).collect::<Vec<_>>(),
            vec![(usize::MAX / 2)..(usize::MAX / 2 + 4)]
        );

        check_counter_values!(
            counter,
            [(usize::MAX / 2)..(usize::MAX / 2 + 4), 0],
            [(usize::MAX / 2 + 4)..(usize::MAX / 2 + 8), 1],
            [(usize::MAX / 2 + 8)..(usize::MAX / 2 + 12), 1],
        );
    }

    #[ktest]
    fn same_behavior_as_simple_counter() {
        let mut counter = RangeCounter::new();
        let mut reference = BTreeMap::new();
        let ranges = [0..9, 4..12, 16..24, 8..20, 2..6, 21..28, 0..28, 13..17];

        for range in ranges {
            let expected = reference_add(&mut reference, &range);
            assert_eq!(counter.add(&range).collect::<Vec<_>>(), expected);
        }

        let ranges = [13..17, 0..28, 21..28, 2..6, 8..20, 16..24, 4..12, 0..9];
        for range in ranges {
            let expected = reference_remove(&mut reference, &range);
            assert_eq!(counter.remove(&range).collect::<Vec<_>>(), expected);
        }

        check_counter_values!(counter, [0..28, 0]);
    }

    #[ktest]
    fn benchmark_against_simple_counter() {
        const NR_RANGES: usize = 128;
        const RANGE_LEN: usize = 4096;
        const RANGE_STRIDE: usize = 2048;
        const BASE: usize = usize::MAX / 4;

        let mut counter = RangeCounter::new();
        let segment_tree_start = arch::read_tsc();
        let mut segment_tree_transitions = 0;
        for index in 0..NR_RANGES {
            let range = benchmark_range(BASE, index, RANGE_LEN, RANGE_STRIDE);
            segment_tree_transitions += counter.add(&range).count();
        }
        for index in (0..NR_RANGES).rev() {
            let range = benchmark_range(BASE, index, RANGE_LEN, RANGE_STRIDE);
            segment_tree_transitions += counter.remove(&range).count();
        }
        let segment_tree_cycles = arch::read_tsc().wrapping_sub(segment_tree_start);

        let mut reference = BTreeMap::new();
        let reference_start = arch::read_tsc();
        let mut reference_transitions = 0;
        for index in 0..NR_RANGES {
            let range = benchmark_range(BASE, index, RANGE_LEN, RANGE_STRIDE);
            reference_transitions += reference_add(&mut reference, &range).len();
        }
        for index in (0..NR_RANGES).rev() {
            let range = benchmark_range(BASE, index, RANGE_LEN, RANGE_STRIDE);
            reference_transitions += reference_remove(&mut reference, &range).len();
        }
        let reference_cycles = arch::read_tsc().wrapping_sub(reference_start);

        hint::black_box(segment_tree_transitions);
        hint::black_box(reference_transitions);
        println!(
            "range counter benchmark: segment_tree={} cycles, btree_map={} cycles",
            segment_tree_cycles, reference_cycles
        );
    }

    fn benchmark_range(
        base: usize,
        index: usize,
        range_len: usize,
        range_stride: usize,
    ) -> Range<usize> {
        let start = base + index * range_stride;
        start..start + range_len
    }

    fn reference_add(
        reference: &mut BTreeMap<usize, usize>,
        range: &Range<usize>,
    ) -> Vec<Range<usize>> {
        let mut updated_ranges = Vec::new();

        for index in range.clone() {
            let count = reference.entry(index).or_insert(0);
            if *count == 0 {
                push_reference_range(&mut updated_ranges, index..index + 1);
            }
            *count += 1;
        }

        updated_ranges
    }

    fn reference_remove(
        reference: &mut BTreeMap<usize, usize>,
        range: &Range<usize>,
    ) -> Vec<Range<usize>> {
        let mut updated_ranges = Vec::new();

        for index in range.clone() {
            let count = reference.get_mut(&index).expect("Removing a zero counter");
            if *count == 1 {
                push_reference_range(&mut updated_ranges, index..index + 1);
            }
            *count -= 1;
            if *count == 0 {
                reference.remove(&index);
            }
        }

        updated_ranges
    }

    fn push_reference_range(updated_ranges: &mut Vec<Range<usize>>, range: Range<usize>) {
        if let Some(last_range) = updated_ranges.last_mut() {
            if last_range.end == range.start {
                last_range.end = range.end;
                return;
            }
        }

        updated_ranges.push(range);
    }
}
