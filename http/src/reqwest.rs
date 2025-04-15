use std::{collections::BTreeMap, num::NonZeroU16};

use async_trait::async_trait;
use bytes::Bytes;

use crate::{
    Error, GenericClient, GenericRequestBuilder, GenericResponse, Method, RequestBuilder, Response,
    StatusCode,
};

impl From<crate::Method> for reqwest::Method {
    fn from(value: crate::Method) -> Self {
        match value {
            crate::Method::Get => Self::GET,
            crate::Method::Post => Self::POST,
            crate::Method::Put => Self::PUT,
            crate::Method::Patch => Self::PATCH,
            crate::Method::Delete => Self::DELETE,
            crate::Method::Head => Self::HEAD,
            crate::Method::Options => Self::OPTIONS,
            crate::Method::Connect => Self::CONNECT,
            crate::Method::Trace => Self::TRACE,
        }
    }
}

pub struct ReqwestClient(reqwest::Client);

impl ReqwestClient {
    #[must_use]
    pub const fn new(client: reqwest::Client) -> Self {
        Self(client)
    }
}

impl GenericClient for ReqwestClient {
    fn request(&self, method: Method, url: &str) -> RequestBuilder {
        RequestBuilder {
            builder: Box::new(ReqwestRequestBuilder(Some(
                self.0.request(method.into(), url),
            ))),
        }
    }
}

pub struct ReqwestRequestBuilder(Option<reqwest::RequestBuilder>);

#[async_trait]
impl GenericRequestBuilder for ReqwestRequestBuilder {
    fn header(&mut self, name: &str, value: &str) {
        let builder = self.0.take().unwrap();
        self.0 = Some(builder.header(name, value));
    }

    fn body(&mut self, body: Bytes) {
        let builder = self.0.take().unwrap();
        self.0 = Some(builder.body(body));
    }

    fn form(&mut self, form: &serde_json::Value) {
        let builder = self.0.take().unwrap();
        self.0 = Some(builder.form(form));
    }

    async fn send(&mut self) -> Result<Response, Error> {
        let builder = self.0.take().unwrap();
        Ok(Response {
            inner: Box::new(ReqwestResponse {
                headers: None,
                inner: Some(builder.send().await?),
            }),
        })
    }
}

pub struct ReqwestResponse {
    headers: Option<BTreeMap<String, String>>,
    inner: Option<reqwest::Response>,
}

#[async_trait]
impl GenericResponse for ReqwestResponse {
    #[must_use]
    fn status(&self) -> StatusCode {
        self.inner.as_ref().unwrap().status().into()
    }

    #[must_use]
    fn headers(&mut self) -> &BTreeMap<String, String> {
        if self.headers.is_none() {
            self.headers = Some(headers_to_btree(self.inner.as_ref().unwrap().headers()));
        }

        self.headers.as_ref().unwrap()
    }

    #[must_use]
    async fn text(&mut self) -> Result<String, Error> {
        let response = self.inner.take().unwrap();
        Ok(response.text().await?)
    }

    #[must_use]
    async fn bytes(&mut self) -> Result<Bytes, Error> {
        let response = self.inner.take().unwrap();
        Ok(response.bytes().await?)
    }

    #[must_use]
    fn bytes_stream(
        &mut self,
    ) -> std::pin::Pin<Box<dyn futures_core::Stream<Item = Result<Bytes, Error>> + Send>> {
        use futures_util::TryStreamExt as _;

        let response = self.inner.take().unwrap();
        Box::pin(response.bytes_stream().map_err(Into::into))
    }
}

#[allow(clippy::fallible_impl_from)]
impl From<reqwest::StatusCode> for StatusCode {
    fn from(value: reqwest::StatusCode) -> Self {
        Self(NonZeroU16::new(value.as_u16()).unwrap())
    }
}

fn headers_to_btree(value: &reqwest::header::HeaderMap) -> BTreeMap<String, String> {
    let mut headers = BTreeMap::new();

    for (key, value) in value {
        if let Ok(value) = value.to_str() {
            headers.insert(key.to_string(), value.to_string());
        }
    }

    headers
}
