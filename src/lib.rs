use gml_parser::{Edge, GMLObject, Graph};
use indexed_list::IndexedList;
use parameters::Parameters;
use std::error::Error;
use std::fs;
use std::iter;
use std::path::Path;

#[cfg(feature = "gsl_compat")]
mod gsl_rng_compat;
#[cfg(feature = "gsl_compat")]
use gsl_rng_compat::MT19937;

#[cfg(not(feature = "gsl_compat"))]
use mt19937::MT19937;
#[cfg(not(feature = "gsl_compat"))]
use rand::{Rng, SeedableRng};

mod indexed_list;
mod math;
pub mod parameters;

type Groups = u64; // group assignment bits

enum Move {
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

pub struct HierarchicalModel {
    rng: MT19937,

    network: Graph,
    max_groups: u32,

    pub num_groups: u32,
    pub groups: Vec<Groups>, // group assignments for each node

    /// for every group (row), list ids of nodes in group.
    /// entries beyond the group size are invalid.
    nodes_in: IndexedList<i32>,
    /// for every group (row), list ids of nodes not in group.
    /// entries beyond (number of nodes - group size) are invalid.
    nodes_out: IndexedList<i32>,

    pub group_size: Vec<usize>,
    pub hcg_edges: Vec<usize>, // number of edges in each group
    pub hcg_pairs: Vec<usize>, // number of possible edges in each group
    pub log_like: f64,         // current log-likelihood
}

fn _read_network(gml_path: &Path) -> Result<Graph, Box<dyn Error>> {
    Ok(Graph::from_gml(GMLObject::from_str(&fs::read_to_string(
        gml_path,
    )?)?)?)
}

fn to_group_matrix(groups: &Vec<Groups>, num_groups: u32) -> Vec<Vec<bool>> {
    groups
        .iter()
        .map(|g| (0..num_groups).map(|r| (g >> r) & 1 != 0).collect())
        .collect()
}

impl HierarchicalModel {
    fn new(network: Graph, max_groups: u32) -> Self {
        // initialize a core-periphery structure with two groups, all nodes in group 0 only.
        assert!(max_groups <= 64);
        math::precompute_ln_fact(network.nodes.len().pow(2) + 1);
        Self {
            max_groups,
            num_groups: 2,
            groups: vec![1; network.nodes.len()],
            group_size: vec![network.nodes.len(), 0],
            hcg_edges: vec![network.edges.len(), 0],
            hcg_pairs: vec![network.edges.len(), 0],
            log_like: f64::NAN,
            rng: MT19937::seed_from_u64(0),
            nodes_in: IndexedList::new(network.nodes.len()),
            nodes_out: IndexedList::new(network.nodes.len()),
            network,
        }
    }

    pub fn with_parameters(params: &Parameters) -> Result<Self, String> {
        if params.max_num_groups > 64 {
            return Err(String::from("number of groups cannot exceed 64"));
        }
        let mut this = Self::new(
            _read_network(&params.gml_path).map_err(|e| e.to_string())?,
            params.max_num_groups,
        );
        this.rng = MT19937::seed_from_u64(params.seed.unwrap_or(0));
        if let Some(groups) = &params.initial_group_config {
            println!("assigning random groups to nodes");
            this.init_groups(groups.clone(), params.initial_num_groups);
        } else {
            let max = 1u64 << (params.initial_num_groups - 1);
            let groups = (0..this.network.nodes.len())
                .map(|_| (this.rng.gen_range(0..max) << 1) + 1)
                .collect();
            println!("assigning user specified groups to nodes");
            this.init_groups(groups, params.initial_num_groups);
        }
        Ok(this)
    }

    /// Highest Common Group
    fn hcg(&self, u: i32, v: i32) -> usize {
        let group_mask = (1u64 << self.num_groups) - 1;
        let masked_u = self.groups[u as usize] & group_mask;
        let masked_v = self.groups[v as usize] & group_mask;

        let common_bits = masked_u & masked_v;
        let common_bits = common_bits | (common_bits >> 1u64);
        let common_bits = common_bits | (common_bits >> 2u64);
        let common_bits = common_bits | (common_bits >> 4u64);
        let common_bits = common_bits | (common_bits >> 8u64);
        let common_bits = common_bits | (common_bits >> 16u64);
        let common_bits = common_bits | (common_bits >> 32u64);

        (63u64 - ((common_bits - (common_bits >> 1u64)).leading_zeros() as u64)) as usize
    }

    ///
    fn hcg_node(&self, old_state: u64, u: i32) -> usize {
        let group_mask = (1u64 << self.num_groups) - 1;
        let masked_u = old_state & group_mask;
        let masked_v = self.groups[u as usize] & group_mask;

        let common_bits = masked_u & masked_v;
        let common_bits = common_bits | (common_bits >> 1u64);
        let common_bits = common_bits | (common_bits >> 2u64);
        let common_bits = common_bits | (common_bits >> 4u64);
        let common_bits = common_bits | (common_bits >> 8u64);
        let common_bits = common_bits | (common_bits >> 16u64);
        let common_bits = common_bits | (common_bits >> 32u64);

        (63u64 - (common_bits - (common_bits >> 1u64)).leading_zeros() as u64) as usize
    }

