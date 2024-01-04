use std::any::Any;
use std::collections::HashMap;
use std::fmt::{Debug, Formatter};
use std::fs::read;
use std::future::Future;
use std::collections::hash_map::DefaultHasher;
use std::hash::{Hash, Hasher};
use std::net::SocketAddr;
use std::ops::Deref;
use std::pin::Pin;
use std::sync::Arc;
use std::sync::atomic::{AtomicU64, Ordering};
use futures_util::SinkExt;
use std::time::{Duration, SystemTime, UNIX_EPOCH};
use std::vec;
use async_trait::async_trait;
use bytes::{Bytes, BytesMut};
use dashmap::DashMap;
use futures_util::stream::{SplitSink, SplitStream};
use futures_util::StreamExt;
use tokio::net::{TcpListener, TcpStream};
use tokio::{pin, select};
use tokio_util::codec::Framed;
use tracing::{error, info, warn};
use svr_common::cfg::{Configure, SvrCfg};
use crate::cs::{CsDisconnectReason, CsError, IntoCsResponse, LogAspect};
use crate::cs::codec::CsExchange;
use crate::cs::CsError::{HandlerNotFound, ServerInitError};
use crate::cs::protocol::{CsAspect, CsHandler, CsRequest, CsResponse};
use crate::cs::codec::CsCodeC;
use tokio::time::sleep;
use prost::Message;
use tokio::sync::broadcast::{channel, Receiver, Sender};
use tokio::sync::broadcast::error::RecvError;
use crate::cs::proto::meta::CsMessage;

pub type ServiceFn = fn(CsRequest) -> Pin<Box<dyn Future<Output=CsResponse>+Send>>;

#[macro_export]
macro_rules! service_handlers {
    ( $( $cmd_id:expr => $handler:expr ),*) => {{
        // 创建一个Arc包装的异步闭包
        move |request: CsRequest| -> Pin<Box<dyn Future<Output=CsResponse>+Send>> {
            Box::pin(async move {
                match request.msg.msg_id {
                    $(
                        $cmd_id => $handler.handle(request).await,
                    )*
                    _ => panic!("not found handler for:{}",request.msg.msg_id)
                }
            }) as Pin<Box<dyn Future<Output = CsResponse> + Send>>
        }
    }};
    ( $( $cmd_id:expr => $handler:expr ),*, _ => $default_handler:expr ) => {{
        // 创建一个Arc包装的异步闭包
        move |request: CsRequest| -> Pin<Box<dyn Future<Output=CsResponse>+Send>> {
            Box::pin(async move {
                match request.msg.msg_id {
                    $(
                        $cmd_id => $handler.handle(request).await,
                    )*
                    _ => $default_handler.handle(request).await
                }
            }) as Pin<Box<dyn Future<Output = CsResponse> + Send>>
        }
    }};
}

pub struct CsServer {
    pub configure:Configure,
    pub router:Arc<ServiceFn>,
    msg_cnt:Arc<AtomicU64>,
    aspects:Arc<Vec<Box<dyn CsAspect>>>,
    raw_broadcast:Sender<Arc<CsInnerMail>>
}
#[derive(Debug)]
pub struct CsInnerMail {
    pub from:String,
    pub dst:Vec<u64>,
    pub mail:CsMessage,
}

struct BcReceiver {

}
impl CsServer {
    pub async fn new(configure:Configure, service: Arc<ServiceFn>) -> Result<CsServer,CsError> where {

        let cfg = &configure.of::<SvrCfg>("config")
            .map_err(|e|ServerInitError(format!("get config error:{:?}",e)))?.cs;
        let (tx,rx) = channel::<Arc<CsInnerMail>>(cfg.broadcast_chan_size);
        let mut s =  CsServer{
            configure,
            router:service,
            msg_cnt:Arc::new(AtomicU64::new(0)),
            aspects:Arc::new(Vec::new()),
            raw_broadcast:tx
        };
        s.add_aspect(LogAspect);
        Ok(s)
    }

    pub fn add_aspect<A>(&mut self,aspect: A) where A:CsAspect + 'static{
        Arc::get_mut(&mut self.aspects).unwrap().push(Box::new(aspect));
    }
    pub async fn run(&self) -> Result<(),CsError> {
        let cfg = &self.configure.of::<SvrCfg>("config")
            .map_err(|e|ServerInitError(format!("get config error:{:?}",e)))?.cs;
        let listener = TcpListener::bind(format!("{}:{}",cfg.ip,cfg.port)).await
            .map_err(|e|ServerInitError(format!("bind addr error:{:?}",e)))?;
        self.start_broadcast();
        self.start_accept(listener);
        Ok(())
    }

    fn start_accept(&self, listener: TcpListener) {
        let router = self.router.clone(); // 克隆Arc，提供给异步块
        let cnt = self.msg_cnt.clone();
        let aspects = self.aspects.clone();
        let broadcast_tx = self.raw_broadcast.clone();
        tokio::spawn(async move{
            loop {
                match listener.accept().await {
                    Ok((skt,addr)) => {
                        dispatch_new_session(skt,addr,broadcast_tx.clone(),router.clone(),aspects.clone());
                    }
                    Err(e) => {
                        error!("accept error occur:{:?}",e)
                    }
                }
            }
        });
    }

    fn start_broadcast(&self) {

        let mut recv = self.raw_broadcast.subscribe();
        tokio::spawn(async move {
            loop {
                let rcv = recv.recv().await;
                // if let Some(msg) = rcv  {
                    // 收集
                    // let keys_and_values: Vec<_> = {
                    //     let sessions_guard = sessions.lock().await; // 锁定sessions
                    //     sessions_guard.iter()
                    //         .map(|ref_multi| (ref_multi.key().clone(), ref_multi.value().clone()))
                    //         .collect() // 收集操作结束，锁会随着guard的离开而释放
                    // };
                    //
                    // // 锁已释放，现在可以对收集出来的数据进行耗时操作
                    // for (key, value) in keys_and_values {
                    //     tokio::spawn(async {
                    //         // 在这异步任务中处理key和value
                    //     });
                    // }
                // }
            }
        });
    }

    async fn start_new_session(&self,skt:TcpStream, addr:SocketAddr) {
    }

}

