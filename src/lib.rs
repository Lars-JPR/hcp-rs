use gml_parser::{Edge, GMLObject, Graph};
use indexed_list::IndexedList;
use mt19937::MT19937;
use parameters::Parameters;
use rand::{Rng, SeedableRng};
use std::error::Error;
use std::fs;
use std::path::Path;
use std::time;

mod indexed_list;
pub mod parameters;

type Groups = u64; // group assignment bits

pub struct HierarchicalModel {
    rng: MT19937,

    network: Graph,
    max_groups: u32,

    num_groups: u32,
    groups: Vec<Groups>, // group assignments for each node

    nodes_in: IndexedList<i32>,  // ??
    nodes_out: IndexedList<i32>, // ??

    group_size: Vec<usize>,
    hcg_edges: Vec<usize>, // number of edges in each group
    hcg_pairs: Vec<usize>, // number of possible edges in each group
    log_like: f64,         // current log-likelihood
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
        Self {
            max_groups,
            num_groups: 2,
            groups: vec![1; network.nodes.len()],
            group_size: vec![network.nodes.len(), 0],
            hcg_edges: vec![network.edges.len(), 0],
            hcg_pairs: vec![network.edges.len(), 0],
            log_like: f64::NAN,
            rng: MT19937::seed_from_u64(time::UNIX_EPOCH.elapsed().unwrap().as_secs()),
            network,
            //todo: not sure what nodes_in and nodes_out do..
            nodes_in: IndexedList::new(),
            nodes_out: IndexedList::new(),
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
    fn hcg(&self, u: i64, v: i64) -> usize {
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

    fn init_groups(&mut self, groups: Vec<Groups>, num_groups: u32) {
        self.groups = groups;
        self.num_groups = num_groups;

        // hierarchical_model::set_nodes_in_out()
        let group_matrix = to_group_matrix(&self.groups, self.num_groups);

        for r in 0..(self.num_groups as usize) {
            self.nodes_in.push_row(&vec![-1; self.network.nodes.len()]);
            self.nodes_out.push_row(&vec![-1; self.network.nodes.len()]);
            let mut in_g = 0;
            let mut out_g = 0;
            for u in 0..self.network.nodes.len() {
                if group_matrix[u][r] {
                    self.nodes_in[(in_g, r)] = u as i32;
                    in_g += 1;
                } else {
                    self.nodes_out[(out_g, r)] = u as i32;
                    out_g += 1;
                }
            }
            self.group_size.push(in_g);
        }
        // void hierarchical_model::set_hcg_edges()
        // FIXME: node ids might not correspond to positions
        self.hcg_edges = vec![0; self.num_groups as usize];
        for &Edge { source, target, .. } in self.network.edges.iter() {
            if source < target {
                let hcg = self.hcg(source, target);
                self.hcg_edges[hcg] += 1;
            }
        }

        // void hierarchical_model::set_hcg_pairs()
        // FIXME: node ids might not correspond to positions
        self.hcg_pairs = vec![0; self.num_groups as usize];
        for source in self.network.nodes.iter() {
            for target in self.network.nodes.iter() {
                let hcg = self.hcg(source.id, target.id);
                self.hcg_pairs[hcg] += 1;
            }
        }
    }
}
