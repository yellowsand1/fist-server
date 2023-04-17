use std::future::Future;
use std::pin::Pin;
use std::task::{Context, Poll};
use serde::{Serialize, Deserialize};
use fist_macro::Model;
use futures::future::{BoxFuture, FutureExt};


// config struct
#[derive(Debug, Deserialize)]
pub struct Settings {
    pub fist: FistConfig,
}

impl Default for Settings {
    fn default() -> Self {
        Settings {
            fist: FistConfig {
                clear_static_times: 10,
            },
        }
    }
}

#[derive(Debug, Deserialize)]
pub struct FistConfig {
    pub clear_static_times: i32,
}

//rest struct
pub trait Model {}

#[derive(Debug, Serialize, Deserialize, Clone, Model)]
#[serde(rename_all = "camelCase")]
pub struct SyncInfoWrapper {
    pub sync_info: SyncInfo,
}

#[derive(Debug, Serialize, Deserialize, Clone, Model)]
#[serde(rename_all = "camelCase")]
pub struct SyncInfo {
    pub fist_id: String,
    pub rollback: bool,
    pub rollback_sql: Vec<RollBackSql>,
    pub end: bool,
    #[serde(skip_serializing, skip_deserializing)]
    pub service_addr: String,
    pub service_port: i32,
}

#[derive(Debug, Serialize, Deserialize, Clone, Model)]
#[serde(rename_all = "camelCase")]
pub struct RollBackSql {
    pub sql: String,
    pub params: Vec<Vec<serde_json::Value>>,
}

//implementation of try finally
pub struct Finally<F: FnOnce()> {
    finally: Option<F>,
}

impl<F: FnOnce()> Finally<F>
{
    pub fn new(f: F) -> Self
    {
        Finally { finally: Some(f) }
    }
}

impl<F: FnOnce()> Drop for Finally<F>
{
    fn drop(&mut self)
    {
        if let Some(f) = self.finally.take() {
            f();
        }
    }
}

pub struct AsyncFinally {
    f: Option<BoxFuture<'static, ()>>,
}

impl AsyncFinally {
    pub fn new<F>(f: F) -> Self
        where
            F: Future<Output=()> + Send + 'static,
    {
        AsyncFinally { f: Some(f.boxed()) }
    }
}

impl Future for AsyncFinally {
    type Output = ();

    fn poll(mut self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Self::Output> {
        if let Some(f) = self.f.as_mut() {
            let _ = f.poll_unpin(cx);
        }
        Poll::Ready(())
    }
}

impl Drop for AsyncFinally {
    fn drop(&mut self) {
        if let Some(f) = self.f.take() {
            tokio::spawn(f);
        }
    }
}