use hcp_rs::parameters::Parameters;
use hcp_rs::HierarchicalModel;
use std::env;
use std::fs::File;
use std::path::PathBuf;

fn main() -> Result<(), String> {
    let parameters_file = PathBuf::from(
        env::args()
            .nth(1)
            .ok_or(String::from("missing parameters file"))?,
    );
    let parameters = Parameters::load(File::open(parameters_file).map_err(|e| e.to_string())?)?;
    let hcp = HierarchicalModel::with_parameters(&parameters);
    println!("{:?}", parameters);
    Ok(())
}
