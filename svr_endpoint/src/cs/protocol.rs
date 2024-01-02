use std::any::Any;
use std::collections::HashMap;
use std::error::Error;
use std::net::SocketAddr;
use std::ops::{Deref, DerefMut};
use std::sync::Arc;
use async_trait::async_trait;
use bytes::{Bytes, BytesMut};
use prost::Message;
use tokio::net::TcpStream;
use tokio::sync::broadcast::Sender;
use tracing::info;
use crate::cs::{CsError, CsInnerMail, CsServer};
use crate::cs::codec::CsExchange;
use crate::cs::CsError::{CodeCError, Other};
use crate::cs::proto::meta::{CsMessage, HelloReq};

pub struct CsRequest {
    pub msg:CsMessage,
    pub sender:Sender<Arc<CsInnerMail>>
}

#[async_trait]
pub trait FromCsRequest{
    type Target;
    fn from_cs_request(r: &mut CsRequest) -> Self::Target;
}


#[derive(Debug)]
pub enum CsDisconnectReason{
    ReadTimeout,
    ReadError(String),
    MsgCodecError(Box<dyn Error + Sync + Send>)
}

#[async_trait]
pub trait CsAspect:Send + Sync{
    async fn on_connect(& self,addr:&SocketAddr){}
    async fn on_disconnect(& self,addr:&SocketAddr,reason:&CsDisconnectReason){}
    async fn before_msg_proc(&self,addr:&SocketAddr,csm:&mut CsMessage) {}
    async fn after_msg_proc(& self,msg_id:i64,seq:i64,elp_millis:u128,r: &mut CsResponse){}
}

pub struct LogAspect;

#[async_trait]
impl CsAspect for LogAspect {
    async fn on_connect(&self, addr: &SocketAddr) {
        info!("new connect enter from:{:?}",addr)
    }
    async fn on_disconnect(&self, addr: &SocketAddr,reason:&CsDisconnectReason) {
        info!("connect will dis for:{:?} with {:?}",addr,reason)
    }
    async fn after_msg_proc(& self,msg_id:i64,seq:i64,elp_millis:u128,r: &mut CsResponse){
        info!("msg {}  seq:{} proc end, cost millis:{}",msg_id,seq,elp_millis)
    }
}


#[async_trait]
pub trait CsHandler : Sync + Send {
    async fn handle(&self,r: CsRequest) -> CsResponse;
}
#[derive(Debug)]
pub struct MsgId(u32);
impl Deref for MsgId {
    type Target = u32;

    fn deref(&self) -> &Self::Target {
        &self.0
    }
}
impl FromCsRequest for MsgId {
    type Target = MsgId;

    fn from_cs_request(r: &mut CsRequest) -> Self::Target {
        MsgId(r.msg.msg_id)
    }
}

#[derive(Debug)]
pub struct MsgBody<T:Message +Default>(T);

impl<T> Deref for MsgBody<T> where T:Message + Default{
    type Target = T;

    fn deref(&self) -> &Self::Target {
        &self.0
    }
}
impl<T> DerefMut for MsgBody<T> where T:Message + Default{
    fn deref_mut(&mut self) -> &mut Self::Target {
        &mut self.0
    }
}

impl<T> FromCsRequest for MsgBody<T> where T:Message + Default {
    type Target = MsgBody<T>;

    fn from_cs_request(r: &mut CsRequest) -> Self::Target {
        let b = &r.msg.body;
        let v = T::decode(b.as_slice()).unwrap();
        MsgBody(v)
    }
}

pub struct CsService {
    inner:Sender<Arc<CsInnerMail>>
}

impl CsService {
    pub fn broadcast<T>(&mut self,msg_id: u32,msg:T) where T:Message + Default {
        let mut bm = BytesMut::new();
        msg.encode(&mut bm).unwrap();
        let m = Arc::new(CsInnerMail{
            from: "".to_string(),
            dst: vec![],
            mail: CsMessage {
                msg_id,
                seq: 0,
                ext: Default::default(),
                body: bm.to_vec(),
            },
        });
        self.inner.send(m).unwrap();
    }
}
impl FromCsRequest for CsService {
    type Target = CsService;

    fn from_cs_request(r: &mut CsRequest) -> Self::Target {
        CsService {
            inner: r.sender.clone()
        }
    }
}

pub struct CsResponse {
    pub header:HashMap<String,String>,
    pub body:Bytes
}

pub trait IntoCsResponse : Sized{
    fn into_cs_resp(self) -> CsResponse;
}
impl IntoCsResponse for  HashMap<String,String> {
    fn into_cs_resp(self) -> CsResponse {
        CsResponse{
            header:self,
            body:Bytes::new()
        }
    }
}
impl IntoCsResponse for HashMap<&str, &str> {

    fn into_cs_resp(self) -> CsResponse {
        CsResponse{
            header:self.into_iter()
                .map(|(k, v)| (String::from(k), String::from(v)))
                .collect(),
            body:Bytes::new()
        }
    }
}

impl IntoCsResponse for CsError {
    fn into_cs_resp(self) -> CsResponse {
        let mut hm = HashMap::new();
        match self {
            CsError::HandleError { err_code,err_msg } => {
                hm.insert("err_code".to_string(),err_code.to_string());
                hm.insert("err_msg".to_string(),err_msg);
            }
            e => {
                hm.insert("err_code".to_string(),"-1".to_string());
                hm.insert("err_msg".to_string(),format!("{:?}",e));
            }
        }
        CsResponse{
            header:hm,
            body:Bytes::new()
        }
    }
}

impl IntoCsResponse for Bytes {

    fn into_cs_resp(self) -> CsResponse {
        CsResponse{
            header:HashMap::new(),
            body:self
        }
    }
}

