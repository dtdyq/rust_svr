#![allow(unused)]

use std::collections::HashMap;
use std::ops::Deref;
use std::sync::Arc;
use std::time::Duration;
use async_trait::async_trait;
use bytes::BytesMut;
use std::pin::Pin;
use dashmap::DashMap;
use serde::Deserialize;
use tokio::time::sleep;
use tracing::info;
use svr_macro::{cs_handler};
use svr_common::{cfg,trace};
use std::future::Future;
mod event;
mod db;
use prost::{EncodeError, Message};
use syn::ReturnType::Default;
use svr_endpoint::cs::{*};
use svr_endpoint::cs::proto::helloworld::HelloReply;
use svr_endpoint::cs::proto::meta::{CsMessage, HelloReq, HelloResp};
use svr_endpoint::service_handlers;

#[cs_handler]
pub async fn hand(msg_id:MsgId,body:MsgBody<HelloReq>) -> impl IntoCsResponse {
    info!("new hand input:{:?} {:?}",msg_id,body);
    HelloResp{echo:"asdas".to_string(),sequence:12}
}

#[cs_handler]
pub async fn hello(msg_id:MsgId,mut css:CsService) -> impl IntoCsResponse {
    info!("will broadcast");
    css.broadcast(12,HelloReply{message:"broad cast makabaka".to_string()});
    HelloResp{echo:"===".to_string(),sequence:123}
}

#[cs_handler]
pub async fn not_impl(msg_id:MsgId) -> impl IntoCsResponse {
    info!("hauiqgweiquewiqwe");
    let mut m = HashMap::new();
    m.insert("err_code".to_string(),"not impl".to_string());
    m
}

#[tokio::main]
async fn main() {
    let cfg = cfg::Configure::from_dir("config".into()).expect("cfg init error");
    let v = trace::init(&cfg);

    let f = service_handlers!{
        0 => hand,
        1 => hello,
        _ => not_impl
    };

    let css = CsServer::new(cfg.clone(),Arc::new(f)).await.unwrap();
    css.run().await.unwrap();

    info!("svr running");
    loop {
        info!("svr running");
        sleep(Duration::from_millis(5000)).await;
    }
}
#[derive(Debug,Deserialize)]
struct C{
    qw:String
}