mod build;
mod cli;
mod config;
mod db;
mod fetch;
mod install;
mod query;
mod recipe;
mod remove;
mod resolver;
mod sync;
mod util;
mod world;

use anyhow::Result;
use clap::Parser;
use std::path::Path;

use cli::{Args, Command};

fn main() {
    let args = Args::parse();
    util::set_quiet(args.quiet);
    if let Err(e) = run(&args) {
        util::error(&format!("{}", e));
        for cause in e.chain().skip(1) {
            util::error(&format!("  {}", cause));
        }
        std::process::exit(1);
    }
}

fn run(args: &Args) -> Result<()> {
    let root = args.root.clone().unwrap_or_else(|| "/".to_string());
    let cfg = config::Config::load(Path::new(&root));
    let paths = config::Paths::new(Path::new(&root), &cfg, args.jobs);
    match &args.command {
        Command::Sync => sync::run(&paths),
        Command::Install { packages, force } => install::run(&paths, packages, *force),
        Command::Remove { packages, yes } => remove::run(&paths, packages, *yes),
        Command::Search { query } => query::search(&paths, query),
        Command::Info { package } => match package {
            Some(p) => query::info_pkg(&paths, p),
            None => query::info_system(&paths),
        },
        Command::List { installed } => query::list(&paths, *installed),
        Command::Deps { package } => query::deps(&paths, package),
        Command::Upgrade => install::upgrade(&paths),
        Command::Depclean { yes } => remove::depclean(&paths, *yes),
        Command::Quickpkg { packages } => install::quickpkg(&paths, packages),
        Command::File { path } => query::file(&paths, path),
        Command::Fetch { packages, force } => install::fetch_only(&paths, packages, *force),
        Command::World => query::world(&paths),
    }
}
