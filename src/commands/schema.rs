use std::path::Path;

use crate::js_runner;

pub fn run(file: &Path) -> anyhow::Result<()> {
    let result = js_runner::run(file)?;
    println!("{result}");
    Ok(())
}
