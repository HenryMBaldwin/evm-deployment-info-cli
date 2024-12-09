use clap::{Parser, Subcommand};
use std::path::{Path, PathBuf};
use serde_json::Value;
use std::collections::HashMap;
use std::fs;
use regex::Regex;
use prettytable::{Table, row};
use std::collections::BTreeMap;
use prettytable::format;

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
    List {
        /// Aggregate networks with common prefixes
        #[arg(short = 'a', long = "aggregate")]
        aggregate: bool,
        /// Output in JSON format
        #[arg(short = 'j', long = "json", conflicts_with = "csv")]
        json: bool,
        /// Output in CSV format
        #[arg(short = 'c', long = "csv", conflicts_with = "json")]
        csv: bool,
    },
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

fn create_sui_style_format() -> prettytable::format::TableFormat {
    let format = format::FormatBuilder::new()
        .column_separator('│')
        .borders('│')
        .separator(
            format::LinePosition::Top,
            format::LineSeparator::new('─', '┬', '╭', '╮'),
        )
        .separator(
            format::LinePosition::Bottom,
            format::LineSeparator::new('─', '┴', '╰', '╯'),
        )
        .separator(
            format::LinePosition::Intern,
            format::LineSeparator::new('─', '┼', '├', '┤'),
        )
        .padding(1, 1)
        .build();
    format
}

