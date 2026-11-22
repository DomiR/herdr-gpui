//! The operations as a graph: one deterministic order in which every
//! operation follows its parents (ties broken by id), and each operation's
//! ancestors as a bit set over that order, so "has this one seen that one"
//! is one lookup.

use super::{Error, Op, OpId, Result};
use std::collections::{BTreeMap, BTreeSet, HashMap};

/// A set of operations, by their place in [`Graph::order`].
#[derive(Clone, Debug, Default, Eq, Hash, PartialEq)]
pub(super) struct Bits(Vec<u64>);

impl Bits {
    pub(super) fn new(len: usize) -> Self {
        Self(vec![0; len.div_ceil(64)])
    }

    pub(super) fn full(len: usize) -> Self {
        let mut bits = Self::new(len);
        (0..len).for_each(|index| bits.insert(index));
        bits
    }

    pub(super) fn insert(&mut self, index: usize) {
        self.0[index / 64] |= 1 << (index % 64);
    }

    pub(super) fn contains(&self, index: usize) -> bool {
        self.0
            .get(index / 64)
            .is_some_and(|word| word & (1 << (index % 64)) != 0)
    }

    pub(super) fn union(&mut self, other: &Self) {
        self.0
            .iter_mut()
            .zip(&other.0)
            .for_each(|(word, other)| *word |= other);
    }

    /// The members also in `mask`.
    pub(super) fn masked(&self, mask: &Self) -> Self {
        Self(
            self.0
                .iter()
                .zip(&mask.0)
                .map(|(word, mask)| word & mask)
                .collect(),
        )
    }

    pub(super) fn iter(&self) -> impl Iterator<Item = usize> + '_ {
        self.0.iter().enumerate().flat_map(|(word_index, word)| {
            (0..64)
                .filter(move |bit| word & (1 << bit) != 0)
                .map(move |bit| word_index * 64 + bit)
        })
    }
}

/// The operations of a log, ordered.
#[derive(Debug)]
pub(super) struct Graph<'a> {
    /// Every operation, each after its parents.
    pub(super) order: Vec<&'a Op>,
    /// Each operation's place in `order`.
    pub(super) index: HashMap<OpId, usize>,
    /// Each operation's strict ancestors.
    pub(super) ancestors: Vec<Bits>,
}

impl<'a> Graph<'a> {
    /// Orders `ops`. A parent that is not among them must be `root`, the
    /// checkpoint the log was pruned to, which every kept operation
    /// descends from.
    pub(super) fn new(ops: &'a BTreeMap<OpId, Op>, root: Option<OpId>) -> Result<Self> {
        let mut children: HashMap<OpId, Vec<OpId>> = HashMap::new();
        let mut waiting: HashMap<OpId, usize> = HashMap::new();
        let mut ready = BTreeSet::new();
        for (id, op) in ops {
            let mut count = 0;
            for parent in op.parents() {
                if ops.contains_key(parent) {
                    children.entry(*parent).or_default().push(*id);
                    count += 1;
                } else if Some(*parent) != root {
                    return Err(Error::ParentMissing);
                }
            }
            if count == 0 {
                ready.insert(*id);
            } else {
                waiting.insert(*id, count);
            }
        }
        let mut order = Vec::with_capacity(ops.len());
        let mut index = HashMap::with_capacity(ops.len());
        while let Some(id) = ready.pop_first() {
            index.insert(id, order.len());
            order.extend(ops.get(&id));
            for child in children.get(&id).into_iter().flatten() {
                if let Some(count) = waiting.get_mut(child) {
                    *count -= 1;
                    if *count == 0 {
                        waiting.remove(child);
                        ready.insert(*child);
                    }
                }
            }
        }
        // Hashes cannot form a cycle, but a log read from disk is checked
        // all the same.
        if order.len() != ops.len() {
            return Err(Error::Invalid);
        }
        let mut ancestors: Vec<Bits> = Vec::with_capacity(order.len());
        for op in &order {
            let mut bits = Bits::new(order.len());
            for parent in op.parents() {
                if let Some(&position) = index.get(parent) {
                    bits.union(&ancestors[position]);
                    bits.insert(position);
                }
            }
            ancestors.push(bits);
        }
        Ok(Self {
            order,
            index,
            ancestors,
        })
    }

    pub(super) fn len(&self) -> usize {
        self.order.len()
    }

    /// Whether `ancestor` is a strict ancestor of `of`.
    pub(super) fn sees(&self, of: usize, ancestor: usize) -> bool {
        self.ancestors[of].contains(ancestor)
    }

    /// Whether neither has seen the other.
    pub(super) fn concurrent(&self, one: usize, other: usize) -> bool {
        one != other && !self.sees(one, other) && !self.sees(other, one)
    }

    /// Operations no other operation names as a parent.
    pub(super) fn heads(&self) -> Vec<OpId> {
        let mut named = Bits::new(self.len());
        for op in &self.order {
            for parent in op.parents() {
                if let Some(&position) = self.index.get(parent) {
                    named.insert(position);
                }
            }
        }
        (0..self.len())
            .filter(|position| !named.contains(*position))
            .map(|position| self.order[position].id())
            .collect()
    }
}
