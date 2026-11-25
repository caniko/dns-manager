//! deSEC bootstrap and read-only DNS inspection via CLI or stdio MCP.

mod api;
mod mcp;
#[cfg(test)]
mod tests;

use std::env;
use std::fs;
use std::io::{self, Write};
use std::path::{Path, PathBuf};

use clap::{Args, Subcommand};

use self::api::{Client, MintArgs};
use crate::{Error, Result};

const READ_REQUIRED: &str =
    "deSEC read calls require --token-file, DESEC_TOKEN_FILE, or DESEC_TOKEN";
const ADMIN_REQUIRED: &str =
    "deSEC write calls require --admin-token-file or DESEC_ADMIN_TOKEN_FILE";

#[derive(Args)]
pub struct DesecArgs {
    /// Bare read token file; defaults to DESEC_TOKEN_FILE, then DESEC_TOKEN.
    #[arg(long, global = true)]
    token_file: Option<PathBuf>,
    /// Bare admin token file; defaults to DESEC_ADMIN_TOKEN_FILE.
    #[arg(long, global = true)]
    admin_token_file: Option<PathBuf>,
    #[command(subcommand)]
    command: Command,
}

#[derive(Subcommand)]
enum Command {
    /// List domains.
    Domains,
    /// Get a domain, including DNSKEY and DS records.
    Domain { name: String },
    /// List a domain's RRsets.
    Rrsets { name: String },
    /// Create an empty domain (admin token required).
    DomainCreate { name: String },
    /// Mint a token and save its secret in a new mode-0600 file (admin token required).
    TokenMint(MintArgs),
    /// Revoke a token by UUID (admin token required).
    TokenRevoke { id: String },
    /// Serve the deSEC MCP tools over stdio.
    McpServe {
        /// Private directory for MCP-minted token files; defaults to DESEC_TOKEN_OUTPUT_DIR.
        #[arg(long)]
        token_output_dir: Option<PathBuf>,
    },
}

struct Credentials {
    read: Option<Client>,
    admin: Option<Client>,
}

impl Credentials {
    fn read(&self) -> Result<&Client> {
        self.read.as_ref().ok_or_else(|| Error::msg(READ_REQUIRED))
    }

    fn admin(&self) -> Result<&Client> {
        self.admin
            .as_ref()
            .ok_or_else(|| Error::msg(ADMIN_REQUIRED))
    }
}

fn token_client(path: &Path) -> Result<Client> {
    let metadata = fs::metadata(path)?;
    if !metadata.is_file() || metadata.len() > 4096 {
        return Err(Error::msg("deSEC token file must be a small regular file"));
    }
    Client::new(fs::read_to_string(path)?)
}

pub fn run(args: DesecArgs) -> Result<()> {
    let token_file = args
        .token_file
        .or_else(|| env::var_os("DESEC_TOKEN_FILE").map(PathBuf::from));
    let admin_file = args
        .admin_token_file
        .or_else(|| env::var_os("DESEC_ADMIN_TOKEN_FILE").map(PathBuf::from));
    let load_read = || match token_file.as_deref() {
        Some(path) => token_client(path).map(Some),
        None => env::var("DESEC_TOKEN").ok().map(Client::new).transpose(),
    };
    let load_admin = || admin_file.as_deref().map(token_client).transpose();
    let read = || load_read()?.ok_or_else(|| Error::msg(READ_REQUIRED));
    let admin = || load_admin()?.ok_or_else(|| Error::msg(ADMIN_REQUIRED));

    let value = match args.command {
        Command::McpServe { token_output_dir } => {
            let credentials = Credentials {
                read: load_read()?,
                admin: load_admin()?,
            };
            let out = token_output_dir
                .or_else(|| env::var_os("DESEC_TOKEN_OUTPUT_DIR").map(PathBuf::from));
            return mcp::serve(
                io::stdin().lock(),
                io::stdout().lock(),
                &credentials,
                out.as_deref(),
            );
        }
        Command::Domains => read()?.domains()?,
        Command::Domain { name } => read()?.domain(&name)?,
        Command::Rrsets { name } => read()?.rrsets(&name)?,
        Command::DomainCreate { name } => admin()?.domain_create(&name)?,
        Command::TokenMint(args) => serde_json::to_value(admin()?.token_mint(&args, &args.out)?)?,
        Command::TokenRevoke { id } => admin()?.token_revoke(&id)?,
    };
    let mut out = io::stdout().lock();
    serde_json::to_writer_pretty(&mut out, &value)?;
    writeln!(out)?;
    Ok(())
}
