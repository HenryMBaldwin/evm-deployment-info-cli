use clap::{Parser, Subcommand};
use std::path::{Path, PathBuf};
use serde_json::Value;
use std::collections::HashMap;
use std::fs;
use regex::Regex;
use prettytable::{Table, row};

const VERSION: &str = "0.1.0";

#[derive(Parser)]
#[command(name = "evm-deployment-info")]
#[command(about = "A CLI tool for analyzing hardhat deployments")]
#[command(version = VERSION)]
struct Cli {
    #[command(subcommand)]
    command: Option<Commands>,

    /// Root directory of the hardhat project
    #[arg(short = 'p', long = "project", default_value = ".")]
    project: PathBuf,
}

#[derive(Subcommand)]
enum Commands {
    /// Count the number of deployments
    Count,
    /// List all deployments and their addresses
    List,
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

fn camel_to_title_case(s: &str) -> String {
    let re = Regex::new(r"([a-z0-9])([A-Z])").unwrap();
    let spaced = re.replace_all(s, "$1 $2").to_string();
    spaced.split_whitespace()
        .map(|word| {
            let mut chars = word.chars();
            match chars.next() {
                None => String::new(),
                Some(c) => c.to_uppercase().chain(chars).collect(),
            }
        })
        .collect::<Vec<_>>()
        .join(" ")
}

fn parse_hardhat_config(root: &Path) -> Result<HashMap<String, u64>, String> {
    let config_path = root.join("hardhat.config.ts");
    let content = fs::read_to_string(config_path)
        .map_err(|e| format!("Failed to read hardhat.config.ts: {}", e))?;

    let mut networks = HashMap::new();
    let network_regex = Regex::new(r#"(\w+):\s*\{[^}]*chainId:\s*(\d+)"#).unwrap();

    for cap in network_regex.captures_iter(&content) {
        let network_name = cap[1].to_string();
        let chain_id = cap[2].parse::<u64>()
            .map_err(|_| format!("Invalid chain ID for network {}", network_name))?;
        networks.insert(network_name, chain_id);
    }

    Ok(networks)
}

fn get_deployment_address(deployment_dir: &Path) -> Result<Option<String>, String> {
    let addresses_path = deployment_dir.join("deployed_addresses.json");
    if !addresses_path.exists() {
        return Ok(None);
    }

    let content = fs::read_to_string(addresses_path)
        .map_err(|e| format!("Failed to read deployed_addresses.json: {}", e))?;
    
    let data: Value = serde_json::from_str(&content)
        .map_err(|e| format!("Failed to parse deployed_addresses.json: {}", e))?;

    // Get the first address we find (assuming there's at least one)
    Ok(data.as_object()
        .and_then(|obj| obj.values().next())
        .and_then(|v| v.as_str())
        .map(String::from))
}

fn list_deployments(root: &Path) -> Result<(), String> {
    let networks = parse_hardhat_config(root)?;
    let deployments_dir = root.join("deployments");
    
    let mut found_deployments = Vec::new();
    let mut missing_deployments = Vec::new();

    for (network_name, chain_id) in networks {
        if network_name == "hardhat" {
            continue;
        }

        let chain_dir = deployments_dir.join(format!("chain-{}", chain_id));
        
        match get_deployment_address(&chain_dir) {
            Ok(Some(address)) => {
                found_deployments.push((network_name, address));
            }
            Ok(None) => {
                missing_deployments.push(network_name);
            }
            Err(e) => eprintln!("Warning: Error reading deployment for {}: {}", network_name, e),
        }
    }

    found_deployments.sort_by(|a, b| a.0.cmp(&b.0));
    missing_deployments.sort();

    if !found_deployments.is_empty() {
        println!("Found {} deployment(s):", found_deployments.len());
        let mut table = Table::new();
        table.add_row(row!["Network", "Address"]);
        for (network, address) in found_deployments {
            table.add_row(row![camel_to_title_case(&network), address]);
        }
        table.printstd();
    }

    if !missing_deployments.is_empty() {
        println!("\nFound the following {} chain(s) in hardhat config without corresponding deployment(s):",
            missing_deployments.len());
        let mut table = Table::new();
        table.add_row(row!["Network"]);
        for network in missing_deployments {
            table.add_row(row![camel_to_title_case(&network)]);
        }
        table.printstd();
    }

    Ok(())
}

fn main() {
    let cli = Cli::parse();
    
    if let Err(e) = validate_hardhat_project(&cli.project) {
        eprintln!("Error: {}", e);
        std::process::exit(1);
    }

    match cli.command {
        None => {
            println!("No command provided. Use --help to see available commands.");
        }
        Some(cmd) => {
            let result = match cmd {
                Commands::Count => count_deployments(&cli.project)
                    .map(|count| println!("Found {} deployment(s)", count)),
                Commands::List => list_deployments(&cli.project),
            };

            if let Err(e) = result {
                eprintln!("Error: {}", e);
                std::process::exit(1);
            }
        }
    }
}
