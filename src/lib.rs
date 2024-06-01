use gml_parser::{GMLObject, Graph};
use indexed_list::IndexedList;
use mt19937::MT19937;
use parameters::Parameters;
use rand_core::SeedableRng;
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
        // todo: init groups
        Ok(this)
    }
}
