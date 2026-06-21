use crate::constant;

type Result<T> = std::result::Result<T, Error>;

#[derive(Debug, thiserror::Error)]
pub enum Error {
    #[error("Failed to encrypt payload")]
    FailedToEncrypt,
    #[error("Canceled")]
    Canceled,
    #[error("Failed to manipulate the terminal: {0}")]
    Terminal(rucline::Error),
    #[error("Invalid password: {0}")]
    InvalidPassword(std::io::Error),
    #[error("Password is not a 32byte base64 encoded key")]
    InvalidKey,
    #[error("Failed to open the keyring: {0}")]
    KeyringOpen(keyring::Error),
    #[error("Failed to get username")]
    User(std::env::VarError),
}

impl crate::PostAction for Error {}

impl crate::Cancelable for Error {
    fn canceled(&self) -> bool {
        matches!(self, Self::Canceled)
    }
}

#[derive(Debug)]
pub struct Secret {
    ring: keyring::Entry,
    source: Source,
    credentials: Credentials,
    cookie: String,
}

impl Secret {
    pub fn new() -> Result<Self> {
        let ring = keyring::Entry::new(
            constant::APP_NAME,
            std::env::var(constant::ENV_SYSTEM_USER)
                .map_err(Error::User)?
                .as_str(),
        )
        .map_err(Error::KeyringOpen)?;

        let (source, credentials) = fetch_credentials(&ring)?;
        let cookie = create_cookie(&credentials)?;

        Ok(Self {
            ring,
            source,
            credentials,
            cookie,
        })
    }

    pub fn cookie(&self) -> &str {
        &self.cookie
    }

    pub fn finalize(&self, unauthorized: bool) {
        if unauthorized {
            if self.source == Source::Keyring {
                prompt_delete_credentials(&self.ring);
            }
        } else if self.source != Source::Keyring {
            prompt_save_credentials(&self.ring, &self.credentials);
        }
    }
}

fn fetch_credentials(ring: &keyring::Entry) -> Result<(Source, Credentials)> {
    if let Some(credentials) = get_credentials_from_env() {
        return Ok((Source::Env, credentials));
    }

    match ring.get_password() {
        Ok(secret) => {
            if let Some(credentials) = Credentials::from_keyring(&secret) {
                return Ok((Source::Keyring, credentials));
            }
            eprintln!("Error while reading the keyring");
            prompt_delete_credentials(ring);
        }
        Err(keyring::Error::NoEntry) => {}
        Err(err) => {
            eprintln!("Error while reading the keyring: {err}");
            prompt_delete_credentials(ring);
        }
    }

    get_credentials_from_stdin().map(|credentials| (Source::Stdin, credentials))
}

fn create_cookie(credentials: &Credentials) -> Result<String> {
    let token = endgame::types::Token {
        timestamp: endgame::types::Timestamp::now() + std::time::Duration::from_hours(1),
        email: credentials.user.clone(),
        given_name: None,
        family_name: None,
        picture: None,
    };
    endgame::dencrypt::encrypt(credentials.key, &token)
        .map(|t| format!("{}={t}", constant::ENDGAME_COOKIE))
        .ok_or(Error::FailedToEncrypt)
}

fn get_credentials_from_env() -> Option<Credentials> {
    if let Ok(user) = std::env::var(constant::ENV_USER) {
        if let Ok(password) = std::env::var(constant::ENV_PASSWORD) {
            match Credentials::new(user, &password) {
                Ok(credentials) => Some(credentials),
                Err(err) => {
                    eprintln!("Ignoring env credentials: {err}");
                    None
                }
            }
        } else {
            eprintln!(
                "{} defined but missing {}. Ignoring",
                constant::ENV_USER,
                constant::ENV_PASSWORD,
            );
            None
        }
    } else if std::env::var(constant::ENV_PASSWORD).is_ok() {
        eprintln!(
            "{} defined but missing {}. Ignoring",
            constant::ENV_PASSWORD,
            constant::ENV_USER,
        );
        None
    } else {
        None
    }
}

fn get_credentials_from_stdin() -> Result<Credentials> {
    use rucline::prompt::Builder;

    let user = rucline::prompt::Prompt::from("User: ")
        .read_line()
        .map_err(Error::Terminal)?
        .some()
        .ok_or(Error::Canceled)?;
    let password = rpassword::prompt_password("Password (base64 encoded): ")
        .map_err(Error::InvalidPassword)?;

    Credentials::new(user, &password)
}

#[derive(Debug, Copy, Clone, Eq, PartialEq)]
enum Source {
    Keyring,
    Env,
    Stdin,
}

#[derive(Debug)]
struct Credentials {
    user: String,
    key: [u8; 32],
}

impl Credentials {
    fn new(user: String, password: &str) -> Result<Self> {
        let key = base64::Engine::decode(&base64::engine::general_purpose::STANDARD, password)
            .map_err(|_| Error::InvalidKey)?
            .try_into()
            .map_err(|_| Error::InvalidKey)?;

        Ok(Self { user, key })
    }

    fn from_keyring(string: &str) -> Option<Self> {
        string
            .split_once('\n')
            .and_then(|(u, p)| Self::new(String::from(u), p).ok())
    }

    fn to_keyring(&self) -> String {
        format!(
            "{}\n{}",
            self.user,
            base64::Engine::encode(&base64::engine::general_purpose::STANDARD, self.key)
        )
    }
}

fn prompt_save_credentials(ring: &keyring::Entry, credentials: &Credentials) {
    if prompt_confirmation("Store credentials in keyring? ")
        && let Err(err) = ring.set_password(&credentials.to_keyring())
    {
        eprintln!("Could not save credentials: {err}");
    }
}

fn prompt_delete_credentials(ring: &keyring::Entry) {
    if prompt_confirmation("Clear keyring? ")
        && let Err(err) = ring.delete_credential()
    {
        eprintln!("Could not delete password: {err}");
    }
}

fn prompt_confirmation(prompt: &str) -> bool {
    use rucline::{crossterm::style::Colorize, prompt::Builder};

    static OPTIONS: [&str; 2] = ["yes", "no"];

    rucline::prompt::Prompt::from(prompt.dark_green())
        .suggester(&OPTIONS[..])
        .completer(&OPTIONS[..])
        .read_line()
        .ok()
        .and_then(rucline::Outcome::some)
        .map(|choice| choice.trim().to_lowercase())
        .is_some_and(|choice| matches!(choice.as_str(), "y" | "yes"))
}
