use std::{collections::BTreeMap, num::NonZeroU16};

use async_trait::async_trait;
use bytes::Bytes;

use crate::{
    Error, GenericClient, GenericRequestBuilder, GenericResponse, Method, RequestBuilder, Response,
    StatusCode,
};

#[derive(Default)]
pub struct SimulatorClient;

impl SimulatorClient {
    #[must_use]
    pub const fn new() -> Self {
        Self
    }
}

impl GenericClient for SimulatorClient {
    fn request(&self, _method: Method, _url: &str) -> RequestBuilder {
        RequestBuilder {
            builder: Box::new(SimulatorRequestBuilder),
        }
    }
}

pub struct SimulatorRequestBuilder;

#[async_trait]
impl GenericRequestBuilder for SimulatorRequestBuilder {
    fn header(&mut self, _name: &str, _value: &str) {}

    fn body(&mut self, _body: Bytes) {}

    fn form(&mut self, _form: &serde_json::Value) {}

    async fn send(&mut self) -> Result<Response, Error> {
        Ok(Response {
            inner: Box::new(SimulatorResponse::default()),
        })
    }
}

#[derive(Default)]
pub struct SimulatorResponse {
    headers: BTreeMap<String, String>,
}

#[async_trait]
impl GenericResponse for SimulatorResponse {
    #[must_use]
    fn status(&self) -> StatusCode {
        StatusCode(NonZeroU16::new(200).unwrap())
    }

    #[must_use]
    fn headers(&mut self) -> &BTreeMap<String, String> {
        &self.headers
    }

    #[must_use]
    async fn text(&mut self) -> Result<String, Error> {
        Ok(String::new())
    }

    #[must_use]
    async fn bytes(&mut self) -> Result<Bytes, Error> {
        Ok(Bytes::new())
    }

    #[must_use]
    fn bytes_stream(
        &mut self,
    ) -> std::pin::Pin<Box<dyn futures_core::Stream<Item = Result<Bytes, Error>> + Send>> {
        Box::pin(futures_util::stream::empty())
    }
}
