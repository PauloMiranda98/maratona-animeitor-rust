pub mod config;
pub mod dataio;
pub mod errors;

use crate::errors::{CResult, Error};

extern crate itertools;
extern crate rand;

use crate::dataio::*;

use hyper::Client;
use hyper_tls::HttpsConnector;

use hyper::body;
use std::io::prelude::*;
use std::sync::Arc;
use tokio;
use tokio::{spawn, sync::Mutex};
use zip;

use tokio::sync::{mpsc, RwLock};

use futures::{FutureExt, Sink, SinkExt, StreamExt};
use warp::ws::{Message, WebSocket};

use warp::Filter;

pub fn spawn_db_update(data_url: String) -> Arc<Mutex<DB>> {
    let shared_db = Arc::new(Mutex::new(DB::empty()));
    let cloned_db = shared_db.clone();
    spawn(async move {
        let dur = tokio::time::Duration::new(30, 0);
        let mut interval = tokio::time::interval(dur);
        loop {
            interval.tick().await;
            let r = update_runs(&data_url, cloned_db.clone()).await;
            match r {
                Ok(_) => (),
                Err(e) => eprintln!("Error updating run: {}", e),
            }
        }
    });
    shared_db
}

pub fn serve_urlbase(
    shared_db: Arc<Mutex<DB>>,
    source: &Option<String>,
    secret: &String,
) -> warp::filters::BoxedFilter<(impl warp::Reply,)> {
    type Shared = Arc<Mutex<DB>>;
    fn with_db(
        db: Shared,
    ) -> impl Filter<Extract = (Shared,), Error = std::convert::Infallible> + Clone {
        warp::any().map(move || db.clone())
    }

    // warp::path(source.clone())
    let runs = warp::path("runs")
        .and(with_db(shared_db.clone()))
        .and_then(serve_runs);

    let all_runs = warp::path("allruns")
        .and(with_db(shared_db.clone()))
        .and_then(serve_all_runs);

    let all_runs_secret = warp::path(format!("allruns_{}", secret))
        .and(with_db(shared_db.clone()))
        .and_then(serve_all_runs_secret);

    // let timer =
    //     warp::path("timer")
    //     .and(with_db(shared_db.clone()))
    //     .and_then(serve_timer);

    let timer = warp::path("timer")
        .and(warp::ws())
        .and(with_db(shared_db.clone()))
        .map(|ws: warp::ws::Ws, db| ws.on_upgrade(move |ws| serve_timer_ws(ws, db)));

    let contest_file = warp::path("contest")
        .and(with_db(shared_db.clone()))
        .and_then(serve_contestfile);

    let scoreboard = warp::path("score")
        .and(with_db(shared_db))
        .and_then(serve_score);

    let routes = runs
        .or(all_runs)
        .or(all_runs_secret)
        .or(timer)
        .or(contest_file)
        .or(scoreboard);

    match source {
        None => routes.boxed(),
        Some(source) => warp::path(source.clone()).and(routes).boxed(),
    }
}

async fn read_bytes_from_path(path: &String) -> CResult<Vec<u8>> {
    read_bytes_from_url(path)
        .await
        .or_else(|_| read_bytes_from_file(path))
}

fn read_bytes_from_file(path: &String) -> CResult<Vec<u8>> {
    Ok(std::fs::read(&path)?)
}

async fn read_bytes_from_url(uri: &String) -> CResult<Vec<u8>> {
    let https = HttpsConnector::new();
    let client = Client::builder().build::<_, hyper::Body>(https);

    let uri = uri.parse()?;

    let resp = client.get(uri).await?;
    let bytes = body::to_bytes(resp.into_body()).await?;
    Ok(bytes.to_vec())
}

fn try_read_from_zip(
    zip: &mut zip::ZipArchive<std::io::Cursor<&std::vec::Vec<u8>>>,
    name: &str,
) -> CResult<String> {
    let mut runs_zip = zip
        .by_name(name)
        .map_err(|e| Error::Info(format!("Could not unpack file: {} {:?}", name, e)))?;
    let mut buffer = Vec::new();
    runs_zip.read_to_end(&mut buffer)?;
    let runs_data = String::from_utf8(buffer)
        .map_err(|_| Error::Info("Could not parse to UTF8".to_string()))?;
    Ok(runs_data)
}

fn read_from_zip(
    zip: &mut zip::ZipArchive<std::io::Cursor<&std::vec::Vec<u8>>>,
    name: &str,
) -> CResult<String> {
    try_read_from_zip(zip, name)
        .or_else(|_| try_read_from_zip(zip, &format!("./{}", name)))
        .or_else(|_| try_read_from_zip(zip, &format!("./sample/{}", name)))
        .or_else(|_| try_read_from_zip(zip, &format!("sample/{}", name)))

    // .or_else(|t| try_read_from_zip(zip, name))?
}

