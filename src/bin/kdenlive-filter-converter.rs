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
    /// (filters are extracted from here)
    input: PathBuf,

    /// Output file
    output: PathBuf,

    /// Optional file where the filtergraph strings are inserted
    /// (if not set the input file is used)
    #[arg(short, long)]
    insert_into: Option<PathBuf>,

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

    let insert_into = cli
        .insert_into
        .map(|p| std::fs::read_to_string(p).wrap_err("Could not read insert_into file"));

    let insert_into = match insert_into {
        None => mlt,
        Some(Ok(insert_into)) => insert_into,
        Some(Err(e)) => return Err(e),
    };

    let mlt_out = add_filtergraph_to_producers(
        insert_into,
        &filter_strings,
        cli.delete_existing_filtergraph,
    );
    std::fs::write(cli.output, mlt_out).wrap_err("Could not write output file")?;

    Ok(())
}
