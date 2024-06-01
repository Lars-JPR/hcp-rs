use hcp_rs::parameters::Parameters;
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
    println!("{:?}", parameters);
    Ok(())
}
