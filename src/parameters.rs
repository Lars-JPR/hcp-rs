use std::collections::HashMap;
use std::env;
use std::io::Read;
use std::io::{BufRead, BufReader};
use std::path::{Path, PathBuf};
use std::str::FromStr;
use std::time;

#[derive(Debug)]
pub struct Parameters {
    pub gml_path: PathBuf,                      // path to gml file
    pub max_itr: u64,                           // maximum number of monte carlo steps
    pub seed: Option<u64>,                      // random number generator seed
    pub max_num_groups: u32,                    // maximum number of groups
    pub initial_num_groups: u32,                // number of groups to initialize simulation with
    pub initial_group_config: Option<Vec<u64>>, // group configuration to initialize simulation with
    pub saved_data_name: String,                // name to prepend saved data files with
    pub save_directory: PathBuf,                // location where data will be saved to
}

fn _get_int<T: FromStr>(m: &HashMap<String, String>, key: &str, default: T) -> Result<T, String> {
    m.get(key).map_or(Ok(default), |s| {
        s.parse().or(Err(format!("not an integer: {}", s)))
    })
}

fn _get_ints<T: FromStr>(m: &HashMap<String, String>, key: &str) -> Result<Option<Vec<T>>, String> {
    m.get(key).map_or(Ok(None), |s| {
        s.split_whitespace()
            .map(|w| w.parse().or(Err(format!("not an integer: {}", s))))
            .collect::<Result<Vec<T>, String>>()
            .map(|v| Some(v))
    })
}

impl Parameters {
    pub fn load(src: impl Read) -> Result<Self, String> {
        let map = BufReader::new(src)
            .lines()
            .map(|l| {
                l.expect("I/O error")
                    .split_once(":")
                    .ok_or(String::from("Malformed parameters file: missing ':'"))
                    .map(|(k, v)| (k.trim().to_lowercase(), v.trim().to_owned()))
            })
            .collect::<Result<HashMap<_, _>, String>>()?;
        Ok(Self {
            gml_path: PathBuf::from(
                map.get("gml_path")
                    .ok_or("Missing required parameter 'gml_path'")?,
            ),
            max_itr: _get_int(&map, "max_itr", 1000000000)?,
            max_num_groups: _get_int(&map, "max_num_groups", 64)?,
            initial_num_groups: _get_int(&map, "initial_num_groups", 2)?,
            initial_group_config: _get_ints(&map, "initial_group_config")?,
            saved_data_name: map
                .get("saved_data_name")
                .map_or(String::from("data"), String::from),
            save_directory: map.get("save_directory").map_or(
                env::current_dir().or(Err(
                    "Missing save_directory and current working dir invalid",
                ))?,
                PathBuf::from,
            ),
            seed: map
                .get("seed")
                .map(|s| u64::from_str(&s).or(Err(format!("not an integer: {}", s))))
                .transpose()?,
        })
    }
    /// prepend base to relative paths
    pub fn resolve_paths(self, base: &Path) -> Parameters {
        let resolve = |p: PathBuf| if p.is_absolute() { p } else { base.join(p) };
        Self {
            gml_path: resolve(self.gml_path),
            save_directory: resolve(self.save_directory),
            ..self
        }
    }

    /// if no seed has been set yet, set based on current time.
    pub fn fix_seed(self) -> Parameters {
        Self {
            seed: self
                .seed
                .or_else(|| Some(time::UNIX_EPOCH.elapsed().unwrap().as_secs())),
            ..self
        }
    }
}
