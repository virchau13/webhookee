mod config;
mod payload;
mod validate;

use anyhow::Context;
use config::Catcher;
use log::{error, info};

use hyper::service::{make_service_fn, service_fn};
use hyper::{Body, Request, Response, Server, StatusCode};
use std::convert::Infallible;
use std::net::SocketAddr;
use std::path::PathBuf;
use std::process::Stdio;
use structopt::StructOpt;
use tokio::io::AsyncWriteExt;
use tokio::process::Command;

pub const PROJ_NAME: &str = "webhookee";

enum CatcherReturn {
    // The webhook was allowed.
    Allowed(Vec<u8> /* the body */),
    // It was denied
    Denied,
}

async fn invoke_catcher(
    catcher: &Catcher,
    request: Request<Body>,
) -> Result<CatcherReturn, anyhow::Error> {
    let req_payload = payload::decode_payload(request)
        .await
        .context("Could not decode payload")?;
    // First validate the request.
    if validate::validate(catcher, &req_payload)
        .await
        .context("Could not validate request")?
    {
        let mut run = Command::new("/bin/sh")
            .arg("-c")
            .arg(&catcher.run)
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .spawn()
            .context("Could not execute .run command")?;
        {
            let mut stdin = run
                .stdin
                .take()
                .context("Could not get stdin handle of run")?;
            let payload_str = serde_json::to_string(&req_payload)
                .context("Could not serialize request payload")?;
            stdin
                .write_all(payload_str.as_bytes())
                .await
                .context("Could not write request payload to run command")?;
            // Close standard input
            drop(stdin);
        }
        let output = run
            .wait_with_output()
            .await
            .context("Could not wait for process to finish")?;
        Ok(CatcherReturn::Allowed(output.stdout))
    } else {
        info!(
            "`{}` to `{}` failed validation, ignoring",
            req_payload.method.0, req_payload.path
        );
        Ok(CatcherReturn::Denied)
    }
}

async fn handle_request(
    config: &config::Config,
    request: Request<Body>,
) -> Result<Response<Body>, Infallible> {
    let mut response = Response::new(Body::empty());
    match config.catchers.iter().find(|&catcher| {
        catcher.path == request.uri().path()
            && catcher.methods.iter().any(|m| m == request.method())
    }) {
        Some(catcher) => match invoke_catcher(catcher, request).await {
            Ok(ret) => match ret {
                CatcherReturn::Denied => {
                    // Deny the request.
                    *response.status_mut() = StatusCode::FORBIDDEN;
                    Ok(response)
                }
                CatcherReturn::Allowed(body_bytes) => {
                    // Return the body.
                    *response.body_mut() = Body::from(body_bytes);
                    *response.status_mut() = StatusCode::OK;
                    Ok(response)
                }
            },
            Err(e) => {
                error!("{}", e);
                *response.status_mut() = StatusCode::INTERNAL_SERVER_ERROR;
                Ok(response)
            }
        },
        None => {
            // Probably just a random HTTP request, ignore it.
            info!(
                "Invalid request {} to path {} found, returning 404",
                request.method(),
                request.uri().path()
            );
            *response.status_mut() = StatusCode::NOT_FOUND;
            Ok(response)
        }
    }
}

#[derive(StructOpt, Debug)]
struct Opt {
    /// Path to config file (defaults to $XDG_CONFIG_HOME/.config/webhookee/config.json)
    #[structopt(long)]
    config: Option<PathBuf>,
    /// Path to log file (defaults to standard output)
    #[structopt(short, long)]
    log_file: Option<PathBuf>,
}

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    let opt = Opt::from_args();

    {
        use simplelog::*;
        if let Some(log_file_path) = opt.log_file {
            let log_file =
                std::fs::File::create(log_file_path).context("Could not open log file")?;
            WriteLogger::init(LevelFilter::Info, Config::default(), log_file)
                .expect("Could not initialize logging");
        } else {
            TermLogger::init(
                LevelFilter::Info,
                Config::default(),
                TerminalMode::Stdout,
                ColorChoice::Auto,
            )
            .expect("Could not initialize logging");
        }
    }

    let config: &'static config::Config = Box::leak(Box::new(config::get(opt.config)?));

    let addr = SocketAddr::from(([0, 0, 0, 0], config.port));

    let make_svc = make_service_fn(|_conn| {
        let cfg = config;
        async move { Ok::<_, Infallible>(service_fn(move |req| handle_request(cfg, req))) }
    });

    let server = Server::bind(&addr).serve(make_svc);

    if let Err(e) = server.await {
        eprintln!("server error: {}", e);
    }

    Ok(())
}
