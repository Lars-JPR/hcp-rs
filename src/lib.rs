use gml_parser::{Edge, GMLObject, Graph};
use parameters::Parameters;
use std::error::Error;
use std::fs;
use std::iter;
use std::path::Path;

#[cfg(feature = "gsl_compat")]
mod gsl_rng_compat;
#[cfg(feature = "gsl_compat")]
use gsl_rng_compat::MT19937;
use multi_group_model::{Move, MultiGroupModel};

#[cfg(not(feature = "gsl_compat"))]
use mt19937::MT19937;
#[cfg(not(feature = "gsl_compat"))]
use rand::{Rng, SeedableRng};

mod indexed_list;
mod math;
mod multi_group_model;
pub mod parameters;

trait HCG {
    /// Highest Common Group
    fn hcg(&self, u: i32, v: i32) -> usize;

    fn hcg_node(&self, old_state: u64, u: i32) -> usize;
}

#[derive(Clone)]
pub struct HierarchicalModel {
    rng: MT19937,

    pub network: Graph,
    pub model: MultiGroupModel,
    pub hcg_edges: Vec<usize>, // number of edges in each group
    pub hcg_pairs: Vec<usize>, // number of possible edges in each group
    pub log_like: f64,         // current log-likelihood
}

fn _read_network(gml_path: &Path) -> Result<Graph, Box<dyn Error>> {
    Ok(Graph::from_gml(GMLObject::from_str(&fs::read_to_string(
        gml_path,
    )?)?)?)
}

fn calc_loglike(a: &Vec<usize>, b: &Vec<usize>) -> f64 {
    iter::zip(a, b)
        .map(|(&e, &p)| math::ln_fact(e) + math::ln_fact(p - e) - math::ln_fact(p + 1))
        .sum()
}

impl HCG for MultiGroupModel {
    fn hcg(&self, u: i32, v: i32) -> usize {
        let group_mask = (1u64 << self.num_groups()) - 1;
        let masked_u = self.groups_of(u as usize) & group_mask;
        let masked_v = self.groups_of(v as usize) & group_mask;

        let common_bits = masked_u & masked_v;
        let common_bits = common_bits | (common_bits >> 1u64);
        let common_bits = common_bits | (common_bits >> 2u64);
        let common_bits = common_bits | (common_bits >> 4u64);
        let common_bits = common_bits | (common_bits >> 8u64);
        let common_bits = common_bits | (common_bits >> 16u64);
        let common_bits = common_bits | (common_bits >> 32u64);

        (63u64 - ((common_bits - (common_bits >> 1u64)).leading_zeros() as u64)) as usize
    }

    fn hcg_node(&self, old_state: u64, u: i32) -> usize {
        let group_mask = (1u64 << self.num_groups()) - 1;
        let masked_u = old_state & group_mask;
        let masked_v = self.groups_of(u as usize) & group_mask;

        let common_bits = masked_u & masked_v;
        let common_bits = common_bits | (common_bits >> 1u64);
        let common_bits = common_bits | (common_bits >> 2u64);
        let common_bits = common_bits | (common_bits >> 4u64);
        let common_bits = common_bits | (common_bits >> 8u64);
        let common_bits = common_bits | (common_bits >> 16u64);
        let common_bits = common_bits | (common_bits >> 32u64);

        (63u64 - (common_bits - (common_bits >> 1u64)).leading_zeros() as u64) as usize
    }
}

impl HierarchicalModel {
    pub fn with_parameters(params: &Parameters) -> Result<Self, String> {
        if params.max_num_groups > 64 {
            return Err(String::from("number of groups cannot exceed 64"));
        }
        let network = _read_network(&params.gml_path).map_err(|e| e.to_string())?;
        math::precompute_ln_fact(&network.nodes.len().pow(2) + 1);
        let mut rng = MT19937::seed_from_u64(params.seed.unwrap_or(0));
        let groups = match &params.initial_group_config {
            Some(groups) => {
                println!("assigning user specified groups to nodes");
                groups.clone()
            }
            _ => {
                println!("assigning random groups to nodes");
                let max = 1u64 << (params.initial_num_groups - 1);
                (0..network.nodes.len())
                    .map(|_| (rng.gen_range(0..max) << 1) + 1)
                    .collect()
            }
        };
        let model =
            MultiGroupModel::with_groups(groups, params.initial_num_groups, params.max_num_groups);

        let (hcg_edges, hcg_pairs) = HierarchicalModel::init_hcg_props(&network, &model);
        let log_like = calc_loglike(&hcg_edges, &hcg_pairs);

        Ok(Self {
            network,
            model,
            hcg_edges,
            hcg_pairs,
            log_like,
            rng,
        })
    }

