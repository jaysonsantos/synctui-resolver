use anyhow::Result;
use clap::Parser;

mod model;
mod ops;
mod scan;
mod tui;

fn main() -> Result<()> {
    let args = tui::Args::parse();
    tui::run(args)
}
