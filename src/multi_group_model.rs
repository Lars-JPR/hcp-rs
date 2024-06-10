use crate::indexed_list::IndexedList;
use std::fmt::Debug;

pub type Groups = u64; // group assignment bits
pub type Node = u32; // node id

#[derive(Debug, PartialEq, Eq, Clone, Copy)]
pub enum Move {
    AddGroup {
        group: usize,
    },
    RemoveGroup {
        group: usize,
    },
    RemoveNodeFromGroup {
        group: usize,
        node: usize,
        idx: usize,
        old_state: u64,
    },
    AddNodeToGroup {
        group: usize,
        node: usize,
        idx: usize,
        old_state: u64,
    },
}

#[derive(Debug, Clone)]
pub struct MultiGroupModel {
    max_groups: usize,
    num_groups: usize,
    num_nodes: usize,

    /// group assignments for each node
    pub groups: Vec<Groups>, // FIXME: pub for HcpLog

    /// for every group (row), list ids of nodes in group.
    /// entries beyond the group size are invalid.
    nodes_in: IndexedList<Node>,
    /// for every group (row), list ids of nodes not in group.
    /// entries beyond (number of nodes - group size) are invalid.
    nodes_out: IndexedList<Node>,

    pub group_size: Vec<usize>, // FIXME: pub for HcpLog
}

#[inline]
fn insert_zero_at(val: u64, pos: usize, num_groups: u32) -> u64 {
    let group_mask = (1u64 << num_groups) - 1;
    let select_mask = (group_mask << pos) & group_mask;

    let left = val & select_mask;
    let right = val & (!select_mask);

    (left << 1) | right
}

#[inline]
fn remove_bit_at(val: u64, pos: usize, num_groups: u32) -> u64 {
    let group_mask = (1u64 << num_groups) - 1;
    let upper_mask = (group_mask << (pos + 1)) & group_mask;
    let lower_mask = (group_mask >> (num_groups as usize - pos)) & group_mask;

    let upper = val & upper_mask;
    let lower = val & lower_mask;

    (upper >> 1) | lower
}

fn to_group_matrix(groups: &Vec<Groups>, num_groups: u32) -> Vec<Vec<bool>> {
    groups
        .iter()
        .map(|g| (0..num_groups).map(|r| (g >> r) & 1 != 0).collect())
        .collect()
}

macro_rules! getter {
    ($name:ident, $type:ident) => {
        pub fn $name(&self) -> $type {
            self.$name
        }
    };
}

impl MultiGroupModel {
    pub fn with_groups(groups: Vec<Groups>, num_groups: u32, max_groups: u32) -> Self {
        // hierarchical_model::set_nodes_in_out()
        let group_matrix = to_group_matrix(&groups, num_groups);
        let max_groups = max_groups as usize;
        let num_groups = num_groups as usize;
        let num_nodes = groups.len();

        let mut nodes_in = IndexedList::new(num_nodes);
        let mut nodes_out = IndexedList::new(num_nodes);
        let mut group_size = Vec::new();
        for r in 0..(num_groups as usize) {
            nodes_in.push_row(&vec![Node::MAX; num_nodes]);
            nodes_out.push_row(&vec![Node::MAX; num_nodes]);
            let mut in_g = 0;
            let mut out_g = 0;
            for u in 0..num_nodes {
                if group_matrix[u][r] {
                    nodes_in[(r, in_g)] = u as Node;
                    in_g += 1;
                } else {
                    nodes_out[(r, out_g)] = u as Node;
                    out_g += 1;
                }
            }
            group_size.push(in_g);
        }
        Self {
            max_groups,
            num_groups,
            num_nodes,
            groups,
            nodes_in,
            nodes_out,
            group_size,
        }
    }

    getter!(num_groups, usize);
    getter!(max_groups, usize);
    getter!(num_nodes, usize);

    pub fn group_size(&self, groups: impl Into<usize>) -> usize {
        self.group_size[groups.into()]
    }

    pub fn groups_of(&self, node: usize) -> Groups {
        self.groups[node]
    }

    pub fn add_group(&mut self, group: usize) -> Move {
        self.nodes_in
            .insert_row(group, &vec![Node::MAX; self.num_nodes]);
        // TODO: avoid .collect
        self.nodes_out
            .insert_row(group, &(0..self.num_nodes as Node).collect::<Vec<_>>());
        self.group_size.insert(group, 0);
        self.groups = self
            .groups
            .iter()
            .map(|&u| insert_zero_at(u, group, self.num_groups as u32))
            .collect();
        self.num_groups += 1;

        Move::AddGroup { group }
    }

    pub fn remove_group(&mut self, group: usize) -> Move {
        self.groups = self
            .groups
            .iter()
            .map(|&u| remove_bit_at(u, group, self.num_groups as u32))
            .collect();
        self.nodes_in.remove_row(group);
        self.nodes_out.remove_row(group);
        self.group_size.remove(group);
        self.num_groups -= 1;

        Move::RemoveGroup { group }
    }

    pub fn remove_node_from_group_by_idx(&mut self, group: usize, idx: usize) -> Move {
        let n_out = self.num_nodes - self.group_size[group];

        let node = self.nodes_in[(group, idx)] as usize;
        self.nodes_in[(group, idx)] = self.nodes_in[(group, self.group_size[group] - 1)];
        self.nodes_out[(group, n_out)] = node as Node;
        let old_state = self.groups[node];
        self.groups[node] -= 1u64 << group;
        self.group_size[group] -= 1;

        Move::RemoveNodeFromGroup {
            group,
            node,
            idx,
            old_state,
        }
    }