    /// initialize group edge count caches hcp_edges, hcp_pairs
    fn init_hcg_props(network: &Graph, model: &MultiGroupModel) -> (Vec<usize>, Vec<usize>) {
        // void hierarchical_model::set_hcg_edges()
        // FIXME: node ids might not correspond to positions
        let mut hcg_edges = vec![0; model.num_groups()];
        for &Edge { source, target, .. } in network.edges.iter() {
            let hcg = model.hcg(source as i32, target as i32);
            hcg_edges[hcg] += 1;
        }

        // void hierarchical_model::set_hcg_pairs()
        // FIXME: node ids might not correspond to positions
        let mut hcg_pairs = vec![0; model.num_groups()];
        for source in network.nodes.iter() {
            for target in network.nodes.iter() {
                if source.id < target.id {
                    let hcg = model.hcg(source.id as i32, target.id as i32);
                    hcg_pairs[hcg] += 1;
                }
            }
        }
        (hcg_edges, hcg_pairs)
    }

    fn uniform_groupsize(&mut self) -> Option<Move> {
        let num_nodes = self.model.num_nodes();
        let num_groups = self.model.num_groups();
        let max_groups = self.model.max_groups();
        let p_type2 = 1f64 / (2 * num_groups * (num_nodes + 1)) as f64;
        if self.rng.gen_bool(p_type2) {
            // adds empty group or does nothing if number of groups is equal to maximum number of groups
            if num_groups == max_groups {
                return None;
            }
            // add empty group
            let rand_group = self.rng.gen_range(1..=num_groups);
            return Some(self.model.add_group(rand_group));
        } else {
            if num_groups == 1 {
                // if only the group of all nodes is left, do nothing
                return None;
            }
            let rand_group = self.rng.gen_range(1..num_groups);
            if self.rng.gen_bool(0.5) {
                // remove a node
                if self.model.group_size(rand_group) == 0 {
                    // if empty, remove group entirely
                    return Some(self.model.remove_group(rand_group));
                }
                let rand_idx = self.rng.gen_range(0..self.model.group_size(rand_group));
                return Some(
                    self.model
                        .remove_node_from_group_by_idx(rand_group, rand_idx),
                );
            } else {
                // add a node
                if self.model.group_size(rand_group) == num_nodes {
                    // if group is already full, do nothing
                    return None;
                }
                let n_out: usize = self.model.num_nodes() - self.model.group_size(rand_group);
                let rand_idx = self.rng.gen_range(0..n_out);
                return Some(self.model.add_node_to_group_by_idx(rand_group, rand_idx));
            }
        }
    }

    fn update_hcg_props(&mut self, m: Move) {
        match m {
            Move::AddGroup { group, .. } => {
                self.hcg_edges.insert(group, 0);
                self.hcg_pairs.insert(group, 0);
            }
            Move::RemoveGroup { group, .. } => {
                self.hcg_edges.remove(group);
                self.hcg_pairs.remove(group);
            }
            Move::AddNodeToGroup {
                node, old_state, ..
            }
            | Move::RemoveNodeFromGroup {
                node, old_state, ..
            } => {
                let u = node as i32;
                for v in 0..self.network.nodes.len() as i32 {
                    if v == u {
                        continue;
                    }
                    let new = HCG::hcg(&self.model, u, v);
                    let old = HCG::hcg_node(&self.model, old_state, v);
                    self.hcg_pairs[old] -= 1;
                    self.hcg_pairs[new] += 1;
                }
                for &Edge { source, target, .. } in self.network.edges.iter() {
                    // TODO: use different graph lib with more efficient neighbour list
                    if !((source == u as i64) ^ (target == u as i64)) {
                        continue;
                    }
                    let v = if source == u as i64 { target } else { source } as i32;
                    let new = HCG::hcg(&self.model, u, v);
                    let old = HCG::hcg_node(&self.model, old_state, v);
                    self.hcg_edges[old] -= 1;
                    self.hcg_edges[new] += 1;
                }
            }
        }
    }

    pub fn get_groups(&mut self) {
        let old_hcg_edges = self.hcg_edges.clone();
        let old_hcg_pairs = self.hcg_pairs.clone();

        let Some(m) = self.uniform_groupsize() else {
            return;
        };

        self.update_hcg_props(m);

        let new_loglike = if let Move::RemoveNodeFromGroup { .. } | Move::AddNodeToGroup { .. } = m
        {
            calc_loglike(&self.hcg_edges, &self.hcg_pairs)
        } else {
            self.log_like
        };

        let alpha = f64::exp(new_loglike - self.log_like); // acceptance probability
        if self.rng.gen_bool(alpha) {
            // accept move
            self.log_like = new_loglike
        } else {
            self.model.undo_move(m);
            self.hcg_edges = old_hcg_edges[..self.model.num_groups()].to_owned();
            self.hcg_pairs = old_hcg_pairs[..self.model.num_groups()].to_owned();
        }
    }
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
