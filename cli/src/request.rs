use crate::{constant, secret};

type Result<T> = std::result::Result<T, Error>;

#[derive(Debug, thiserror::Error)]
pub enum Error {
    #[error("Canceled")]
    Canceled,
    #[error("Failed to build client")]
    Token,
    #[error("Failed to manipulate the terminal: {0}")]
    Terminal(rucline::Error),
    #[error("Failed to parse host URL: {0}")]
    Url(url::ParseError),
    #[error("Failed to build client: {0}")]
    ReqwestBuild(reqwest::Error),
    #[error("Unauthorized")]
    Unauthorized,
    #[error("Failed to send request: {0}")]
    ReqwestSend(reqwest::Error),
    #[error("Failed to deserialize response: {0}")]
    ReqwestJson(reqwest::Error),
    #[error("Server responded with an error: {0}")]
    Server(reqwest::StatusCode, Option<types::Error>),
    #[error("Unexpected response for request {0} {1}")]
    UnexpectedResponse(reqwest::Method, &'static str, types::Payload),
}

impl crate::PostAction for Error {
    fn post(self) {
        match self {
            Self::Server(_, Some(error)) => {
                let kind = error.kind;
                if let Some(message) = error.message {
                    eprintln!("{kind}: {message}");
                } else {
                    eprintln!("{kind}");
                }
            }
            Self::UnexpectedResponse(_, _, response) => {
                if let Ok(response) = serde_json::to_string(&response) {
                    eprintln!("{response}");
                } else {
                    eprintln!("{response:#?}");
                }
            }
            _ => {}
        }
    }
}

impl crate::Cancelable for Error {
    fn canceled(&self) -> bool {
        matches!(self, Self::Canceled)
    }
}

pub struct Request {
    // TODO: Implement save and delete for the host file
    host: reqwest::Url,
    client: reqwest::Client,
    secret: secret::Secret,
    unauthorized: std::sync::atomic::AtomicBool,
}

impl Request {
    pub fn new(secret: secret::Secret) -> Result<Self> {
        let host = get_host()?;
        let mut headers = reqwest::header::HeaderMap::new();
        headers.insert(
            reqwest::header::COOKIE,
            reqwest::header::HeaderValue::from_str(secret.cookie()).map_err(|_| Error::Token)?,
        );

        let client = reqwest::Client::builder()
            .redirect(reqwest::redirect::Policy::none())
            .default_headers(headers)
            .build()
            .map_err(Error::ReqwestBuild)?;

        let unauthorized = std::sync::atomic::AtomicBool::new(false);

        Ok(Self {
            host,
            client,
            secret,
            unauthorized,
        })
    }

    pub async fn update(&self) -> Result<()> {
        let (skulls, quicks) = tokio::join!(
            self.get_resource_no_cache::<types::Skull>(None),
            self.get_resource_no_cache::<types::Quick>(None),
        );

        cache::save(&skulls?);
        cache::save(&quicks?);
        Ok(())
    }

    pub async fn get_cacheable<C>(&self) -> Result<Vec<C>>
    where
        C: sealed::Cacheable,
    {
        if let Some(resource) = cache::read() {
            return Ok(resource);
        }

        let response = self.get_resource_no_cache::<C>(None).await?;
        cache::save(&response);
        Ok(response)
    }

    pub async fn get_occurrences<Search>(&self, search: Search) -> Result<Vec<types::Occurrence>>
    where
        Search: Into<Option<types::request::occurrence::Search>>,
    {
        let search = search.into();
        self.get_resource_no_cache(
            search
                .as_ref()
                .map(types::request::occurrence::Search::to_query),
        )
        .await
    }

    pub async fn post_occurrences(
        &self,
        occurrences: Vec<types::request::occurrence::Item>,
    ) -> Result<()> {
        let url = self.url_for::<types::Occurrence>()?;

        let body = types::request::occurrence::Create { items: occurrences };
        let request = self.client.post(url).json(&body);
        let response = self.send(request).await?;
        match response {
            types::Payload::Change(types::Change::Created) => Ok(()),
            payload => Err(Error::UnexpectedResponse(
                reqwest::Method::POST,
                "occurrence",
                payload,
            )),
        }
    }
}

impl Request {
    fn url_for<R>(&self) -> Result<reqwest::Url>
    where
        R: sealed::Resource,
    {
        self.host.join(R::PATH).map_err(Error::Url)
    }

    async fn get_resource_no_cache<R>(&self, query: Option<String>) -> Result<Vec<R>>
    where
        R: sealed::Resource,
    {
        let mut url = self.url_for::<R>()?;
        if let Some(query) = query {
            url.set_query(Some(query.as_ref()));
        }
        let request = self.client.get(url);

        let response = self.send(request).await?;
        R::extract(response)
            .map_err(|r| Error::UnexpectedResponse(reqwest::Method::GET, R::PATH, r))
    }

