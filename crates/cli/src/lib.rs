pub mod cli;
mod deps;
mod docs;
mod list;
pub mod monitor;
mod path;
mod profile;
mod project;
mod rename;
mod replace;
mod snapshot;
mod watch;
mod wizard;

#[allow(clippy::wildcard_imports)]
pub use cli::*;
pub use deps::{CleanupArgs, DepsArgs, handle_cleanup, handle_deps};
pub use docs::{DocsArgs, handle_docs};
pub use list::handle_list_command;
pub use monitor::MonitorArgs;
pub use path::handle_path_command;
pub use profile::{ProfileArgs, handle_profile};
pub use project::{ProjectArgs, handle_project};
pub use rename::{RenameArgs, handle_rename};
pub use replace::{handle_find_replace, handle_replace};
pub use snapshot::{SnapshotArgs, handle_snapshot};
pub use watch::{WatchArgs, handle_watch};
pub use wizard::{list_templates, run_wizard};
