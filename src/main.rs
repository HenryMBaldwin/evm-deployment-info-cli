use clap::{Parser, Subcommand};
use std::path::{Path, PathBuf};

const VERSION: &str = "0.1.0";

#[derive(Parser)]
#[command(name = "evm-deployment-info")]
#[command(about = "A CLI tool for analyzing hardhat deployments")]
#[command(version = VERSION)]
struct Cli {
    #[command(subcommand)]
    command: Option<Commands>,

    /// Root directory of the hardhat project
    #[arg(short = 'r', long = "root", default_value = ".")]
    root: PathBuf,
}

#[derive(Subcommand)]
enum Commands {
    /// Count the number of deployments
    Count,
}

fn validate_hardhat_project(root: &Path) -> Result<(), String> {
    let config_path = root.join("hardhat.config.ts");
    if !config_path.exists() {
        return Err("No hardhat.config.ts found in the specified root directory".to_string());
    }
    Ok(())
}

fn count_deployments(root: &Path) -> Result<usize, String> {
    let deployments_dir = root.join("deployments");
    if !deployments_dir.exists() {
        return Ok(0);
    }

    match deployments_dir.read_dir() {
        Ok(entries) => Ok(entries.filter(|e| e.is_ok() && e.as_ref().unwrap().path().is_dir()).count()),
        Err(e) => Err(format!("Failed to read deployments directory: {}", e)),
    }
}

fn main() {
    let cli = Cli::parse();
    
    // Validate the hardhat project first
    if let Err(e) = validate_hardhat_project(&cli.root) {
        eprintln!("Error: {}", e);
        std::process::exit(1);
    }

    match cli.command {
        None => {
            println!("No command provided. Use --help to see available commands.");
        }
        Some(cmd) => {
            match cmd {
                Commands::Count => {
                    match count_deployments(&cli.root) {
                        Ok(count) => println!("Found {} deployment(s)", count),
                        Err(e) => {
                            eprintln!("Error counting deployments: {}", e);
                            std::process::exit(1);
                        }
                    }
                }
            }
        }
    }
}