    fn init_groups(&mut self, groups: Vec<Groups>, num_groups: u32) {
        self.groups = groups;
        self.num_groups = num_groups;
        self.group_size.clear();
        self.hcg_edges.clear();
        self.hcg_pairs.clear();
        self.nodes_in.clear();
        self.nodes_out.clear();

        // hierarchical_model::set_nodes_in_out()
        let group_matrix = to_group_matrix(&self.groups, self.num_groups);

        for r in 0..(self.num_groups as usize) {
            self.nodes_in.push_row(&vec![-1; self.network.nodes.len()]);
            self.nodes_out.push_row(&vec![-1; self.network.nodes.len()]);
            let mut in_g = 0;
            let mut out_g = 0;
            for u in 0..self.network.nodes.len() {
                if group_matrix[u][r] {
                    self.nodes_in[(r, in_g)] = u as i32;
                    in_g += 1;
                } else {
                    self.nodes_out[(r, out_g)] = u as i32;
                    out_g += 1;
                }
            }
            self.group_size.push(in_g);
        }
        // void hierarchical_model::set_hcg_edges()
        // FIXME: node ids might not correspond to positions
        self.hcg_edges = vec![0; self.num_groups as usize];
        for &Edge { source, target, .. } in self.network.edges.iter() {
            let hcg = self.hcg(source as i32, target as i32);
            self.hcg_edges[hcg] += 1;
        }

        // void hierarchical_model::set_hcg_pairs()
        // FIXME: node ids might not correspond to positions
        self.hcg_pairs = vec![0; self.num_groups as usize];
        for source in self.network.nodes.iter() {
            for target in self.network.nodes.iter() {
                if source.id < target.id {
                    let hcg = self.hcg(source.id as i32, target.id as i32);
                    self.hcg_pairs[hcg] += 1;
                }
            }
        }

        self.log_like = self.calc_loglike();
    }

    fn add_group(&mut self, group: usize) -> Move {
        let num_nodes = self.network.nodes.len();
        self.nodes_in.insert_row(group, &vec![-1; num_nodes]);
        // TODO: avoid .collect
        self.nodes_out
            .insert_row(group, &(0..num_nodes as i32).collect::<Vec<_>>());
        self.group_size.insert(group, 0);
        self.hcg_edges.insert(group, 0);
        self.hcg_pairs.insert(group, 0);
        self.groups = self
            .groups
            .iter()
            .map(|&u| insert_zero_at(u, group, self.num_groups))
            .collect();
        self.num_groups += 1;

        Move::AddGroup { group }
    }

    fn remove_group(&mut self, group: usize) -> Move {
        self.groups = self
            .groups
            .iter()
            .map(|&u| remove_bit_at(u, group, self.num_groups))
            .collect();
        self.nodes_in.remove_row(group);
        self.nodes_out.remove_row(group);
        self.hcg_edges.remove(group);
        self.hcg_pairs.remove(group);
        self.group_size.remove(group);
        self.num_groups -= 1;

        Move::RemoveGroup { group }
    }