fn list_deployments(root: &Path, aggregate: bool, json: bool, csv: bool) -> Result<(), String> {
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

    if json {
        let mut output = serde_json::Map::new();
        
        if !found_deployments.is_empty() {
            if aggregate {
                let mut grouped = serde_json::Map::new();
                for (network, address) in found_deployments {
                    let parts: Vec<&str> = network.split(|c: char| c.is_uppercase()).collect();
                    let prefix = parts[0].to_string();
                    let suffix = network[prefix.len()..].to_string();
                    
                    let suffix = if suffix.is_empty() {
                        "Mainnet".to_string()
                    } else {
                        suffix
                    };
                    
                    let entry = grouped.entry(prefix).or_insert_with(|| serde_json::Value::Object(serde_json::Map::new()));
                    if let Some(obj) = entry.as_object_mut() {
                        obj.insert(suffix, serde_json::Value::String(address));
                    }
                }
                output.insert("deployments".to_string(), serde_json::Value::Object(grouped));
            } else {
                let mut deployments = serde_json::Map::new();
                for (network, address) in found_deployments {
                    deployments.insert(network, serde_json::Value::String(address));
                }
                output.insert("deployments".to_string(), serde_json::Value::Object(deployments));
            }
        }

        if !missing_deployments.is_empty() {
            if aggregate {
                let mut grouped = serde_json::Map::new();
                for network in missing_deployments {
                    let parts: Vec<&str> = network.split(|c: char| c.is_uppercase()).collect();
                    let prefix = parts[0].to_string();
                    let suffix = network[prefix.len()..].to_string();
                    
                    let suffix = if suffix.is_empty() {
                        "Mainnet".to_string()
                    } else {
                        suffix
                    };
                    
                    let entry = grouped.entry(prefix).or_insert_with(|| serde_json::Value::Array(Vec::new()));
                    if let Some(arr) = entry.as_array_mut() {
                        arr.push(serde_json::Value::String(suffix));
                    }
                }
                output.insert("missing".to_string(), serde_json::Value::Object(grouped));
            } else {
                output.insert(
                    "missing".to_string(),
                    serde_json::Value::Array(
                        missing_deployments.into_iter()
                            .map(serde_json::Value::String)
                            .collect()
                    )
                );
            }
        }

        println!("{}", serde_json::to_string_pretty(&output).map_err(|e| e.to_string())?);
    } else if csv {
        println!("Network,Address");
        if aggregate {
            let mut grouped: BTreeMap<String, Vec<(String, String)>> = BTreeMap::new();
            for (network, address) in found_deployments {
                let parts: Vec<&str> = network.split(|c: char| c.is_uppercase()).collect();
                let prefix = parts[0].to_string();
                let suffix = network[prefix.len()..].to_string();
                
                let suffix = if suffix.is_empty() {
                    "Mainnet".to_string()
                } else {
                    suffix
                };
                
                grouped.entry(prefix)
                    .or_default()
                    .push((suffix, address));
            }

            for (prefix, mut networks) in grouped {
                networks.sort_by(|a, b| {
                    if a.0 == "Mainnet" {
                        std::cmp::Ordering::Less
                    } else if b.0 == "Mainnet" {
                        std::cmp::Ordering::Greater
                    } else {
                        a.0.cmp(&b.0)
                    }
                });
                
                for (suffix, address) in networks {
                    println!("{} {},{}",
                        camel_to_title_case(&prefix),
                        camel_to_title_case(&suffix),
                        address
                    );
                }
            }

            if !missing_deployments.is_empty() {
                println!("\nMissing Networks");
                for network in missing_deployments {
                    let parts: Vec<&str> = network.split(|c: char| c.is_uppercase()).collect();
                    let prefix = parts[0].to_string();
                    let suffix = network[prefix.len()..].to_string();
                    
                    let suffix = if suffix.is_empty() {
                        "Mainnet".to_string()
                    } else {
                        suffix
                    };
                    
                    println!("{} {},",
                        camel_to_title_case(&prefix),
                        camel_to_title_case(&suffix)
                    );
                }
            }
        } else {
            for (network, address) in found_deployments {
                println!("{},{}", camel_to_title_case(&network), address);
            }
            
            if !missing_deployments.is_empty() {
                println!("\nMissing Networks");
                for network in missing_deployments {
                    println!("{},", camel_to_title_case(&network));
                }
            }
        }
    } else {
        if !found_deployments.is_empty() {
            println!("Found {} deployment(s):", found_deployments.len());
            
            if aggregate {
                let mut grouped: BTreeMap<String, Vec<(String, String)>> = BTreeMap::new();
                for (network, address) in found_deployments {
                    let parts: Vec<&str> = network.split(|c: char| c.is_uppercase()).collect();
                    let prefix = parts[0].to_string();
                    let suffix = network[prefix.len()..].to_string();
                    
                    let suffix = if suffix.is_empty() {
                        "Mainnet".to_string()
                    } else {
                        suffix
                    };
                    
                    grouped.entry(prefix)
                        .or_default()
                        .push((suffix, address));
                }

                let mut table = Table::new();
                table.set_format(create_sui_style_format());
                table.add_row(row![bF-> "Network", bF-> "Address"]);

                for (prefix, mut networks) in grouped {
                    networks.sort_by(|a, b| {
                        if a.0 == "Mainnet" {
                            std::cmp::Ordering::Less
                        } else if b.0 == "Mainnet" {
                            std::cmp::Ordering::Greater
                        } else {
                            a.0.cmp(&b.0)
                        }
                    });
                    
                    table.add_row(row![bF-> format!("{}:", camel_to_title_case(&prefix)), ""]);
                    
                    for (suffix, address) in networks {
                        table.add_row(row![
                            format!("  {}", camel_to_title_case(&suffix)),
                            address
                        ]);
                    }
                }
                table.printstd();
            } else {
                let mut table = Table::new();
                table.set_format(create_sui_style_format());
                table.add_row(row![bF-> "Network", bF-> "Address"]);
                
                found_deployments.sort_by(|a, b| a.0.cmp(&b.0));
                for (network, address) in found_deployments {
                    table.add_row(row![camel_to_title_case(&network), address]);
                }
                table.printstd();
            }
        }

        if !missing_deployments.is_empty() {
            println!("\nFound the following {} chain(s) in hardhat config without corresponding deployment(s):",
                missing_deployments.len());
            
            if aggregate {
                let mut grouped: BTreeMap<String, Vec<String>> = BTreeMap::new();
                for network in missing_deployments {
                    let parts: Vec<&str> = network.split(|c: char| c.is_uppercase()).collect();
                    let prefix = parts[0].to_string();
                    let suffix = network[prefix.len()..].to_string();
                    
                    let suffix = if suffix.is_empty() {
                        "Mainnet".to_string()
                    } else {
                        suffix
                    };
                    
                    grouped.entry(prefix)
                        .or_default()
                        .push(suffix);
                }

                let mut table = Table::new();
                table.set_format(create_sui_style_format());
                table.add_row(row![bF-> "Network"]);

                for (prefix, mut networks) in grouped {
                    networks.sort_by(|a, b| {
                        if a == "Mainnet" {
                            std::cmp::Ordering::Less
                        } else if b == "Mainnet" {
                            std::cmp::Ordering::Greater
                        } else {
                            a.cmp(b)
                        }
                    });
                    
                    table.add_row(row![bF-> format!("{}:", camel_to_title_case(&prefix))]);
                    for suffix in networks {
                        table.add_row(row![format!("  {}", camel_to_title_case(&suffix))]);
                    }
                }
                table.printstd();
            } else {
                let mut table = Table::new();
                table.set_format(create_sui_style_format());
                table.add_row(row![bF-> "Network"]);
                
                missing_deployments.sort();
                for network in missing_deployments {
                    table.add_row(row![camel_to_title_case(&network)]);
                }
                table.printstd();
            }
        }
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
                Commands::List { aggregate, json, csv } => list_deployments(&cli.project, aggregate, json, csv),
            };

            if let Err(e) = result {
                eprintln!("Error: {}", e);
                std::process::exit(1);
            }
        }
    }
}
