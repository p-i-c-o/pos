//! Operator-only account provisioning and revocation.
//!
//! These commands require local server shell access and the configured
//! PostgreSQL connection. They are never bound to HTTP routes.

use std::{
    error::Error,
    io::{self, Read},
    net::SocketAddr,
};

use clap::{Parser, Subcommand};
use pos_postgres::PostgresStore;
use uuid::Uuid;
use zeroize::Zeroizing;

#[derive(Parser)]
#[command(
    name = "pos-server",
    version,
    about = "POS local server and trusted administration"
)]
pub(crate) struct Cli {
    /// Local loopback listen address, or POS_SERVER_LISTEN.
    #[arg(long, env = "POS_SERVER_LISTEN", default_value = pos_server::DEFAULT_LISTEN)]
    pub listen: SocketAddr,

    #[command(subcommand)]
    pub action: Option<Command>,
}

#[derive(Subcommand)]
pub(crate) enum Command {
    /// Trusted local administrator account and device operations.
    Admin {
        #[command(subcommand)]
        action: AdminAction,
    },
}

#[derive(Subcommand)]
pub(crate) enum AdminAction {
    /// Provision an account and an owner-only personal workspace.
    CreateUser {
        #[arg(long)]
        email: String,
        /// Read the password from standard input instead of prompting.
        /// For controlled scripts only, never as a command-line argument.
        #[arg(long)]
        password_stdin: bool,
    },
    /// Revoke one device's access across all active sessions.
    RevokeDevice {
        #[arg(long)]
        user: Uuid,
        #[arg(long)]
        device: Uuid,
    },
    /// Disable one account (revokes access immediately). Requires --yes.
    DisableUser {
        #[arg(long)]
        user: Uuid,
        #[arg(long)]
        yes: bool,
    },
}

fn read_password(from_stdin: bool) -> Result<Zeroizing<String>, Box<dyn Error>> {
    if from_stdin {
        let mut bytes = Vec::new();
        io::stdin().take(1025).read_to_end(&mut bytes)?;
        let mut password = Zeroizing::new(String::from_utf8(bytes)?);
        while password.ends_with(['\n', '\r']) {
            password.pop();
        }
        return Ok(password);
    }

    let password = Zeroizing::new(rpassword::prompt_password("New POS account password: ")?);
    let confirmation = Zeroizing::new(rpassword::prompt_password("Confirm password: ")?);
    if *password != *confirmation {
        return Err("passwords do not match".into());
    }
    Ok(password)
}

pub(crate) async fn run_admin(action: AdminAction) -> Result<(), Box<dyn Error>> {
    if matches!(action, AdminAction::DisableUser { yes: false, .. }) {
        return Err("account disablement requires explicit --yes".into());
    }
    let url = std::env::var("POS_SERVER_DATABASE_URL")
        .map_err(|_| "POS_SERVER_DATABASE_URL must point to a dedicated POS database")?;
    if url.is_empty() {
        return Err("POS_SERVER_DATABASE_URL cannot be empty".into());
    }

    // Only local operators who can access the POS server's database URL are
    // authorized for these trusted actions. Opening applies POS migrations.
    let store = PostgresStore::open(&url).await?;
    match action {
        AdminAction::CreateUser {
            email,
            password_stdin,
        } => {
            let password = read_password(password_stdin)?;
            let (user_id, workspace) = store
                .provision_user_with_personal_workspace(&email, &password)
                .await?;
            println!("Created POS user: {user_id}");
            println!("Personal workspace: {workspace}");
        }
        AdminAction::RevokeDevice { user, device } => {
            if store.revoke_device(user, device).await? {
                println!("Revoked device {device} for user {user}");
            } else {
                return Err("no matching active device for that user".into());
            }
        }
        AdminAction::DisableUser { user, yes } => {
            debug_assert!(yes);
            if store.disable_user(user).await? {
                println!("Disabled user {user}; all sessions are now invalid");
            } else {
                return Err("user not found or already disabled".into());
            }
        }
    }
    store.close().await;
    Ok(())
}
