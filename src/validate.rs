use crate::{
    config::{self, Catcher},
    payload::Payload,
};

use anyhow::Context;
use hyper::HeaderMap;
use ring::hmac;
use std::{env, process::Stdio};
use tokio::{io::AsyncWriteExt, process::Command};

fn validate_github(headers: &HeaderMap, body: &[u8], secret: &[u8]) -> bool {
    let sent_hash_hex = match headers.get("x-hub-signature-256") {
        // Skip the initial `sha256=`.
        Some(hash) => &hash.as_bytes()["sha256=".len()..],
        // GitHub may not be the source of this request.
        None => return false,
    };
    let sent_hash = match hex::decode(sent_hash_hex) {
        Ok(raw) => raw,
        Err(_) => return false,
    };
    let key = hmac::Key::new(hmac::HMAC_SHA256, secret);
    hmac::verify(&key, body, &sent_hash).is_ok()
}

pub async fn validate(catcher: &Catcher, req_payload: &Payload) -> Result<bool, anyhow::Error> {
    match &catcher.validate {
        config::Validate::Dont => Ok(true),
        config::Validate::Command(cmd) => {
            // XXX maybe make crossplatform later on?
            let req_payload_str = serde_json::to_string(req_payload)
                .context("Could not serialize request payload")?;
            let mut validator = Command::new("/bin/sh")
                .arg("-c")
                .arg(cmd)
                .stdin(Stdio::piped())
                .spawn()
                .context("Could not execute validation process")?;
            {
                let mut stdin = validator
                    .stdin
                    .take()
                    .expect("Could not take standard input of validation process");
                stdin
                    .write_all(req_payload_str.as_bytes())
                    .await
                    .context("Could not write request payload")?;
                // Close standard input
                drop(stdin);
            }
            let status = validator
                .wait()
                .await
                .context("Could not wait for validator to exit")?;
            Ok(status.code() == Some(0))
        }
        config::Validate::GitHub(keyspec) => {
            if let Some(body) = &req_payload.body {
                let mut keyspec_chars = keyspec.chars();
                if keyspec_chars.next() == Some('$') {
                    // It's an environment variable.
                    let key_var = keyspec_chars.as_str();
                    let key = env::var(key_var).map_err(|_| {
                        anyhow::Error::msg(format!(
                            "Could not resolve environment variable {} or was not valid UTF-8",
                            key_var
                        ))
                    })?;
                    Ok(validate_github(
                        &req_payload.headers.0,
                        body.as_bytes(),
                        key.as_bytes(),
                    ))
                } else {
                    Ok(validate_github(
                        &req_payload.headers.0,
                        body.as_bytes(),
                        keyspec.as_bytes(),
                    ))
                }
            } else {
                // GitHub webhooks are always POST requests.
                Ok(false)
            }
        }
    }
}
