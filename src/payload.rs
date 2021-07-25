// The request payload.

use std::{borrow::Cow, str::FromStr};

use anyhow::Context;
use hyper::{Body, HeaderMap, Method, Request};
use serde::{
    de::{self, Error},
    ser::SerializeMap,
    Deserialize, Serialize,
};

pub struct HeaderMapWrapper(pub HeaderMap);
impl From<HeaderMap> for HeaderMapWrapper {
    fn from(m: HeaderMap) -> Self {
        Self(m)
    }
}
impl Serialize for HeaderMapWrapper {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: serde::Serializer,
    {
        let mut map = serializer.serialize_map(Some(self.0.len()))?;
        for (name, value) in &self.0 {
            let value_bytes = value.as_bytes();
            match std::str::from_utf8(value_bytes) {
                Ok(value_str) => map.serialize_entry(name.as_str(), value_str)?,
                Err(_) => map.serialize_entry(name.as_str(), value_bytes)?,
            }
        }
        map.end()
    }
}

#[derive(Eq, Clone)]
pub struct MethodWrapper(pub Method);
impl From<Method> for MethodWrapper {
    fn from(m: Method) -> Self {
        Self(m)
    }
}
impl<T: PartialEq<Method>> PartialEq<T> for MethodWrapper {
    fn eq(&self, other: &T) -> bool {
        other == &self.0
    }
}

impl Serialize for MethodWrapper {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: serde::Serializer,
    {
        serializer.serialize_str(self.0.as_str())
    }
}

impl<'de> Deserialize<'de> for MethodWrapper {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: serde::Deserializer<'de>,
    {
        let s: Cow<str> = Deserialize::deserialize(deserializer)?;
        match Method::from_str(&s) {
            Ok(method) => Ok(MethodWrapper(method)),
            Err(_) => Err(D::Error::invalid_value(
                de::Unexpected::Str("x"),
                &"Not a valid HTTP verb",
            )),
        }
    }
}

#[derive(Serialize)]
#[serde(untagged)]
pub enum BytesOrString {
    Bytes(Vec<u8>),
    Str(String),
}
impl BytesOrString {
    pub fn as_bytes(&self) -> &[u8] {
        match self {
            BytesOrString::Bytes(b) => &b,
            BytesOrString::Str(s) => s.as_bytes(),
        }
    }
}

#[derive(Serialize)]
pub struct Payload {
    pub method: MethodWrapper,
    pub path: String,
    pub headers: HeaderMapWrapper,
    pub body: Option<BytesOrString>,
}

pub async fn decode_payload(request: Request<Body>) -> Result<Payload, anyhow::Error> {
    let (req_info, body) = request.into_parts();
    let body_slice: &[u8] = &hyper::body::to_bytes(body)
        .await
        .context("Could not get HTTP body")?;
    let body_vec = body_slice.to_vec();
    Ok(Payload {
        method: req_info.method.into(),
        path: req_info.uri.path().to_owned(),
        headers: req_info.headers.into(),
        body: Some(match String::from_utf8(body_vec) {
            Ok(s) => BytesOrString::Str(s),
            Err(e) => BytesOrString::Bytes(e.into_bytes()),
        }),
    })
}