fn skt_hash(addr:&SocketAddr) -> u64 {
    let mut h = DefaultHasher::new();
    addr.hash(&mut h);
    h.finish()
}

fn dispatch_new_session(skt:TcpStream, addr:SocketAddr, broadcast: Sender<Arc<CsInnerMail>>, service:Arc<ServiceFn>, aspects: Arc<Vec<Box<dyn CsAspect>>>) {

    let mut bc_recv = broadcast.subscribe();
    let session_id = skt_hash(&addr);


    let framed = Framed::new(skt, CsCodeC{});
    let (mut writer, mut reader) = framed.split();


    tokio::spawn(async move {
        for f in aspects.iter() {
            f.on_connect(&addr).await;
        }
        let mut csdr = None;
        loop {
            select! {
                to = sleep(Duration::from_secs(13)) =>{
                    csdr = Some(CsDisconnectReason::ReadTimeout);
                    break;
                },
                ok = try_read_one_msg(&mut reader) =>{
                    match ok {
                        Err(e) => {
                            csdr = Some(CsDisconnectReason::ReadError(format!("{:?}",e)));
                            break;
                        },
                        Ok(cse) => {
                            match proc_one_msg(aspects.deref(),&addr,service.deref(),&broadcast,cse).await {
                                None => {},
                                Some(msg) =>{
                                    send_msg(&mut writer,&msg).await;
                                }
                            }
                        }
                    }
                },
                mail = proc_from_bcx(&session_id,&mut bc_recv) => {
                    match mail {
                        None => {},
                        Some(m) =>{
                            send_msg(&mut writer,&m.mail).await;
                        }
                    }
                }
            }
        }
        for f in aspects.iter() {
            f.on_disconnect(&addr,&csdr.as_ref().unwrap()).await;
        }
    });
}
async fn try_read_one_msg(reader:&mut SplitStream<Framed<TcpStream, CsCodeC>>) -> Result<CsExchange,CsError> {
    match reader.next().await {
        None => {
            Err(CsError::IOError("peer end".to_string()))
        }
        Some(msg) => {
            match msg {
                Ok(ori) => {
                    Ok(ori)
                }
                Err(e) => {
                    Err(e)
                }
            }
        }
    }
}
async fn proc_one_msg(aspects:&Vec<Box<dyn CsAspect>>, clt_addr:& SocketAddr, router:&ServiceFn, tx:&Sender<Arc<CsInnerMail>>, mut msg: CsExchange) -> Option<CsMessage> {
    let mut csm:CsMessage = match CsMessage::decode(&mut msg.data) {
        Ok(msg) => msg,
        Err(e) => {
            // todo 解析报错要不要disconnect
            for f in aspects.iter() {
                f.after_msg_proc(-1, -1, 0, &mut CsError::CodeCError(format!("decode input error:{:?}",e)).into_cs_resp()).await;
            }
            return None;
        }
    };
    for f in aspects.iter() {
        f.before_msg_proc(clt_addr, &mut csm).await;
    }
    let time_now = SystemTime::now();
    let msg_id = csm.msg_id;
    let seq = csm.seq;
    let mut r = (router)(CsRequest { msg: csm,sender:tx.clone() }).await;
    let elp = time_now.elapsed().unwrap().as_millis();
    for f in aspects.iter() {
        f.after_msg_proc(msg_id as i64, seq as i64, elp, &mut r).await;
    }

    Some(CsMessage{
        msg_id,
        seq,
        ext: r.header,
        body:r.body.to_vec()
    })
}

async fn proc_from_bcx(key:&u64,mut recv:&mut Receiver<Arc<CsInnerMail>>) -> Option<Arc<CsInnerMail>> {
    match recv.recv().await {
        Ok(v) => {
            if v.dst.is_empty() || v.dst.contains(key) {
                Some(v)
            } else {
                None
            }
        }
        Err(e) => {
            error!("recv broadcast error:{:?}",e);
            None
        }
    }
}
async fn send_msg(writer:&mut SplitSink<Framed<TcpStream, CsCodeC>, CsExchange>,msg:&CsMessage) {
    let mut  bm = BytesMut::new();
    msg.encode(&mut bm).unwrap();
    let cse = CsExchange {
        len: bm.len() as u32,
        data: bm.freeze(),
    };
    match writer.send(cse).await {
        Ok(_) => {
            info!("sended msg to :{:?} {:?}",msg.seq,writer)
        }
        Err(e) => {
            error!("send rsp error:{:?}",e);
        }
    };
    // self.on_msg.deref()();
}