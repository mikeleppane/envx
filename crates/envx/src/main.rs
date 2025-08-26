use clap::Parser;
use color_eyre::Result;
use envx_cli::Cli;

#[cfg(not(windows))]
use jemallocator::Jemalloc;
#[cfg(windows)]
use mimalloc::MiMalloc;

#[cfg(windows)]
#[global_allocator]
static GLOBAL: MiMalloc = MiMalloc;

#[cfg(not(windows))]
#[global_allocator]
static GLOBAL: Jemalloc = Jemalloc;

fn main() -> Result<()> {
    // Initialize tracing
    tracing_subscriber::fmt().with_max_level(tracing::Level::DEBUG).init();
    color_eyre::install()?;

    let cli = Cli::parse();

    envx_cli::execute(cli)?;

    Ok(())
}