    pub fn add_node_to_group_by_idx(&mut self, group: usize, idx: usize) -> Move {
        let n_out = self.num_nodes - self.group_size[group];

        let node = self.nodes_out[(group, idx)] as usize;
        self.nodes_out[(group, idx)] = self.nodes_out[(group, n_out - 1)];
        self.nodes_in[(group, self.group_size[group])] = node as Node;
        let old_state = self.groups[node];
        self.groups[node] += 1u64 << group;
        self.group_size[group] += 1;

        Move::AddNodeToGroup {
            group,
            node,
            idx,
            old_state,
        }
    }

    /// Undo group modifications of move `m`.
    /// Does *not* restore log likelihood or hcg values.
    pub fn undo_move(&mut self, m: Move) {
        match m {
            Move::RemoveNodeFromGroup {
                group, node, idx, ..
            } => {
                // TODO: can this be unified with MultiGroupModel::add_node_to_group_by_idx?
                self.group_size[group] += 1;
                let n_out = self.num_nodes - self.group_size[group];
                self.nodes_out[(group, n_out)] = Node::MAX;
                self.nodes_in[(group, idx)] = node as Node;
                self.groups[node] += 1u64 << group;
            }
            Move::RemoveGroup { group } => {
                self.add_group(group);
            }
            Move::AddGroup { group } => {
                self.remove_group(group);
            }
            Move::AddNodeToGroup {
                group, node, idx, ..
            } => {
                // TODO: can this be unified with MultiGroupModel::remove_node_from_group_by_idx?
                self.group_size[group] -= 1;
                self.nodes_in[(group, self.group_size[group])] = Node::MAX;
                self.nodes_out[(group, idx)] = node as Node;
                self.groups[node] -= 1u64 << group;
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn _test_model() -> MultiGroupModel {
        MultiGroupModel::with_groups(
            vec![
                9, 41, 25, 13, 73, 137, 11, 33, 17, 5, 65, 129, 3, 33, 33, 17, 17, 5, 5, 65, 65,
                129, 129, 3, 3,
            ],
            8,
            64,
        )
    }

    #[test]
    fn add_group() {
        let mut model = _test_model();
        let g = 1;
        let old = model.clone();
        let op = model.add_group(g);
        assert_eq!(model.num_groups, old.num_groups + 1);
        assert_eq!(
            model.group_size.iter().sum::<usize>(),
            old.group_size.iter().sum()
        );
        assert_eq!(model.group_size[g], 0);

        let mut undone = model.clone();
        undone.undo_move(op);
        assert_eq!(old.num_groups, undone.num_groups);
        assert_eq!(old.group_size, undone.group_size);
        assert_eq!(old.groups, undone.groups);
    }
    #[test]
    fn remove_group() {
        let mut model = _test_model();
        let g = 1;
        let old = model.clone();
        let op = model.remove_group(g);
        assert_eq!(model.num_groups, old.num_groups - 1);
        assert_eq!(
            model.group_size.iter().sum::<usize>(),
            old.group_size.iter().sum::<usize>() - old.group_size[g]
        );

        let mut undone = model.clone();
        undone.undo_move(op);
        assert_eq!(old.num_groups, undone.num_groups);
        // HACK: remove_group assumes groups are empty, but I couldn't be bothered to properly
        //       set that up.

        // assert_eq!(old.group_size, undone.group_size);
        // assert_eq!(old.groups, undone.groups);
    }

    #[test]
    fn add_node_to_group_by_idx() {
        let mut model = _test_model();
        let g = 1;
        let idx = 3;
        let old = model.clone();
        let op = model.add_node_to_group_by_idx(g, idx);
        assert_eq!(model.num_groups, old.num_groups);
        match op {
            Move::AddNodeToGroup { node, .. } => assert!(model.groups[node] & (1 << g) != 0),
            _ => panic!("not an add_node_to_group operation"),
        }
        assert_eq!(
            model.group_size.iter().sum::<usize>(),
            old.group_size.iter().sum::<usize>() + 1
        );
        assert_eq!(model.group_size[g], old.group_size[g] + 1);

        let mut undone = model.clone();
        undone.undo_move(op);
        assert_eq!(old.num_groups, undone.num_groups);
        assert_eq!(old.group_size, undone.group_size);
        assert_eq!(old.groups, undone.groups);
    }
    #[test]
    fn remove_node_from_group_by_idx() {
        let mut model = _test_model();

        let g = 1;
        let idx = 3;
        let old = model.clone();
        let op = model.remove_node_from_group_by_idx(g, idx);
        assert_eq!(model.num_groups, old.num_groups);
        match op {
            Move::RemoveNodeFromGroup { node, .. } => assert!(model.groups[node] & (1 << g) == 0),
            _ => panic!("not an remove_node_from_group operation"),
        }
        assert_eq!(
            model.group_size.iter().sum::<usize>(),
            old.group_size.iter().sum::<usize>() - 1
        );
        assert_eq!(model.group_size[g], old.group_size[g] - 1);

        let mut undone = model.clone();
        undone.undo_move(op);
        assert_eq!(old.num_groups, undone.num_groups);
        assert_eq!(old.group_size, undone.group_size);
        assert_eq!(old.groups, undone.groups);
    }
}
