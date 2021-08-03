use hyper::service::{make_service_fn, service_fn};
use hyper::{Body, Client, Request, Response, Server};
use hyper_tls::HttpsConnector;
use std::net::SocketAddr;
use std::sync::Arc;

const STRIPPED: [&str; 6] = [
    "content-length",
    "transfer-encoding",
    "accept-encoding",
    "content-encoding",
    "host",
    "connection",
];

#[derive(Debug)]
enum ReverseProxyError {
    Hyper(hyper::Error),
    HyperHttp(hyper::http::Error),
}

impl From<hyper::Error> for ReverseProxyError {
    fn from(e: hyper::Error) -> Self {
        ReverseProxyError::Hyper(e)
    }
}

impl From<hyper::http::Error> for ReverseProxyError {
    fn from(e: hyper::http::Error) -> Self {
        ReverseProxyError::HyperHttp(e)
    }
}

impl std::fmt::Display for ReverseProxyError {
    fn fmt(&self, fmt: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(fmt, "{:?}", self)
    }
}

impl std::error::Error for ReverseProxyError {}

struct ReverseProxy {
    scheme: String,
    host: String,
    client: Client<HttpsConnector<hyper::client::HttpConnector>>,
}

impl ReverseProxy {
    async fn handle(
        self: Arc<Self>,
        mut req: Request<Body>,
    ) -> Result<Response<Body>, ReverseProxyError> {
        let h = req.headers_mut();
        for key in &STRIPPED {
            h.remove(*key);
        }
        let mut builder = hyper::Uri::builder()
            .scheme(&*self.scheme)
            .authority(&*self.host);
        if let Some(pq) = req.uri().path_and_query() {
            builder = builder.path_and_query(pq.clone());
        }
        *req.uri_mut() = builder.build()?;

        log::debug!("request == {:?}", req);

        let response = self.client.request(req).await?;
        log::debug!("response == {:?}", response);

        Ok(response)
    }
}

#[tokio::main]
async fn main() {
    env_logger::Builder::from_env(env_logger::Env::default().default_filter_or("info")).init();
    let addr = SocketAddr::from(([127, 0, 0, 1], 3000));

    let https = HttpsConnector::new();
    let client = Client::builder().build(https);

    let rp = Arc::new(ReverseProxy {
        client,
        scheme: "https".to_owned(),
        host: "www.fpcomplete.com".to_owned(),
    });

    let make_svc = make_service_fn(move |_conn| {
        let rp = Arc::clone(&rp);
        async move {
            Ok::<_, ReverseProxyError>(service_fn(move |req| {
                let rp = Arc::clone(&rp);
                rp.handle(req)
            }))
        }
    });

    let server = Server::bind(&addr).serve(make_svc);
    log::info!("Server started, bound on {}", addr);

    if let Err(e) = server.await {
        log::error!("server error: {}", e);
        std::process::abort();
    }
}
