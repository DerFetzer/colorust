use std::path::PathBuf;

use clap::Parser;
use color_eyre::eyre::{Result, WrapErr};
use colorust::mlt::{add_filtergraph_to_producers, get_filter_strings};
use log::debug;
use roxmltree::Document;

#[derive(Parser)]
#[command(author, version)]
struct Cli {
    /// Kdenlive input file
    input: PathBuf,

    /// Output file
    output: PathBuf,

    /// Delete existing filtergraph properties from all producers
    #[arg(short, long)]
    delete_existing_filtergraph: bool,
}

fn main() -> Result<()> {
    colorust::init_logging()?;

    let cli = Cli::parse();

    let mlt = std::fs::read_to_string(cli.input).wrap_err("Could not read input file")?;
    let doc = Document::parse(&mlt).wrap_err("Could not parse input file as XML")?;

    let filter_strings = get_filter_strings(&doc.root());
    debug!("Filter strings: {filter_strings:#?}");

    let mlt_out =
        add_filtergraph_to_producers(mlt, &filter_strings, cli.delete_existing_filtergraph);
    std::fs::write(cli.output, mlt_out).wrap_err("Could not write output file")?;

    Ok(())
}
