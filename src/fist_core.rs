use std::net::SocketAddr;
use std::ops::AddAssign;
use std::sync::Arc;
use std::time::Duration;
use actix_web::HttpRequest;
use tokio::sync::{RwLock, Semaphore};
use anyhow::Result;
use lazy_static::lazy_static;
use log::{error, info};
use serde::Serialize;
use crate::model::{AsyncFinally, Model, SyncInfo, SyncInfoWrapper};
use reqwest::Client;
use dashmap::DashMap;
use futures::future::try_join_all;

lazy_static! {
    /** store the syncInfo in memory **/
    static ref SYNC_INFO: DashMap<String, Vec<SyncInfo>> = DashMap::new();
    /** store the rollback command stateMachine and change every request **/
    static ref ROLL_BACK: DashMap<String, bool> = DashMap::new();
    /** store the end of request from java client **/
    static ref END: DashMap<String, bool> = DashMap::new();
    /** counter to count the transaction times **/
    static ref COUNTER: Arc::<RwLock<i128>> = Arc::new(RwLock::new(0));
    /** static the reqwest client **/
    static ref HTTP_CLIENT: Client = {
        Client::builder()
            .pool_max_idle_per_host(20)
            .timeout(Duration::from_secs(15))
            .tcp_keepalive(Duration::from_secs(7200))
            .http1_title_case_headers()
            .build()
            .unwrap()
    };
    /** semaphore to limit the number of concurrent requests, set the concurrency to cpu * 8 for now **/
    static ref SEMAPHORE: Arc<Semaphore> = Arc::new(Semaphore::new(20));
}

/**
 *@description Process sync_info from java client when a globalTransaction is triggered
 * I'll use a static HashMap to store and group the sync_info by fist_id,
 * then I'll need another static HashMap to store if rollback the transaction or not
 * which is the implementation of stateMachine.
 * Of course I should store if the transaction is ended or not.
 *@param sync_info
 *@author hyl
 *@date 2023/4/11
 */
pub async fn process_sync_info(sync_info: String, request: HttpRequest) -> Result<()> {
    let info: SyncInfoWrapper = serde_json::from_str(&sync_info)?;
    let mut info = info.sync_info;
    {
        record_service_addr(&mut info, request).await?;
    }
    let fist_id = info.fist_id.clone();
    let roll_back = info.rollback.clone();
    let end = info.end.clone();
    let new_rollback;
    let new_end;
    {
        SYNC_INFO.entry(fist_id.clone()).or_insert_with(Vec::new).push(info);
        new_rollback = ROLL_BACK.entry(fist_id.clone()).or_insert(false).value() | roll_back;
        ROLL_BACK.insert(fist_id.clone(), new_rollback);
        new_end = END.entry(fist_id.clone()).or_insert(false).value() | end;
        END.insert(fist_id.clone(), new_end);
    }
    let id_for_log = fist_id.clone();
    {
        scan_static_resource(fist_id, new_rollback, new_end).await?;
    }
    info!("SyncInfo : {:?}",SYNC_INFO.get(&id_for_log).unwrap().value());
    {
        COUNTER.write().await.add_assign(1);
    }
    {
        info!("Counter : {:?}",COUNTER.read().await);
    }
    Ok(())
}

async fn record_service_addr(sync_info: &mut SyncInfo, request: HttpRequest) -> Result<()> {
    let port = sync_info.service_port.to_string();
    let peer_addr: Option<SocketAddr> = request.peer_addr();
    if let Some(addr) = peer_addr {
        info!("Client IP: {}, Port: {}", addr.ip(), port);
        let mut ip = "http://".to_string();
        ip.push_str(&addr.ip().to_string());
        ip.push_str(":");
        ip.push_str(&port);
        ip.push_str("/fist/core");
        sync_info.service_addr = ip;
    } else {
        error!("Unable to get client address,{:?}",request);
    }
    Ok(())
}

/**
 *@description scan the static members to find if this is the end of a transaction
 * or if this transaction needs rollback
 *@author hyl
 *@date 2023/4/11
 */
async fn scan_static_resource(fist_id: String, rollback: bool, end: bool) -> Result<()> {
    if let Some(_sync_info) = SYNC_INFO.get(&fist_id) {
        //should add more information like count of services to judge if should end now in very short future if necessary
        if end {
            let temp_fist_id = fist_id.clone();
            //try block from here and would execute anyway ! What a elegant way to do this !
            let finally_future = async move {
                {
                    // Clear static resource according to fist_id
                    let _ = SYNC_INFO.remove(&fist_id);
                    let _ = ROLL_BACK.remove(&fist_id);
                    let _ = END.remove(&fist_id);
                }
            };
            let _async_finally = AsyncFinally::new(finally_future);
            if rollback {
                //send rollback command to all services
                send_rollback(&temp_fist_id).await.expect("rollback request send error");
            }
        }
    }
    Ok(())
}

async fn send_rollback(fist_id: &str) -> Result<(), reqwest::Error> {
    let mut tasks = Vec::new();
    if let Some(infos) = SYNC_INFO.get(fist_id) {
        for info in infos.value() {
            let url = info.service_addr.clone();
            let body = info.clone();
            let semaphore_clone = Arc::clone(&SEMAPHORE);
            tasks.push(tokio::spawn(async move {
                //permit automatically drop when out of scope
                let _permit = semaphore_clone.acquire().await;
                if let Err(e) = call_service(&url, body).await {
                    error!("rollback request callback error: {:?}", e);
                }
            }));
        }
        try_join_all(tasks).await.expect("rollback request send error}");
    }
    Ok(())
}

async fn call_service<T>(url: &str, body: T) -> Result<(), reqwest::Error>
    where
        T: Serialize + Model + Send + Sync + 'static,
{
    let url = url.to_string();
    let response = HTTP_CLIENT
        .post(&url)
        .header("Content-Type", "application/json")
        .header("Connection", "keep-alive")
        .json(&serde_json::json!(body))
        .send().await;
    match response {
        Ok(res) => {
            info!("rollback request callback status: {:?}, url: {:?}", res.status(), url);
        }
        Err(e) => {
            error!("rollback request callback error: {:?}", e);
        }
    }
    Ok(())
}