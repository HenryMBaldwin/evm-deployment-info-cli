use clap::{Parser, Subcommand};
use std::path::{Path, PathBuf};
use serde_json::Value;
use std::collections::HashMap;
use std::fs;
use regex::Regex;
use prettytable::{Table, row};
use std::collections::BTreeMap;
use prettytable::format;
use std::process::Command;
use reqwest;

const VERSION: &str = "0.1.1";

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
    /// Count the number of deployments in the deployments directory
    Count,
    /// List all deployments and their addresses
    List {
        /// Aggregate networks with common prefixes
        #[arg(short = 'a', long = "aggregate")]
        aggregate: bool,
        /// Output in JSON format
        #[arg(short = 'j', long = "json", conflicts_with = "csv", group = "output_format")]
        json: bool,
        /// Output in CSV format
        #[arg(short = 'c', long = "csv", conflicts_with = "json", group = "output_format")]
        csv: bool,
        /// Output file (only valid with --json or --csv)
        #[arg(short = 'o', long = "outfile", requires = "output_format")]
        outfile: Option<PathBuf>,
    },
    /// Audit deployments and config entries
    Audit {
        /// Output in JSON format
        #[arg(short = 'j', long = "json", conflicts_with = "csv", group = "output_format")]
        json: bool,
        /// Output in CSV format
        #[arg(short = 'c', long = "csv", conflicts_with = "json", group = "output_format")]
        csv: bool,
        /// Output file (only valid with --json or --csv)
        #[arg(short = 'o', long = "outfile", requires = "output_format")]
        outfile: Option<PathBuf>,
    },
    /// Display version information
    Version,
    
    /// Check for updates and install the latest version
    #[command(aliases = ["upgrade"])]
    Update {
        /// Force update without version check
        #[arg(short = 'f', long = "force")]
        force: bool,
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

fn list_deployments(root: &Path, aggregate: bool, json: bool, csv: bool, outfile: Option<&Path>) -> Result<(), String> {
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

        let output = serde_json::to_string_pretty(&output).map_err(|e| e.to_string())?;
        if let Some(path) = outfile {
            if let Some(parent) = path.parent() {
                fs::create_dir_all(parent).map_err(|e| format!("Failed to create directory: {}", e))?;
            }
            fs::write(path, output).map_err(|e| format!("Failed to write to file: {}", e))?;
        } else {
            println!("{}", output);
        }
    } else if csv {
        let mut csv_content = String::from("Network,Address\n");
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
                    csv_content.push_str(&format!("{} {},{}\n",
                        camel_to_title_case(&prefix),
                        camel_to_title_case(&suffix),
                        address
                    ));
                }
            }

            if !missing_deployments.is_empty() {
                csv_content.push_str("\nMissing Networks\n");
                for network in missing_deployments {
                    let parts: Vec<&str> = network.split(|c: char| c.is_uppercase()).collect();
                    let prefix = parts[0].to_string();
                    let suffix = network[prefix.len()..].to_string();
                    
                    let suffix = if suffix.is_empty() {
                        "Mainnet".to_string()
                    } else {
                        suffix
                    };
                    
                    csv_content.push_str(&format!("{} {},",
                        camel_to_title_case(&prefix),
                        camel_to_title_case(&suffix)
                    ));
                }
            }
        } else {
            for (network, address) in found_deployments {
                csv_content.push_str(&format!("{},{}\n", camel_to_title_case(&network), address));
            }
            
            if !missing_deployments.is_empty() {
                csv_content.push_str("\nMissing Networks\n");
                for network in missing_deployments {
                    csv_content.push_str(&format!("{},", camel_to_title_case(&network)));
                }
            }
        }

        if let Some(path) = outfile {
            fs::write(path, csv_content).map_err(|e| format!("Failed to write to file: {}", e))?;
        } else {
            print!("{}", csv_content);
        }
    } else {
        if !found_deployments.is_empty() {
            if aggregate {
                let mut grouped: BTreeMap<String, Vec<(String, String)>> = BTreeMap::new();
                for (network, address) in found_deployments.clone() {
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

                println!("Found {} Ecosystem(s) for a total of {} deployment(s):", 
                    grouped.len(),
                    found_deployments.len()
                );

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
                println!("Found {} deployment(s):", found_deployments.len());
                
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

fn audit_deployments(root: &Path, json: bool, csv: bool, outfile: Option<&Path>) -> Result<(), String> {
    let networks = parse_hardhat_config(root)?;
    let deployments_dir = root.join("deployments");
    
    let mut config_without_deployment = Vec::new();
    let mut deployment_without_config = Vec::new();

    // Check for configs without deployments
    for (network_name, chain_id) in &networks {
        if network_name == "hardhat" {
            continue;
        }
        let chain_dir = deployments_dir.join(format!("chain-{}", chain_id));
        if !chain_dir.exists() || get_deployment_address(&chain_dir)?.is_none() {
            config_without_deployment.push((network_name.clone(), *chain_id));
        }
    }

    // Check for deployments without configs
    if deployments_dir.exists() {
        for entry in fs::read_dir(&deployments_dir).map_err(|e| e.to_string())? {
            let entry = entry.map_err(|e| e.to_string())?;
            let path = entry.path();
            if path.is_dir() {
                if let Some(dir_name) = path.file_name().and_then(|n| n.to_str()) {
                    if let Some(chain_id) = dir_name.strip_prefix("chain-") {
                        if let Ok(chain_id) = chain_id.parse::<u64>() {
                            if !networks.values().any(|&id| id == chain_id) {
                                deployment_without_config.push(chain_id);
                            }
                        }
                    }
                }
            }
        }
    }

    if json {
        let mut output = serde_json::Map::new();
        output.insert(
            "config_without_deployment".to_string(),
            serde_json::json!(config_without_deployment
                .iter()
                .map(|(name, id)| {
                    serde_json::json!({
                        "network": name,
                        "chain_id": id
                    })
                })
                .collect::<Vec<_>>())
        );
        output.insert(
            "deployment_without_config".to_string(),
            serde_json::json!(deployment_without_config)
        );

        let output = serde_json::to_string_pretty(&output).map_err(|e| e.to_string())?;
        if let Some(path) = outfile {
            if let Some(parent) = path.parent() {
                fs::create_dir_all(parent).map_err(|e| format!("Failed to create directory: {}", e))?;
            }
            fs::write(path, output).map_err(|e| format!("Failed to write to file: {}", e))?;
        } else {
            println!("{}", output);
        }
    } else if csv {
        let mut csv_content = String::new();
        
        csv_content.push_str("Configs Without Deployments\nNetwork,Chain ID\n");
        for (name, id) in &config_without_deployment {
            csv_content.push_str(&format!("{},{}\n", name, id));
        }
        
        csv_content.push_str("\nDeployments Without Configs\nChain ID\n");
        for id in &deployment_without_config {
            csv_content.push_str(&format!("{}\n", id));
        }

        if let Some(path) = outfile {
            if let Some(parent) = path.parent() {
                fs::create_dir_all(parent).map_err(|e| format!("Failed to create directory: {}", e))?;
            }
            fs::write(path, csv_content).map_err(|e| format!("Failed to write to file: {}", e))?;
        } else {
            print!("{}", csv_content);
        }
    } else {
        if !config_without_deployment.is_empty() {
            println!("\nFound {} network(s) in config without deployments:", config_without_deployment.len());
            let mut table = Table::new();
            table.set_format(create_sui_style_format());
            table.add_row(row![bF-> "Network", bF-> "Chain ID"]);
            for (name, id) in config_without_deployment {
                table.add_row(row![name, id]);
            }
            table.printstd();
        }

        if !deployment_without_config.is_empty() {
            println!("\nFound {} deployment(s) without config entries:", deployment_without_config.len());
            let mut table = Table::new();
            table.set_format(create_sui_style_format());
            table.add_row(row![bF-> "Chain ID", bF-> "Chain List"]);
            
            for id in deployment_without_config {
                table.add_row(row![
                    id,
                    Fb-> format!("https://chainlist.org/chain/{}", id)
                ]);
            }
            table.printstd();
        }
    }

    Ok(())
}

fn get_latest_version() -> Result<String, String> {
    let client = reqwest::blocking::Client::builder()
        .user_agent("evm-deployment-info-cli")
        .build()
        .map_err(|e| format!("Failed to create HTTP client: {}", e))?;

    let response = client
        .get("https://api.github.com/repos/HenryMBaldwin/evm-deployment-info-cli/releases/latest")
        .send()
        .map_err(|e| format!("Failed to check for updates: {}", e))?;
    
    if !response.status().is_success() {
        return Err("Failed to get latest version information".to_string());
    }

    let release: serde_json::Value = response.json()
        .map_err(|e| format!("Failed to parse response: {}", e))?;
    
    release["tag_name"]
        .as_str()
        .map(|v| v.trim_start_matches('v').to_string())
        .ok_or_else(|| "Invalid version format in response".to_string())
}

fn check_install_permissions() -> bool {
    let install_path = Path::new("/usr/local/bin");
    match fs::metadata(install_path) {
        Ok(metadata) => {
            #[cfg(unix)]
            {
                use std::os::unix::fs::MetadataExt;
                let uid = unsafe { libc::getuid() };
                metadata.uid() == uid || uid == 0
            }
            #[cfg(not(unix))]
            {
                true
            }
        }
        Err(_) => false,
    }
}

fn main() {
    let cli = Cli::parse();
    
    match cli.command {
        None => {
            println!("No command provided. Use --help to see available commands.");
        }
        Some(cmd) => {
            // Handle version and update commands before project validation
            match cmd {
                Commands::Version => {
                    println!("evm-deployment-info v{}", VERSION);
                    return;
                }
                Commands::Update { force } => {
                    println!("Checking for updates...");
                    
                    match get_latest_version() {
                        Ok(latest_version) => {
                            if !force && latest_version == VERSION {
                                println!("You're already running the latest version ({})", VERSION);
                                return ();
                            }
                            
                            println!("Current version: {}", VERSION);
                            println!("Latest version:  {}", latest_version);
                            
                            if !force && latest_version < VERSION.to_string() {
                                println!("Warning: Latest version is older than current version");
                                println!("Use --force to update anyway");
                                return ();
                            }

                            if !check_install_permissions() {
                                println!("Error: Insufficient permissions to perform update");
                                println!("Please run with sudo:");
                                println!("\n    sudo evm-deployment-info update\n");
                                return ();
                            }
                            
                            println!("Installing update...");
                            
                            let install_cmd = r#"
                                curl -fsSL https://raw.githubusercontent.com/HenryMBaldwin/evm-deployment-info-cli/refs/heads/master/install.sh | sudo bash
                            "#;
                            
                            match Command::new("sh")
                                .arg("-c")
                                .arg(install_cmd)
                                .status() 
                            {
                                Ok(status) => {
                                    if status.success() {
                                        println!("Successfully updated to version {}", latest_version);
                                        return ();
                                    } else {
                                        println!("Failed to update. Please try again or update manually");
                                        return ();
                                    }
                                }
                                Err(e) => {
                                    println!("Error during update: {}", e);
                                    return ();
                                }
                            }
                        }
                        Err(e) => {
                            println!("Error checking for updates: {}", e);
                            return ();
                        }
                    }
                }
                _ => {
                    // Validate hardhat project for all other commands
                    if let Err(e) = validate_hardhat_project(&cli.project) {
                        eprintln!("Error: {}", e);
                        std::process::exit(1);
                    }
                }
            }

            let result = match cmd {
                Commands::Count => count_deployments(&cli.project)
                    .map(|count| println!("Found {} deployment(s)", count)),
                Commands::List { aggregate, json, csv, outfile } => {
                    list_deployments(&cli.project, aggregate, json, csv, outfile.as_deref())
                }
                Commands::Audit { json, csv, outfile } => {
                    audit_deployments(&cli.project, json, csv, outfile.as_deref())
                }
                Commands::Version | Commands::Update { .. } => Ok(()),
            };

            if let Err(e) = result {
                eprintln!("Error: {}", e);
                std::process::exit(1);
            }
        }
    }
}
