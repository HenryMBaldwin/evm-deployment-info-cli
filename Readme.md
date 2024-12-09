# EVM Deployment Info CLI

A small CLI tool to help you enumerate your EVM deployments from your Hardhat project.

## Installation

```bash
curl -fsSL https://raw.githubusercontent.com/HenryMBaldwin/evm-deployment-info/refs/heads/master/install.sh | sudo bash
```

or clone down this repository and install the CLI with cargo:

```bash
git clone https://github.com/HenryMBaldwin/evm-deployment-info.git
cd evm-deployment-info
```

Install the CLI with cargo:

```bash
cargo install --path .
```


## Usage

```bash
evm-deployment-info --help
```

```bash
evm-deployment-info --version
```

```bash
evm-deployment-info update
```


## Commands

`evm-deployment-info` should either be run from the root of your Hardhat project or with the `--project` flag.

### Count

Count the number of deployments in the deployments directory.

```bash
evm-deployment-info count
```

### List

List all deployments in the config with their addresses.

```bash
evm-deployment-info list
```

options:

- `--aggregate` - Aggregate networks with common prefixes (e.g. `Ethereum` and `Ethereum Sepolia` will be aggregated into `Ethereum`)
- `--json` - Output in JSON format
- `--csv` - Output in CSV format
- `--outfile` - Output to a file, must be used with `--json` or `--csv`

### Audit

Audit the deployments in the config and deployments directory. Checks if either contains any deployments that are not in the other.

```bash
evm-deployment-info audit
```

### Version 

Check the version of the CLI.

```bash
evm-deployment-info version
```

### Update

Update the CLI to the latest version.

```bash
evm-deployment-info update
```
