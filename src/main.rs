use hcp_rs::parameters::Parameters;
use hcp_rs::HierarchicalModel;
use std::env;
use std::fmt::Display;
use std::fs;
use std::fs::File;
use std::io;
use std::io::{BufWriter, Write};
use std::path::{Path, PathBuf};
use std::time;

#[derive(Debug, Default)]
struct HcpLog {
    groups: Vec<Vec<u64>>, // called `intermediate_states` and `configs` in cpp version
    num_groups: Vec<usize>,
    hcg_edges: Vec<Vec<usize>>,
    hcg_pairs: Vec<Vec<usize>>,
    group_size: Vec<Vec<usize>>,
    log_like: Vec<f64>, // called energies in cpp version
}

impl HcpLog {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn shapshot(&mut self, hcp: &HierarchicalModel) {
        self.groups.push(hcp.model.groups.clone());
        self.hcg_edges.push(hcp.hcg_edges.clone());
        self.hcg_pairs.push(hcp.hcg_pairs.clone());
        self.group_size.push(hcp.model.group_size.clone());
        self.log_like.push(hcp.log_like.clone());
        self.num_groups.push(hcp.model.num_groups().clone());
    }

    fn dump_vec_space_separated<T: Display, W: Write>(w: &mut W, v: &Vec<T>) -> io::Result<()> {
        if let Some((last, rest)) = v.split_last() {
            for x in rest {
                write!(w, "{} ", x)?;
            }
            write!(w, "{}", last)?;
        }
        Ok(())
    }

    pub fn dump(&self, save_dir: &Path, name: &str) -> io::Result<()> {
        if !save_dir.exists() {
            fs::create_dir_all(save_dir)?;
        }

        macro_rules! dv {
            ($data:expr, $suff:expr) => {{
                let path = save_dir.join(format!("{}_{}.txt", name, $suff));
                let mut w = BufWriter::new(File::create(path)?);
                for row in $data {
                    HcpLog::dump_vec_space_separated(&mut w, row)?;
                    writeln!(w)?;
                }
                w.flush()?;
            }};
        }

        macro_rules! d {
            ($data:expr, $suff:expr) => {{
                let path = save_dir.join(format!("{}_{}.txt", name, $suff));
                let mut w = BufWriter::new(File::create(path)?);
                for x in $data {
                    writeln!(w, "{}", x)?;
                }
                w.flush()?;
            }};
        }

        dv!(&self.groups, "configs");
        d!(&self.num_groups, "num_groups");
        dv!(&self.group_size, "group_size");
        dv!(&self.hcg_edges, "edges");
        dv!(&self.hcg_pairs, "pairs");
        d!(&self.log_like, "ll");
        Ok(())
    }
}

fn main() -> Result<(), String> {
    let parameters_file = PathBuf::from(
        env::args()
            .nth(1)
            .ok_or(String::from("missing parameters file"))?,
    );
    let parameters = Parameters::load(File::open(&parameters_file).map_err(|e| e.to_string())?)?
        .resolve_paths(&parameters_file.parent().unwrap_or(Path::new(".")))
        .fix_seed();
    println!("{:?}", parameters);
    let mut hcp = HierarchicalModel::with_parameters(&parameters).map_err(|e| e.to_string())?;
    let mut log = HcpLog::new();

    println!("seed: {}", parameters.seed.unwrap_or(0));
    println!("number of pairs: {:?}", hcp.hcg_pairs);
    println!("number of edges: {:?}", hcp.hcg_edges);
    for i in 0..parameters.max_itr {
        hcp.get_groups();
        if i % 10000000 == 0 {
            println!("-----------------------------------------------------");
            println!(
                "time: {}",
                time::SystemTime::now()
                    .duration_since(time::UNIX_EPOCH)
                    .map_or("???".to_string(), |d| d.as_secs().to_string())
            );
            println!("iteration: {} energy: {:.4}", i, hcp.log_like);
            println!("number of pairs: {:?}", hcp.hcg_pairs);
            println!("number of edges: {:?}", hcp.hcg_edges);
            println!("group sizes: {:?}", hcp.model.group_size);
        }

        if (i > 10000000) && (i % 1500 == 0) {
            log.shapshot(&hcp);
        }
    }
    println!("Writing data to file.");
    log.dump(&parameters.save_directory, &parameters.saved_data_name)
        .map_err(|e| e.to_string())?;
    Ok(())
}
