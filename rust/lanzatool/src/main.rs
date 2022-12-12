mod cli;
mod esp;
mod gc;
mod generation;
mod install;
mod pe;
mod signature;
mod utils;

use anyhow::Result;
use clap::Parser;

use cli::Cli;

fn main() -> Result<()> {
    Cli::parse().call()
}