    fn remove_node_from_group_by_idx(&mut self, group: usize, idx: usize) -> Move {
        let n_out = self.network.nodes.len() - self.group_size[group];

        let node = self.nodes_in[(group, idx)] as usize;
        self.nodes_in[(group, idx)] = self.nodes_in[(group, self.group_size[group] - 1)];
        self.nodes_out[(group, n_out)] = node as i32;
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

    fn add_node_to_group_by_idx(&mut self, group: usize, idx: usize) -> Move {
        let n_out = self.network.nodes.len() - self.group_size[group];

        let node = self.nodes_out[(group, idx)] as usize;
        self.nodes_out[(group, idx)] = self.nodes_out[(group, n_out - 1)];
        self.nodes_in[(group, self.group_size[group])] = node as i32;
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

    fn uniform_groupsize(&mut self) -> Option<Move> {
        let num_nodes = self.network.nodes.len();
        let p_type2 = 1f64 / (2 * self.num_groups as usize * (num_nodes + 1)) as f64;
        if self.rng.gen_bool(p_type2) {
            // adds empty group or does nothing if number of groups is equal to maximum number of groups
            if self.num_groups == self.max_groups {
                return None;
            }
            // add empty group
            let rand_group = self.rng.gen_range(1..=self.num_groups as usize);
            return Some(self.add_group(rand_group));
        } else {
            if self.num_groups == 1 {
                // if only the group of all nodes is left, do nothing
                return None;
            }
            let rand_group = self.rng.gen_range(1..self.num_groups as usize);
            if self.rng.gen_bool(0.5) {
                // remove a node
                if self.group_size[rand_group] == 0 {
                    // if empty, remove group entirely
                    return Some(self.remove_group(rand_group));
                }
                let rand_idx = self.rng.gen_range(0..self.group_size[rand_group]);
                return Some(self.remove_node_from_group_by_idx(rand_group, rand_idx));
            } else {
                // add a node
                if self.group_size[rand_group] == num_nodes {
                    // if group is already full, do nothing
                    return None;
                }
                let n_out = self.network.nodes.len() - self.group_size[rand_group];
                let rand_idx = self.rng.gen_range(0..n_out);
                return Some(self.add_node_to_group_by_idx(rand_group, rand_idx));
            }
        }
    }

    fn update_hcg_props(&mut self, u: i32, old_state: u64) {
        for v in 0..self.network.nodes.len() as i32 {
            if v == u {
                continue;
            }
            let new = self.hcg(u, v);
            let old = self.hcg_node(old_state, v);
            self.hcg_pairs[old] -= 1;
            self.hcg_pairs[new] += 1;
        }
        for &Edge { source, target, .. } in self.network.edges.iter() {
            // TODO: use different graph lib with more efficient neighbour list
            if !((source == u as i64) ^ (target == u as i64)) {
                continue;
            }
            let v = if source == u as i64 { target } else { source } as i32;
            let new = self.hcg(u, v);
            let old = self.hcg_node(old_state, v);
            self.hcg_edges[old] -= 1;
            self.hcg_edges[new] += 1;
        }
    }

    fn calc_loglike(&self) -> f64 {
        iter::zip(&self.hcg_edges, &self.hcg_pairs)
            .map(|(&e, &p)| math::ln_fact(e) + math::ln_fact(p - e) - math::ln_fact(p + 1))
            .sum()
    }

    pub fn get_groups(&mut self) {
        let old_hcg_edges = self.hcg_edges.clone();
        let old_hcg_pairs = self.hcg_pairs.clone();

        let Some(m) = self.uniform_groupsize() else {
            return;
        };

        let new_loglike = match m {
            // adding/removing empty groups does not affect log likelihood
            Move::AddGroup { .. } | Move::RemoveGroup { .. } => self.log_like,
            Move::AddNodeToGroup {
                node, old_state, ..
            }
            | Move::RemoveNodeFromGroup {
                node, old_state, ..
            } => {
                self.update_hcg_props(node as i32, old_state);
                self.calc_loglike()
            }
        };

        let alpha = f64::exp(new_loglike - self.log_like); // acceptance probability
        if alpha >= 1.0 || self.rng.gen_bool(alpha) {
            // accept move
            self.log_like = new_loglike
        } else {
            // revert move
            let num_nodes = self.network.nodes.len();
            match m {
                Move::RemoveNodeFromGroup {
                    group, node, idx, ..
                } => {
                    // TODO: can this be unified with HierarchicalModel::add_node_to_group_by_idx?
                    self.group_size[group] += 1;
                    let n_out = num_nodes - self.group_size[group];
                    self.nodes_out[(group, n_out)] = -1;
                    self.nodes_in[(group, idx)] = node as i32;
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
                    // TODO: can this be unified with HierarchicalModel::remove_node_from_group_by_idx?
                    self.group_size[group] -= 1;
                    self.nodes_in[(group, self.group_size[group])] = -1;
                    self.nodes_out[(group, idx)] = node as i32;
                    self.groups[node] -= 1u64 << group;
                }
            }
            self.hcg_edges = old_hcg_edges[..self.num_groups as usize].to_owned();
            self.hcg_pairs = old_hcg_pairs[..self.num_groups as usize].to_owned();
        }
    }
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

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs::File;
    use std::io::Read;
    use std::path::Path;

    #[test]
    fn example() {
        let hcp = HierarchicalModel::with_parameters(
            &Parameters::load(File::open("examples/parameters.txt").unwrap().chain(
                &b"initial_group_config: 9 41 25 13 73 137 11 33 17 5 65 129 3 33 33 17 17 5 5 65 65 129 129 3 3\n"[..]
            ).chain(&b"initial_num_groups: 8\n"[..])
            )
            .unwrap()
            .resolve_paths(Path::new("examples/")),
        )
        .unwrap();
        assert_eq!(
            hcp.groups,
            [
                9, 41, 25, 13, 73, 137, 11, 33, 17, 5, 65, 129, 3, 33, 33, 17, 17, 5, 5, 65, 65,
                129, 129, 3, 3
            ]
        );
        assert_eq!(hcp.num_groups, 8);
        assert_eq!(hcp.group_size, [25, 4, 4, 7, 4, 4, 4, 4]);
        assert_eq!(hcp.hcg_edges, [0, 6, 6, 21, 6, 6, 6, 6]);
        assert_eq!(hcp.hcg_pairs, [243, 6, 6, 21, 6, 6, 6, 6]);
        assert!(
            (hcp.log_like - -20.2637).abs() < 0.001,
            "{} != {}",
            hcp.log_like,
            -20.2637
        );
    }
}
