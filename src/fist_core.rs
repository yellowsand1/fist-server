use std::collections::HashMap;
use std::net::SocketAddr;
use std::ops::AddAssign;
use std::time::Duration;
use actix_web::HttpRequest;
use tokio::sync::RwLock;
use crate::SETTINGS;
use anyhow::Result;
use lazy_static::lazy_static;
use log::{error, info};
use serde::Serialize;
use crate::model::{AsyncFinally, Model, SyncInfo, SyncInfoWrapper};
use reqwest::Client;

lazy_static! {
    /** store the syncInfo in memory **/
    static ref SYNC_INFO: RwLock<HashMap<String, Vec<SyncInfo>>> = RwLock::new(HashMap::new());
    /** store the rollback command stateMachine and change every request **/
    static ref ROLL_BACK: RwLock<HashMap<String, bool>> = RwLock::new(HashMap::new());
    /** store the end of request from java client **/
    static ref END: RwLock<HashMap<String, bool>> = RwLock::new(HashMap::new());
    /** counter to count the transaction times **/
    static ref COUNTER: RwLock<i128> = RwLock::new(0);

    static ref HTTP_CLIENT: Client = {
        Client::builder()
            .pool_max_idle_per_host(20)
            .timeout(Duration::from_secs(30))
            .tcp_keepalive(Duration::from_secs(720))
            .http1_title_case_headers()
            .build()
            .unwrap()
    };
}

/**
 *@description Process sync_info from java client when a globalTransaction is triggered
 * I'll use a static HashMap to store group the sync_info by fist_id,
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
    {
        let mut sync_info_map = SYNC_INFO.write().await;
        sync_info_map.entry(fist_id.clone()).or_insert_with(Vec::new).push(info);
        let mut roll_back_map = ROLL_BACK.write().await;
        let already_rollback = roll_back_map.entry(fist_id.clone()).or_insert(false);
        let new_rollback = *already_rollback | roll_back;
        roll_back_map.insert(fist_id.clone(), new_rollback);
        let mut end_map = END.write().await;
        let already_end = end_map.entry(fist_id.clone()).or_insert(false);
        let new_end = *already_end | end;
        end_map.insert(fist_id.clone(), new_end);
    }
    {
        info!("SyncInfo: {:?}", SYNC_INFO.read().await);
    }
    scan_static_resource(fist_id).await?;
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
async fn scan_static_resource(fist_id: String) -> Result<()> {
    if let Some(sync_info) = SYNC_INFO.read().await.get(&fist_id) {
        let times;
        {
            times = SETTINGS.read().await.get_int("clear_static_times")?;
        }
        let is_end = sync_info.iter().any(|info| info.end);
        //should add more information like count of services to judge if should end now in very short future if necessary
        if is_end {
            let temp_fist_id = fist_id.clone();
            if *ROLL_BACK.read().await.get(&temp_fist_id).unwrap_or(&false) {
                //send rollback command to all services
                // tokio::spawn(async move {
                send_rollback(&temp_fist_id).await.expect("rollback request send error");
                // });
            }
            //try block from here and would execute anyway ! What a elegant way to do this !
            let finally_future = async move {
                let t;
                {
                    // Clear static resource according to fist_id
                    let mut sync_info = SYNC_INFO.write().await;
                    let mut roll_back = ROLL_BACK.write().await;
                    let mut end_map = END.write().await;
                    let mut counter = COUNTER.write().await;
                    sync_info.remove(&fist_id);
                    roll_back.remove(&fist_id);
                    end_map.remove(&fist_id);
                    counter.add_assign(1);
                    t = *counter % times as i128;
                }
                if t == 0 {
                    SYNC_INFO.write().await.shrink_to_fit();
                    ROLL_BACK.write().await.shrink_to_fit();
                    END.write().await.shrink_to_fit();
                }
            };
            let _async_finally = AsyncFinally::new(finally_future);
        }
    }
    Ok(())
}

async fn send_rollback(fist_id: &str) -> Result<(), reqwest::Error> {
    let mut tasks = Vec::new();
    SYNC_INFO.read().await.get(fist_id).unwrap_or(&Vec::new()).iter().for_each(|info| {
        let url = info.service_addr.clone();
        let body = info.clone();
        tasks.push(tokio::spawn(async move {
            if let Err(e) = call_service(&url, body).await {
                error!("rollback request callback error: {:?}", e);
            }
        }));
    });
    futures::future::try_join_all(tasks).await.expect("rollback request send error");
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