    async fn send(&self, request: reqwest::RequestBuilder) -> Result<types::Payload> {
        if self.unauthorized.load(std::sync::atomic::Ordering::Relaxed) {
            return Err(Error::Unauthorized);
        }

        let response = request.send().await.map_err(Error::ReqwestSend)?;

        let status = response.status();

        if unauthorized(status) {
            self.unauthorized
                .store(true, std::sync::atomic::Ordering::Relaxed);
            return Err(Error::Unauthorized);
        }

        let body = response.json::<types::Response>().await;

        if status.is_success() {
            match body.map_err(Error::ReqwestJson)? {
                types::Response::Error(error) => Err(Error::Server(status, Some(error))),
                types::Response::Payload(payload) => Ok(payload),
            }
        } else {
            let error = body.ok().and_then(|response| match response {
                types::Response::Error(error) => Some(error),
                types::Response::Payload(_) => None,
            });
            Err(Error::Server(status, error))
        }
    }
}

impl crate::PostAction for Request {
    fn post(self) {
        self.secret
            .finalize(self.unauthorized.load(std::sync::atomic::Ordering::Relaxed));
    }
}

mod sealed {
    pub trait Resource: serde::ser::Serialize + serde::de::DeserializeOwned {
        type Query: serde::Serialize;
        const PATH: &'static str;
        fn extract(payload: types::Payload) -> Result<Vec<Self>, types::Payload>;
    }

    impl Resource for types::Skull {
        type Query = ();

        const PATH: &'static str = "skull";

        fn extract(payload: types::Payload) -> Result<Vec<Self>, types::Payload> {
            match payload {
                types::Payload::Skulls(skulls) => Ok(skulls),
                payload => Err(payload),
            }
        }
    }

    impl Resource for types::Quick {
        type Query = ();

        const PATH: &'static str = "occurrence/quick";

        fn extract(payload: types::Payload) -> Result<Vec<Self>, types::Payload> {
            match payload {
                types::Payload::Quicks(quicks) => Ok(quicks),
                payload => Err(payload),
            }
        }
    }

    impl Resource for types::Occurrence {
        type Query = types::request::occurrence::Search;

        const PATH: &'static str = "occurrence";

        fn extract(payload: types::Payload) -> Result<Vec<Self>, types::Payload> {
            match payload {
                types::Payload::Occurrences(occurrences) => Ok(occurrences),
                payload => Err(payload),
            }
        }
    }

    pub trait Cacheable: Resource {}

    impl Cacheable for types::Skull {}
    impl Cacheable for types::Quick {}
}

fn get_host() -> Result<reqwest::Url> {
    fn get_host_string() -> Result<(String, bool)> {
        use rucline::prompt::Builder;

        if let Ok(host) = std::env::var(constant::ENV_HOST) {
            return Ok((host, true));
        }

        if let Some(host) = constant::path::host().and_then(|p| std::fs::read_to_string(p).ok()) {
            return Ok((host, false));
        }

        rucline::prompt::Prompt::from("Host: ")
            .read_line()
            .map_err(Error::Terminal)?
            .some()
            .ok_or(Error::Canceled)
            .map(|host| (host, true))
    }

    let (mut host, mut should_write) = get_host_string()?;
    if !host.ends_with('/') {
        host.push('/');
        should_write = true;
    }

    if should_write
        && let Some(path) = constant::path::host()
        && let Some(parent) = path.parent()
    {
        if let Err(err) = std::fs::create_dir_all(parent) {
            eprintln!("Failed to create host path: {err}");
        } else if let Err(err) = std::fs::write(path, &host) {
            eprintln!("Failed to write host file: {err}");
        }
    }

    reqwest::Url::parse(&host).map_err(Error::Url)
}

fn unauthorized(status: reqwest::StatusCode) -> bool {
    status == reqwest::StatusCode::UNAUTHORIZED
        || status == reqwest::StatusCode::FORBIDDEN
        || status == reqwest::StatusCode::FOUND
}

mod cache {
    use crate::constant;

    pub fn read<R>() -> Option<Vec<R>>
    where
        R: super::sealed::Resource,
    {
        let path = constant::path::cache().map(|p| p.join(R::PATH))?;
        let bytes = std::fs::read(path).ok()?;
        serde_json::from_slice(bytes.as_slice()).ok()
    }

    pub fn save<R>(data: &[R]) -> Option<()>
    where
        R: super::sealed::Resource,
    {
        let path = constant::path::cache()?;
        if !path.exists() {
            std::fs::create_dir(path).ok()?;
        }
        let path = path.join(R::PATH);

        let bytes = serde_json::to_vec(data).ok()?;
        std::fs::write(path, bytes).ok()
    }
}
