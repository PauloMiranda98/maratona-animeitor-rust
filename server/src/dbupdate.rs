use crate::DB;
use crate::errors::{CResult, Error};

use std::future::Future;
use std::sync::Arc;

use crate::membroadcast;
use tokio::sync::broadcast;
use tokio::{spawn, sync::Mutex};


async fn update_runs_from_data(
    data: (i64, data::ContestFile, data::RunsFile),
    runs: &Arc<Mutex<DB>>,
    runs_tx: &membroadcast::Sender<data::RunTuple>,
    time_tx: &broadcast::Sender<data::TimerData>,
) -> CResult<()> {
    let (time_data, contest_data, runs_data) = data;

    let mut db = runs.lock().await;
    let fresh_runs = db.refresh_db(time_data, contest_data, runs_data)?;

    if runs_tx.receiver_count() > 0 {
        for r in fresh_runs {
            runs_tx.send_memo(r.clone());
        }
    }
    if time_tx.receiver_count() > 0 {
        time_tx.send(db.timer_data()).map_err(|e| Error::SendError(format!("Cannot send timer {:?}", e)))?;
    }
    Ok(())
}

pub fn spawn_db_update_f<F, Fut>(loader: F) -> (
    Arc<Mutex<DB>>,
    Arc<membroadcast::Sender<data::RunTuple>>,
    broadcast::Sender<data::TimerData>
)
where
    F: Fn() -> Fut + Send + 'static,
    Fut: Future<Output = CResult<(i64, data::ContestFile, data::RunsFile)>> + Send,
{
    let shared_db = Arc::new(Mutex::new(DB::empty()));
    let cloned_db = shared_db.clone();
    let (orig_runs_tx, _) = membroadcast::channel(1000000);
    let (time_tx, _) = broadcast::channel(1000000);
    let runs_tx = Arc::new(orig_runs_tx);
    let runs_tx_2 = runs_tx.clone();
    let time_tx_2 = time_tx.clone();

    spawn(async move {
        let dur = tokio::time::Duration::new(1, 0);
        let mut interval = tokio::time::interval(dur);
        loop {
            interval.tick().await;

            let data = loader();

            match data.await {
                Ok(data_ok) => {
                    let result = update_runs_from_data(data_ok, &shared_db, &runs_tx, &time_tx).await;
                    match result {
                        Ok(()) => (),
                        Err(error) => eprintln!("Retrying after error updating runs: \n{}", error),
                    }
                }
                Err(error) => {
                    eprintln!("Retrying after error loading data: \n{}", error);
                }
            }
        }
    });
    (cloned_db, runs_tx_2.clone(), time_tx_2.clone())
}