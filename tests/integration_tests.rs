use hyper::http::Uri;
use hyper::{client::HttpConnector, Client};
use hyper::{Body, Method, Request, Response, StatusCode};
use process_control::ChildExt;
use std::ffi::OsStr;
use std::future::Future;
use std::process::Command;
use std::time::Duration;
use tempfile::TempDir;
use tokio::fs::{File, OpenOptions};
use tokio::io::AsyncWriteExt;
use tokio::test;

fn local_uri(port: u16, path: &str) -> Uri {
    format!("http://localhost:{}{}", port, path)
        .parse()
        .unwrap()
}

async fn full_body(body: Body) -> String {
    String::from_utf8((&hyper::body::to_bytes(body).await.unwrap()).to_vec()).unwrap()
}

const TIMEOUT_SECS: u64 = 2;

async fn send_req(
    client: &Client<HttpConnector>,
    req: Request<Body>,
) -> Result<Response<Body>, hyper::Error> {
    tokio::time::timeout(Duration::from_secs(TIMEOUT_SECS), client.request(req))
        .await
        .unwrap()
}
async fn get(client: &Client<HttpConnector>, uri: Uri) -> Result<Response<Body>, hyper::Error> {
    tokio::time::timeout(Duration::from_secs(TIMEOUT_SECS), client.get(uri))
        .await
        .unwrap()
}

async fn invoke<F, Fut>(config: &str, f: F)
where
    F: FnOnce(Client<HttpConnector>) -> Fut,
    Fut: Future<Output = ()>,
{
    invoke_with_env(&[], config, f).await
}

async fn invoke_with_env<F, Fut>(env: &[(&str, &str)], config: &str, f: F)
where
    F: FnOnce(Client<HttpConnector>) -> Fut,
    Fut: Future<Output = ()>,
{
    let path_to_webhookee = assert_cmd::cargo::cargo_bin("webhookee");
    let tmp_dir = TempDir::new().expect("Could not create temporary dir");
    let cfg_file_path = tmp_dir.path().join("config.json");
    let log_file_path = tmp_dir.path().join("log");
    let mut cfg_file: File = OpenOptions::new()
        .read(true)
        .write(true)
        .create_new(true)
        .open(&cfg_file_path)
        .await
        .unwrap();
    cfg_file.write_all(config.as_bytes()).await.unwrap();
    let mut child = Command::new(path_to_webhookee)
        .arg("--config")
        .arg(&cfg_file_path)
        .arg("--log-file")
        .arg(&log_file_path)
        .envs(env.iter().map(|(k, v)| (OsStr::new(k), OsStr::new(v))))
        .spawn()
        .expect("Failed to execute webhookee");

    // Wait for the server to come up.
    tokio::time::sleep(Duration::from_secs(1)).await;

    // We want to make *sure* the child dies.
    let hook = std::panic::take_hook();
    let child_terminator = child.terminator().unwrap();
    std::panic::set_hook(Box::new(move |info| {
        unsafe { child_terminator.terminate() }.unwrap();
        // let mut logs = String::new();
        // std::io::Read::read_to_string(&mut std::fs::File::open(&log_file_path).unwrap(), &mut logs).unwrap();
        // println!("server logs:\n{:?}", logs);
        hook(info)
    }));

    let client = Client::new();
    f(client).await;
    let _ = child.kill();
}

#[test]
async fn echo() {
    // The webhook will echo any GET or POST request.
    invoke(
        r#"{
    "port": 3017,
    "catchers": [{
        "methods": ["GET", "POST"],
        "path": "/test/echo",
        "run": "jq -j .body",
        "validate": false
    }]
}"#,
        |client| async move {
            for method in [Method::GET, Method::POST] {
                let uri = local_uri(3017, "/test/echo");
                let req = Request::builder()
                    .method(method)
                    .uri(uri)
                    .body(Body::from("hello? anyone there?"))
                    .unwrap();
                let res = send_req(&client, req).await.unwrap();
                assert_eq!(res.status(), StatusCode::OK);
                assert_eq!(full_body(res.into_body()).await, "hello? anyone there?");
            }
        },
    )
    .await;
}

#[test]
async fn fail_validation() {
    invoke(
        r#"{
   "port": 3018,
   "catchers": [{
        "methods": ["GET"],
        "path": "/test/fail_validation",
        "run": "true",
        "validate": "/bin/false # should exit with 1"
   }]
}"#,
        |client| async move {
            let uri = local_uri(3018, "/test/fail_validation");
            let res = get(&client, uri).await.unwrap();
            assert_eq!(res.status(), StatusCode::FORBIDDEN);
            assert_eq!(full_body(res.into_body()).await, "");
        },
    )
    .await;
}

#[test]
async fn four_oh_four() {
    invoke(
        r#"{
    "port": 3019,
    "catchers": []
}"#,
        |client| async move {
            let uri = local_uri(3019, "/test/four_oh_four");
            let res = get(&client, uri).await.unwrap();
            assert_eq!(res.status(), StatusCode::NOT_FOUND);
            assert_eq!(full_body(res.into_body()).await, "");
        },
    )
    .await;
}

#[test]
async fn empty_body_on_get() {
    invoke(
        r#"{
    "port": 3020,
    "catchers": [{
        "methods": ["GET"],
        "path": "/test/empty_body_on_get",
        "run": "true",
        "validate": "[ -z \"$(jq -r '.body')\" ] # check if body is empty"
    }]
}"#,
        |client| async move {
            let uri = local_uri(3020, "/test/empty_body_on_get");
            let res = get(&client, uri).await.unwrap();
            assert_eq!(res.status(), StatusCode::OK);
            assert_eq!(full_body(res.into_body()).await, "");
        },
    )
    .await;
}

#[test]
async fn github_validation() {
    // The example key used here is completely random.
    invoke_with_env(
        &[("KEY", "7211fd007695d5110dc4e0a5334730f90118ac26")],
        r#"{
    "port": 3021,
    "catchers": [{
        "methods": ["POST"],
        "path": "/test/github_validation_raw",
        "run": "printf raw",
        "validate": ["github", "7211fd007695d5110dc4e0a5334730f90118ac26"]
    }, {
        "methods": ["POST"],
        "path": "/test/github_validation_env",
        "run": "printf env",
        "validate": ["github", "$KEY"]
    }]
}"#,
        |client| async move {
            for suffix in ["raw", "env"] {
                let uri = local_uri(3021, &("/test/github_validation_".to_owned() + suffix));
                let req = Request::builder()
                    .method("POST")
                    .uri(uri)
                    // Generated via
                    // ```py
                    // hmac.new(bytes('7211fd007695d5110dc4e0a5334730f90118ac26', 'utf-8'),
                    // msg=bytes('{}', 'utf-8'), digestmod='sha256').hexdigest()
                    // ```
                    .header(
                        "X-Hub-Signature-256",
                        "sha256=814c5791a2a1a868bdb1d14c0072ad6955883df5f0aa4f86918ae82d88f059c2",
                    )
                    .body(Body::from("{}"))
                    .unwrap();
                let res = send_req(&client, req).await.unwrap();
                assert_eq!(res.status(), StatusCode::OK);
                assert_eq!(full_body(res.into_body()).await, suffix);
            }
        },
    )
    .await;
}
