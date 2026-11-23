//! The sets the flattening keeps are runs of consecutive numbers, put in order and merged: they hold
//! exactly the numbers put in them, in one form, whatever order and however often they were put.

use std::collections::BTreeSet;

use salsa::{RunBuilder, Runs};

/// A repeatable sequence of numbers: a test of sets that is the same every time it is run.
struct Sequence(u64);

impl Sequence {
    fn next(&mut self) -> u64 {
        self.0 = self
            .0
            .wrapping_mul(6_364_136_223_846_793_005)
            .wrapping_add(1_442_695_040_888_963_407);
        self.0 >> 33
    }

    /// A number below `limit`.
    fn below(&mut self, limit: u32) -> u32 {
        u32::try_from(self.next() % u64::from(limit)).expect("below a u32")
    }

    /// Numbers in a handful of clusters, as the edges of a crate's files are.
    fn numbers(&mut self, limit: u32) -> Vec<u32> {
        let mut numbers = Vec::new();
        for _ in 0..self.below(6) {
            let start = self.below(limit);
            let length = self.below(40);
            numbers.extend((start..start.saturating_add(length)).filter(|number| *number < limit));
        }
        numbers
    }
}

fn built(numbers: &[u32]) -> Runs {
    let mut builder = RunBuilder::default();
    for number in numbers {
        builder.set(*number);
    }
    builder.build()
}

fn model(numbers: &[u32]) -> BTreeSet<u32> {
    numbers.iter().copied().collect()
}

fn members(set: &Runs) -> BTreeSet<u32> {
    set.numbers().collect()
}

/// The runs a set of numbers is, by the plainest means.
fn expected_runs(numbers: &BTreeSet<u32>) -> Vec<(u32, u32)> {
    let mut runs: Vec<(u32, u32)> = Vec::new();
    for number in numbers {
        match runs.last_mut() {
            Some((first, length)) if *first + *length == *number => *length += 1,
            _ => runs.push((*number, 1)),
        }
    }
    runs
}

#[test]
fn a_set_holds_exactly_the_numbers_put_in_it_as_maximal_runs() {
    let mut sequence = Sequence(1);
    for _ in 0..500 {
        let numbers = sequence.numbers(10_000);
        let set = built(&numbers);
        let wanted = model(&numbers);
        assert_eq!(members(&set), wanted);
        assert_eq!(
            set.runs(),
            expected_runs(&wanted),
            "sorted, disjoint, never adjacent"
        );
        assert_eq!(set.len(), wanted.len() as u64);
        assert_eq!(set.is_empty(), wanted.is_empty());
    }
}

#[test]
fn equal_sets_are_equal_whatever_order_they_were_built_in() {
    let mut sequence = Sequence(2);
    for _ in 0..200 {
        let mut numbers = sequence.numbers(5_000);
        let forward = built(&numbers);
        numbers.reverse();
        let backward = built(&numbers);
        // And with every number put in twice.
        let twice = built(&[numbers.clone(), numbers].concat());
        assert_eq!(forward, backward);
        assert_eq!(forward, twice);
        let hashed: BTreeSet<_> = [forward, backward, twice]
            .into_iter()
            .map(|set| set.runs().to_vec())
            .collect();
        assert_eq!(hashed.len(), 1);
    }
}

#[test]
fn a_union_holds_what_both_held() {
    let mut sequence = Sequence(3);
    for _ in 0..300 {
        let (left, right) = (sequence.numbers(3_000), sequence.numbers(3_000));
        let mut builder = RunBuilder::default();
        for number in &left {
            builder.set(*number);
        }
        builder.or(&built(&right));
        let union = builder.build();
        let wanted: BTreeSet<u32> = model(&left).union(&model(&right)).copied().collect();
        assert_eq!(members(&union), wanted);
        assert_eq!(union.runs(), expected_runs(&wanted));
    }
}

#[test]
fn joining_many_sets_is_the_same_as_joining_them_once_each() {
    // Enough joins that the builder puts its runs in order while it grows, with sets that overlap.
    let mut sequence = Sequence(4);
    let mut builder = RunBuilder::default();
    let mut wanted = BTreeSet::new();
    for _ in 0..400 {
        let numbers = sequence.numbers(2_000);
        wanted.extend(numbers.iter().copied());
        builder.or(&built(&numbers));
    }
    let union = builder.build();
    assert_eq!(members(&union), wanted);
    assert_eq!(union.runs(), expected_runs(&wanted));
}

#[test]
fn a_set_of_one_high_number_is_one_run() {
    let set = built(&[4_000_000_000]);
    assert_eq!(set.runs(), [(4_000_000_000, 1)]);
    let both = {
        let mut builder = RunBuilder::default();
        builder.set(0);
        builder.or(&set);
        builder.build()
    };
    assert_eq!(both.runs(), [(0, 1), (4_000_000_000, 1)]);
}

#[test]
fn the_empty_set_is_one_set() {
    assert_eq!(built(&[]), Runs::default());
    assert!(built(&[]).is_empty());
}
