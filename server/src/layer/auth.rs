use super::Auth;

impl<S, I> tower_layer::Layer<I> for Auth<S>
where
    S: Clone,
{
    type Service = Middleware<S, I>;

    fn layer(&self, inner: I) -> Self::Service {
        Middleware {
            inner,
            auth: self.clone(),
        }
    }
}

#[derive(Debug, Clone)]
pub struct Middleware<S, I> {
    inner: I,
    auth: Auth<S>,
}

impl<B, S, I> tower_service::Service<hyper::Request<B>> for Middleware<S, I>
where
    S: Clone + Send + Sync + 'static,
    I: tower_service::Service<hyper::Request<B>, Response = axum::response::Response>,
    I::Error: std::fmt::Display,
    I::Future: Unpin,
{
    type Response = I::Response;
    type Error = I::Error;
    type Future = Future<I::Future>;

    fn poll_ready(
        &mut self,
        cx: &mut std::task::Context<'_>,
    ) -> std::task::Poll<Result<(), Self::Error>> {
        self.inner.poll_ready(cx)
    }

    fn call(&mut self, mut request: hyper::Request<B>) -> Self::Future {
        if let Some((user, session)) = pre_auth(&request, &self.auth.services) {
            let span = tracing::span!(target: "layer", tracing::Level::INFO, "auth", %user);
            request.extensions_mut().insert(session);
            Future::Pass(self.inner.call(request), span)
        } else {
            Future::Forbidden
        }
    }
}

#[tracing::instrument(target = "layer", skip_all)]
fn pre_auth<'r, 's, B, S>(
    request: &'r hyper::Request<B>,
    services: &'s std::collections::HashMap<String, S>,
) -> Option<(&'r str, S)>
where
    's: 'r,
    S: Clone,
{
    #[cfg(feature = "dev")]
    {
        let user = services.keys().next()?;
        services.get(user).map(|s| (user.as_str(), s.clone()))
    }

    #[cfg(not(feature = "dev"))]
    {
        let header = super::X_EMAIL;

        let Some(user_header) = request.headers().get(&header) else {
            tracing::warn!(%header, "Header is missing");
            return None;
        };

        let user = match user_header.to_str() {
            Ok(user) => user,
            Err(error) => {
                tracing::warn!(%header, %error, "Header is not parseable as a String");
                return None;
            }
        };

        services.get(user).map(|s| (user, s.clone()))
    }
}

pub enum Future<F> {
    Forbidden,
    Pass(F, tracing::Span),
}

impl<F, E> std::future::Future for Future<F>
where
    F: std::future::Future<Output = Result<axum::response::Response, E>> + Unpin,
    E: std::fmt::Display,
{
    type Output = F::Output;

    fn poll(
        self: std::pin::Pin<&mut Self>,
        cx: &mut std::task::Context<'_>,
    ) -> std::task::Poll<Self::Output> {
        match self.get_mut() {
            Self::Forbidden => {
                let response =
                    axum::response::IntoResponse::into_response(hyper::StatusCode::FORBIDDEN);
                std::task::Poll::Ready(Ok(response))
            }
            Self::Pass(f, span) => {
                let _span = span.enter();
                std::pin::Pin::new(f).poll(cx)
            }
        }
    }
}
