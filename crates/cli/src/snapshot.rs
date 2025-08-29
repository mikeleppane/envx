use clap::{Args, Subcommand};
use color_eyre::Result;
use comfy_table::Table;
use envx_core::{EnvVarManager, SnapshotManager};

#[derive(Args)]
pub struct SnapshotArgs {
    #[command(subcommand)]
    pub command: SnapshotCommands,
}

#[derive(Subcommand)]
pub enum SnapshotCommands {
    /// Create a new snapshot
    Create {
        /// Snapshot name
        name: String,
        /// Description
        #[arg(short, long)]
        description: Option<String>,
    },
    /// List all snapshots
    List,
    /// Show details of a snapshot
    Show {
        /// Snapshot name or ID
        snapshot: String,
    },
    /// Restore from a snapshot
    Restore {
        /// Snapshot name or ID
        snapshot: String,
        /// Force restore without confirmation
        #[arg(short, long)]
        force: bool,
    },
    /// Delete a snapshot
    Delete {
        /// Snapshot name or ID
        snapshot: String,
        /// Force deletion without confirmation
        #[arg(short, long)]
        force: bool,
    },
    /// Compare two snapshots
    Diff {
        /// First snapshot
        snapshot1: String,
        /// Second snapshot
        snapshot2: String,
    },
}

/// Handle snapshot-related commands.
///
/// # Errors
///
/// This function will return an error if:
/// - The snapshot manager cannot be initialized
/// - Environment variable loading fails
/// - Snapshot operations fail (create, restore, delete, etc.)
/// - File I/O operations fail during snapshot operations
/// - User input cannot be read from stdin
/// - Invalid snapshot names or IDs are provided
pub fn handle_snapshot(args: SnapshotArgs) -> Result<()> {
    let snapshot_manager = SnapshotManager::new()?;
    let mut env_manager = EnvVarManager::new();
    env_manager.load_all()?;

    match args.command {
        SnapshotCommands::Create { name, description } => {
            let vars = env_manager.list().into_iter().cloned().collect();
            let snapshot = snapshot_manager.create(name, description, vars)?;
            println!("✅ Created snapshot: {} (ID: {})", snapshot.name, snapshot.id);
        }
        SnapshotCommands::List => {
            let snapshots = snapshot_manager.list()?;
            if snapshots.is_empty() {
                println!("No snapshots found.");
                return Ok(());
            }

            let mut table = Table::new();
            table.set_header(vec!["Name", "ID", "Created", "Variables", "Description"]);

            for snapshot in snapshots {
                table.add_row(vec![
                    snapshot.name,
                    snapshot.id[..8].to_string(),
                    snapshot.created_at.format("%Y-%m-%d %H:%M").to_string(),
                    snapshot.variables.len().to_string(),
                    snapshot.description.unwrap_or_default(),
                ]);
            }

            println!("{table}");
        }
        SnapshotCommands::Show { snapshot } => {
            let snap = snapshot_manager.get(&snapshot)?;
            println!("Snapshot: {}", snap.name);
            println!("ID: {}", snap.id);
            println!("Created: {}", snap.created_at.format("%Y-%m-%d %H:%M:%S"));
            println!("Description: {}", snap.description.unwrap_or_default());
            println!("Variables: {}", snap.variables.len());

            // Show first 10 variables
            println!("\nFirst 10 variables:");
            for (i, (name, var)) in snap.variables.iter().take(10).enumerate() {
                println!("  {}. {} = {}", i + 1, name, var.value);
            }

            if snap.variables.len() > 10 {
                println!("  ... and {} more", snap.variables.len() - 10);
            }
        }
        SnapshotCommands::Restore { snapshot, force } => {
            if !force {
                print!("⚠️  This will replace all current environment variables. Continue? [y/N] ");
                std::io::Write::flush(&mut std::io::stdout())?;

                let mut input = String::new();
                std::io::stdin().read_line(&mut input)?;
                if !input.trim().eq_ignore_ascii_case("y") {
                    println!("Cancelled.");
                    return Ok(());
                }
            }

            snapshot_manager.restore(&snapshot, &mut env_manager)?;
            println!("✅ Restored from snapshot: {snapshot}");
        }
        SnapshotCommands::Delete { snapshot, force } => {
            if !force {
                print!("⚠️  Delete snapshot '{snapshot}'? [y/N] ");
                std::io::Write::flush(&mut std::io::stdout())?;

                let mut input = String::new();
                std::io::stdin().read_line(&mut input)?;
                if !input.trim().eq_ignore_ascii_case("y") {
                    println!("Cancelled.");
                    return Ok(());
                }
            }

            snapshot_manager.delete(&snapshot)?;
            println!("✅ Deleted snapshot: {snapshot}");
        }
        SnapshotCommands::Diff { snapshot1, snapshot2 } => {
            let diff = snapshot_manager.diff(&snapshot1, &snapshot2)?;

            if diff.added.is_empty() && diff.removed.is_empty() && diff.modified.is_empty() {
                println!("No differences found between snapshots.");
                return Ok(());
            }

            if !diff.added.is_empty() {
                println!("➕ Added in {snapshot2}:");
                for (name, var) in &diff.added {
                    println!("   {} = {}", name, var.value);
                }
            }

            if !diff.removed.is_empty() {
                println!("\n➖ Removed in {snapshot2}:");
                for (name, var) in &diff.removed {
                    println!("   {} = {}", name, var.value);
                }
            }

            if !diff.modified.is_empty() {
                println!("\n🔄 Modified:");
                for (name, (old, new)) in &diff.modified {
                    println!("   {name}:");
                    println!("     Old: {}", old.value);
                    println!("     New: {}", new.value);
                }
            }
        }
    }

    Ok(())
}