async fn update_runs(uri: &String, runs: Arc<Mutex<DB>>) -> CResult<()> {
    // let zip_data = read_bytes_from_url(uri).await?;
    let zip_data = read_bytes_from_path(uri).await?;

    let reader = std::io::Cursor::new(&zip_data);
    let mut zip = zip::ZipArchive::new(reader)
        .map_err(|e| Error::Info(format!("Could not open zipfile: {:?}", e)))?;

    let time_data: i64 = read_from_zip(&mut zip, "time")?.parse()?;

    let contest_data = read_from_zip(&mut zip, "contest")?;
    let contest_data = read_contest(&contest_data)?;

    let runs_data = read_from_zip(&mut zip, "runs")?;
    let runs_data = read_runs(&runs_data)?;

    let mut db = runs.lock().await;
    db.refresh_db(time_data, contest_data, runs_data)?;
    Ok(())
}

async fn serve_runs(runs: Arc<Mutex<DB>>) -> Result<impl warp::Reply, warp::Rejection> {
    let db = runs.lock().await;
    let r = serde_json::to_string(&*db.latest()).unwrap();
    Ok(r)
}

async fn serve_timer_ws(ws: warp::ws::WebSocket, runs: Arc<Mutex<DB>>) {
    let (mut tx, _) = ws.split();

    let fut = async move {
        
        let dur = tokio::time::Duration::new(1, 0);
        let mut interval = tokio::time::interval(dur);
        
        let mut old = data::TimerData::fake();

        loop {
            interval.tick().await;
            let l = runs.lock().await.timer_data();

            if l != old {
                old = l;
                let t = serde_json::to_string(&l).unwrap();
                let m = Message::text(t);
                tx.send(m).await.expect("Error sending");
            }
        }
    };

    tokio::task::spawn(fut);
}

// async fn serve_timer(runs: Arc<Mutex<DB>>) -> Result<impl warp::Reply, warp::Rejection> {
//     let db = runs.lock().await;
//     let r = serde_json::to_string(&db.timer_data()).unwrap();
//     Ok(r)
// }

async fn serve_all_runs(runs: Arc<Mutex<DB>>) -> Result<impl warp::Reply, warp::Rejection> {
    let db = runs.lock().await;
    let r = serde_json::to_string(&db.run_file).unwrap();
    Ok(r)
}

async fn serve_all_runs_secret(runs: Arc<Mutex<DB>>) -> Result<impl warp::Reply, warp::Rejection> {
    let db = runs.lock().await;
    let r = serde_json::to_string(&db.run_file_secret).unwrap();
    Ok(r)
}

async fn serve_contestfile(runs: Arc<Mutex<DB>>) -> Result<impl warp::Reply, warp::Rejection> {
    let db = runs.lock().await;
    let r = serde_json::to_string(&db.contest_file_begin).unwrap();
    Ok(r)
}

async fn serve_score(runs: Arc<Mutex<DB>>) -> Result<impl warp::Reply, warp::Rejection> {
    let db = runs.lock().await;
    let r = serde_json::to_string(&db.get_scoreboard()).unwrap();
    Ok(r)
}

pub fn random_path_part() -> String {
    use rand::Rng;
    const CHARSET: &[u8] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZ\
                            abcdefghijklmnopqrstuvwxyz\
                            0123456789";
    const PASSWORD_LEN: usize = 6;
    let mut rng = rand::thread_rng();
    (0..PASSWORD_LEN)
        .map(|_| {
            let idx = rng.gen_range(0, CHARSET.len());
            CHARSET[idx] as char
        })
        .collect()
}

pub async fn serve_simple_contest(url_base: String, server_port: u16, secret: &String) {
    let shared_db = spawn_db_update(url_base);
    serve_simple_contest_assets(shared_db, server_port, secret).await
}

pub async fn serve_simple_contest_assets(db: Arc<Mutex<DB>>, server_port: u16, secret: &String) {
    let static_assets = warp::path("static").and(warp::fs::dir("static"));
    let seed_assets = warp::path("seed").and(warp::fs::dir("client"));

    let root = warp::path::end()
        .map(|| warp::redirect(warp::http::Uri::from_static("/seed/everything2.html")));

    let routes = root
        .or(static_assets)
        .or(seed_assets)
        .or(serve_urlbase(db, &None, secret));
    warp::serve(routes).run(([0, 0, 0, 0], server_port)).await
}

#[tokio::main]
async fn main() {
    let routes = warp::path("echo")
        // The `ws()` filter will prepare the Websocket handshake.
        .and(warp::ws())
        .map(|ws: warp::ws::Ws| {
            // And then our closure will be called when it completes...
            ws.on_upgrade(|websocket| {
                // Just echo all messages back...
                let (tx, rx) = websocket.split();
                rx.forward(tx).map(|result| {
                    if let Err(e) = result {
                        eprintln!("websocket error: {:?}", e);
                    }
                })
            })
        });

    warp::serve(routes).run(([127, 0, 0, 1], 3030)).await;
}
