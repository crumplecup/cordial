//! Kahn validation and quality-area ranking.

use tracing::instrument;

use super::LintConstraint;
use crate::error::{CordialError, CordialResult};

pub(super) const MAX_NODES: usize = 256;

#[instrument(level = "debug", skip(constraints))]
pub(super) fn validate(known_ids: &[&str], constraints: &[LintConstraint]) -> CordialResult<()> {
    if !table_is_valid(known_ids, constraints) {
        if known_ids.len() > MAX_NODES {
            return Err(CordialError::lint_order_cycle());
        }
        for row in constraints {
            if index_of(known_ids, row.id).is_none() {
                return Err(CordialError::lint_order_unknown_constraint(row.id));
            }
            for predecessor in row.after {
                if index_of(known_ids, predecessor).is_none() {
                    return Err(CordialError::lint_order_dangling(row.id, *predecessor));
                }
            }
        }
        return Err(CordialError::lint_order_cycle());
    }
    Ok(())
}

pub(crate) const fn table_is_valid(known_ids: &[&str], constraints: &[LintConstraint]) -> bool {
    if known_ids.len() > MAX_NODES {
        return false;
    }
    let mut constraint_index = 0;
    while constraint_index < constraints.len() {
        let row = &constraints[constraint_index];
        if index_of(known_ids, row.id).is_none() {
            return false;
        }
        let mut after_index = 0;
        while after_index < row.after.len() {
            if index_of(known_ids, row.after[after_index]).is_none() {
                return false;
            }
            after_index += 1;
        }
        constraint_index += 1;
    }
    kahn_completes(known_ids, constraints)
}

const fn kahn_completes(known_ids: &[&str], constraints: &[LintConstraint]) -> bool {
    let n = known_ids.len();
    let mut indegree = [0usize; MAX_NODES];
    let mut constraint_index = 0;
    while constraint_index < constraints.len() {
        let row = &constraints[constraint_index];
        let Some(to) = index_of(known_ids, row.id) else {
            return false;
        };
        let mut after_index = 0;
        while after_index < row.after.len() {
            if index_of(known_ids, row.after[after_index]).is_none() {
                return false;
            }
            indegree[to] += 1;
            after_index += 1;
        }
        constraint_index += 1;
    }

    let mut remaining = n;
    let mut placed = [false; MAX_NODES];
    while remaining > 0 {
        let mut next = None;
        let mut index = 0;
        while index < n {
            if !placed[index] && indegree[index] == 0 {
                next = Some(index);
                break;
            }
            index += 1;
        }
        let Some(node) = next else {
            return false;
        };
        placed[node] = true;
        remaining -= 1;
        let id = known_ids[node];
        constraint_index = 0;
        while constraint_index < constraints.len() {
            let row = &constraints[constraint_index];
            let mut after_index = 0;
            while after_index < row.after.len() {
                if ids_eq(row.after[after_index], id)
                    && let Some(to) = index_of(known_ids, row.id)
                {
                    indegree[to] -= 1;
                }
                after_index += 1;
            }
            constraint_index += 1;
        }
    }
    true
}

#[instrument(level = "debug", skip(constraints))]
pub(super) fn ranked_ids(
    known_ids: &[std::borrow::Cow<'static, str>],
    constraints: &[LintConstraint],
) -> Vec<&'static str> {
    let n = known_ids.len();
    let mut indegree = vec![0usize; n];
    for row in constraints {
        let Some(to) = known_ids.iter().position(|id| id.as_ref() == row.id()) else {
            continue;
        };
        indegree[to] += row.after().len();
    }

    let mut placed = vec![false; n];
    let mut out = Vec::with_capacity(n);
    while out.len() < n {
        let mut candidates = Vec::new();
        for (index, degree) in indegree.iter().enumerate() {
            if !placed[index] && *degree == 0 {
                candidates.push(index);
            }
        }
        candidates.sort_by_key(|index| known_ids[*index].as_ref());
        let Some(node) = candidates.first().copied() else {
            break;
        };
        placed[node] = true;
        let id: &'static str = match &known_ids[node] {
            std::borrow::Cow::Borrowed(id) => id,
            std::borrow::Cow::Owned(_) => continue,
        };
        out.push(id);
        for row in constraints {
            if row.after().contains(&id)
                && let Some(to) = known_ids
                    .iter()
                    .position(|known| known.as_ref() == row.id())
            {
                indegree[to] -= 1;
            }
        }
    }
    out
}

const fn ids_eq(left: &str, right: &str) -> bool {
    let left = left.as_bytes();
    let right = right.as_bytes();
    if left.len() != right.len() {
        return false;
    }
    let mut index = 0;
    while index < left.len() {
        if left[index] != right[index] {
            return false;
        }
        index += 1;
    }
    true
}

const fn index_of(ids: &[&str], id: &str) -> Option<usize> {
    let mut index = 0;
    while index < ids.len() {
        if ids_eq(ids[index], id) {
            return Some(index);
        }
        index += 1;
    }
    None
}
