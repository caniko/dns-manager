//! `dns-manager` command-line interface.
//!
//! Each subcommand reads a JSON document (from `--config <file>` or stdin) and
//! either prints resolved config or writes a rendered output directory.

use std::fs;
use std::io::{self, Read, Write};
use std::os::unix::fs::PermissionsExt;
use std::path::{Path, PathBuf};
use std::process::ExitCode;

use clap::{Args, Parser, Subcommand};

use dns_manager::render::{cloudflare, octodns, zonefile, OutputFile};
use dns_manager::{parse_document, resolve_document, Result};

#[derive(Parser)]
#[command(name = "dns-manager", version, about, long_about = None)]
struct Cli {
    #[command(subcommand)]
    command: Command,
}

#[derive(Subcommand)]
enum Command {
    /// Resolve raw DNS config and print the merged zones as JSON.
    Resolve(ConfigArgs),
    /// Render BIND zonefiles, one file per zone, into the output directory.
    Zonefile(OutArgs),
    /// Render a generic octoDNS config directory (config.yaml + zones/).
    Octodns(OutArgs),
    /// Render a Cloudflare octoDNS config directory (config.yaml + zones/).
    Cloudflare(OutArgs),
}

#[derive(Args)]
struct ConfigArgs {
    /// Input JSON file, or `-` for stdin.
    #[arg(short, long, default_value = "-")]
    config: String,
}

#[derive(Args)]
struct OutArgs {
    /// Input JSON file, or `-` for stdin.
    #[arg(short, long, default_value = "-")]
    config: String,
    /// Output directory (created if missing).
    #[arg(short, long)]
    out: PathBuf,
}

fn main() -> ExitCode {
    let cli = Cli::parse();
    match run(cli) {
        Ok(()) => ExitCode::SUCCESS,
        Err(err) => {
            eprintln!("dns-manager: {err}");
            ExitCode::FAILURE
        }
    }
}

fn run(cli: Cli) -> Result<()> {
    match cli.command {
        Command::Resolve(args) => {
            let doc = parse_document(&read_input(&args.config)?)?;
            let config = resolve_document(&doc)?;
            let json = serde_json::to_string_pretty(&config)?;
            io::stdout().write_all(json.as_bytes())?;
            io::stdout().write_all(b"\n")?;
            Ok(())
        }
        Command::Zonefile(args) => {
            let doc = parse_document(&read_input(&args.config)?)?;
            let config = resolve_document(&doc)?;
            let files: Vec<OutputFile> = zonefile::render_all(&config)
                .into_iter()
                .map(|(zone, content)| OutputFile::new(zone, content))
                .collect();
            write_outputs(&args.out, &files)
        }
        Command::Octodns(args) => {
            let input: octodns::OctodnsInput = serde_json::from_str(&read_input(&args.config)?)?;
            let config = resolve_document(&input.dns_config)?;
            let out_abs = prepare_out(&args.out)?;
            let files = octodns::render(&input, &config, &out_abs)?;
            write_outputs(&args.out, &files)
        }
        Command::Cloudflare(args) => {
            let input: cloudflare::CloudflareInput =
                serde_json::from_str(&read_input(&args.config)?)?;
            let config = resolve_document(&input.dns_config)?;
            let out_abs = prepare_out(&args.out)?;
            let files = cloudflare::render(&input, &config, &out_abs)?;
            write_outputs(&args.out, &files)
        }
    }
}

/// Read the input JSON from a file path, or stdin when the path is `-`.
fn read_input(config: &str) -> Result<String> {
    if config == "-" {
        let mut buf = String::new();
        io::stdin().read_to_string(&mut buf)?;
        Ok(buf)
    } else {
        Ok(fs::read_to_string(config)?)
    }
}

/// Create the output directory and return its absolute path (embedded into
/// octoDNS `directory` fields).
fn prepare_out(out: &Path) -> Result<String> {
    fs::create_dir_all(out)?;
    let abs = fs::canonicalize(out)?;
    Ok(abs.to_string_lossy().into_owned())
}

fn write_outputs(out: &Path, files: &[OutputFile]) -> Result<()> {
    fs::create_dir_all(out)?;
    for file in files {
        let path = out.join(&file.path);
        if let Some(parent) = path.parent() {
            fs::create_dir_all(parent)?;
        }
        fs::write(&path, &file.content)?;
        if file.executable {
            let mut perms = fs::metadata(&path)?.permissions();
            perms.set_mode(0o755);
            fs::set_permissions(&path, perms)?;
        }
    }
    Ok(())
}
