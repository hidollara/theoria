//! Evaluate failure sets after a backend has supplied attribute dependencies.

/// Each node describes a set of possible failure contracts, not runtime values.
pub enum Node<'a> {
    Union(&'a [usize]),
    Reasons(&'a [usize]),
    References {
        selected: &'a [bool],
        sources: &'a [usize],
    },
    Filter {
        source: usize,
        reasons: &'a [usize],
    },
    When {
        condition: usize,
        value: usize,
    },
}

/// A monotone finite-set fixed point also supports forward binding references.
pub const fn evaluate<const N: usize, const E: usize>(nodes: &[Node<'_>]) -> [[bool; E]; N] {
    assert!(nodes.len() == N);
    let mut sets = [[false; E]; N];
    loop {
        let mut changed = false;
        let mut i = 0;
        while i < N {
            let mut e = 0;
            while e < E {
                let present = present(&nodes[i], e, E, sets.as_flattened());
                if present && !sets[i][e] {
                    sets[i][e] = true;
                    changed = true;
                }
                e += 1;
            }
            i += 1;
        }
        if !changed {
            return sets;
        }
    }
}

const fn contains(values: &[usize], value: usize) -> bool {
    let mut i = 0;
    while i < values.len() {
        if values[i] == value {
            return true;
        }
        i += 1;
    }
    false
}

const fn present(node: &Node<'_>, e: usize, width: usize, sets: &[bool]) -> bool {
    match node {
        Node::Union(sources) => {
            let mut found = false;
            let mut j = 0;
            while j < sources.len() {
                found |= sets[sources[j] * width + e];
                j += 1;
            }
            found
        }
        Node::Reasons(reasons) => contains(reasons, e),
        Node::References { selected, sources } => {
            assert!(selected.len() == sources.len());
            let mut found = false;
            let mut j = 0;
            while j < sources.len() {
                found |= selected[j] && sets[sources[j] * width + e];
                j += 1;
            }
            found
        }
        Node::Filter { source, reasons } => sets[*source * width + e] && contains(reasons, e),
        Node::When { condition, value } => {
            let mut any = false;
            let mut j = 0;
            while j < width {
                any |= sets[*condition * width + j];
                j += 1;
            }
            any && sets[*value * width + e]
        }
    }
}

/// Evaluate the same graph when its dimensions are known only at runtime.
pub fn evaluate_dynamic(nodes: &[Node<'_>], reasons: usize) -> Vec<Vec<bool>> {
    let mut sets = vec![false; nodes.len() * reasons];
    loop {
        let mut changed = false;
        for (i, node) in nodes.iter().enumerate() {
            for e in 0..reasons {
                let index = i * reasons + e;
                if present(node, e, reasons, &sets) && !sets[index] {
                    sets[index] = true;
                    changed = true;
                }
            }
        }
        if !changed {
            return (0..nodes.len())
                .map(|i| sets[i * reasons..(i + 1) * reasons].to_vec())
                .collect();
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn runtime_and_const_evaluation_share_forward_references_and_handler_rules() {
        const NODES: &[Node<'_>] = &[
            Node::Union(&[1]),
            Node::Reasons(&[0, 1]),
            Node::References {
                selected: &[true],
                sources: &[0],
            },
            Node::Filter {
                source: 2,
                reasons: &[1],
            },
            Node::When {
                condition: 3,
                value: 5,
            },
            Node::Reasons(&[2]),
            Node::Union(&[3, 4]),
        ];
        const SETS: [[bool; 3]; 7] = evaluate(NODES);
        assert_eq!(SETS[6], [false, true, true]);
        assert_eq!(evaluate_dynamic(NODES, 3), SETS.map(|row| row.to_vec()));
        assert_eq!(
            evaluate_dynamic(&[Node::Union(&[])], 0),
            [Vec::<bool>::new()]
        );
    }
}
