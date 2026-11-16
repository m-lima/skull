#![deny(warnings, clippy::pedantic, clippy::all, rust_2018_idioms)]

#[derive(Debug, thiserror::Error)]
enum Error {
    #[error("Path does not exist")]
    PathDoesNotExist,
    #[error("Path is not a directory")]
    PathNotDir,
    #[error("Could not open file")]
    CouldNotOpenFile,
    #[error("Invalid port")]
    InvalidPort,
}

pub fn parse() -> Args {
    <Inner as clap::Parser>::parse().into()
}

#[derive(Debug)]
pub struct Args {
    pub verbosity: Verbosity,
    pub socket: Socket,
    #[cfg(feature = "threads")]
    pub threads: boile_rs::rt::Threads,
    pub create: bool,
    pub users: std::collections::HashSet<String>,
    pub db: std::path::PathBuf,
}

impl From<Inner> for Args {
    fn from(value: Inner) -> Self {
        Self {
            verbosity: value.verbosity(),
            socket: value.socket,
            #[cfg(feature = "threads")]
            threads: value.threads,
            create: value.create,
            users: value.users.users(),
            db: value.db,
        }
    }
}

#[derive(Debug, clap::Parser)]
pub struct Inner {
    /// Verbosity level
    #[arg(short, action = clap::ArgAction::Count)]
    verbosity: u8,

    /// Location to serve on either a port for serving on TCP, or a path to a Unix Domain Socket.
    ///
    /// If the argument starts with `unix:`, it will be interpreted as a path.
    /// Otherwise, it will be interpreted as a port
    #[arg(short, long, default_value_t = Socket::Port(80), value_parser = parse_socket)]
    socket: Socket,

    /// Number of threads
    #[arg(short, long, default_value = "auto", value_parser = boile_rs::rt::threads::parse)]
    #[cfg(feature = "threads")]
    threads: boile_rs::rt::Threads,

    /// Create the databases if they don't exist
    #[arg(short, long)]
    create: bool,

    #[command(flatten)]
    users: Users,

    /// Path to databases directory
    #[arg(value_parser = parse_db)]
    db: std::path::PathBuf,
}

impl Inner {
    fn verbosity(&self) -> Verbosity {
        let (level, include_spans) = match self.verbosity {
            0 => (tracing::Level::ERROR, false),
            1 => (tracing::Level::WARN, false),
            2 => (tracing::Level::INFO, false),
            3 => (tracing::Level::INFO, true),
            4 => (tracing::Level::DEBUG, true),
            _ => (tracing::Level::TRACE, true),
        };

        Verbosity {
            level,
            include_spans,
        }
    }
}

#[derive(Debug, Copy, Clone)]
pub struct Verbosity {
    pub level: tracing::Level,
    pub include_spans: bool,
}

#[derive(Debug, Clone, clap::Args)]
#[group(required = true)]
struct Users {
    /// Initialize with the given users present
    #[arg(short = 'U', long, value_delimiter = ',')]
    add_user: Vec<String>,

    /// Path to the file with the list of users with which to initialize
    ///
    /// File should list one user per line
    /// Lines are trimmed and ignored if empty or starting with `#`
    #[arg(short, long, value_parser = clap::builder::TypedValueParser::try_map(clap::builder::PathBufValueParser::new(), to_user_list))]
    users: Option<std::collections::HashSet<String>>,
}

impl Users {
    fn users(self) -> std::collections::HashSet<String> {
        let mut users = self.users.unwrap_or_default();
        users.extend(self.add_user);
        users
    }
}

#[derive(Debug, Clone)]
pub enum Socket {
    Port(u16),
    Unix(std::path::PathBuf),
}

impl std::fmt::Display for Socket {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Socket::Port(port) => port.fmt(f),
            Socket::Unix(path) => path.display().fmt(f),
        }
    }
}

fn parse_socket(input: &str) -> Result<Socket, Error> {
    if let Some(path) = input.strip_prefix("unix:") {
        Ok(Socket::Unix(std::path::PathBuf::from(path)))
    } else {
        match input.parse() {
            Ok(port @ 1..) => Ok(Socket::Port(port)),
            _ => Err(Error::InvalidPort),
        }
    }
}

fn parse_db(input: &str) -> Result<std::path::PathBuf, Error> {
    let input = input.strip_prefix("sqlite://").unwrap_or(input);
    let path = std::path::PathBuf::from(input);

    if !path.exists() {
        Err(Error::PathDoesNotExist)
    } else if !path.is_dir() {
        Err(Error::PathNotDir)
    } else {
        Ok(path)
    }
}

fn to_user_list(path: std::path::PathBuf) -> Result<std::collections::HashSet<String>, Error> {
    let content = std::fs::read_to_string(path).map_err(|_| Error::CouldNotOpenFile)?;
    let mut users = std::collections::HashSet::new();

    for line in content.lines() {
        let line = line.trim();
        if line.starts_with('#') {
            continue;
        }
        users.insert(String::from(line));
    }

    Ok(users)
}
