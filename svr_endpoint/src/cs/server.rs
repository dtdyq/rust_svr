use std::any::Any;
use std::collections::HashMap;
use std::fmt::{Debug, Formatter};
use std::fs::read;
use std::future::Future;
use std::hash::{DefaultHasher, Hash, Hasher};
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
    aspects:Arc<Vec<Box<dyn CsAspect>>>
}

#[derive(Clone)]
enum Broadcast {
    All(CsExchange),
    Subset{
        ids:Vec<String>,
        msg:CsExchange
    }
}
struct UpStream {
    src:String,
    bc:Broadcast
}
#[derive(Clone)]
struct DownStream {
    src:String,
    bc:Broadcast
}

#[derive(Debug)]
pub struct CsInnerMail {
    pub from:String,
    pub dst:Vec<String>,
    pub mail:CsMessage,
}

struct BcReceiver {

}
impl CsServer {
    pub async fn new(configure:Configure, service: Arc<ServiceFn>) -> Result<CsServer,CsError> where {
        let mut s =  CsServer{
            configure,
            router:service,
            msg_cnt:Arc::new(AtomicU64::new(0)),
            aspects:Arc::new(Vec::new())
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
        let listener = TcpListener::bind(format!("127.0.0.1:{}",cfg.port)).await
            .map_err(|e|ServerInitError(format!("bind addr error:{:?}",e)))?;
        let (tx,rx) = channel::<Arc<CsInnerMail>>(2048);
        self.start_broadcast(rx);
        self.start_accept(listener,tx);
        Ok(())
    }

    fn start_accept(&self, listener: TcpListener, tx: Sender<Arc<CsInnerMail>>) {
        let router = self.router.clone(); // 克隆Arc，提供给异步块
        let cnt = self.msg_cnt.clone();
        let aspects = self.aspects.clone();

        tokio::spawn(async move{
            loop {
                match listener.accept().await {
                    Ok((skt,addr)) => {
                        let mut session = Session::from(skt,addr,tx.clone(),router.clone(),aspects.clone());
                        session.start().await;
                    }
                    Err(e) => {
                        error!("accept error occur:{:?}",e)
                    }
                }
            }
        });
    }

    fn start_broadcast(&self, mut rx: Receiver<Arc<CsInnerMail>>) {

        tokio::spawn(async move {
            loop {
                let rcv = rx.recv().await;
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

    fn session_id(session:&Session) -> SessionId{
        SessionId{
            peer: session.clt_addr,
            timestamp:SystemTime::now().duration_since(UNIX_EPOCH).unwrap().as_millis(),

        }
    }

}

#[derive(Debug,Clone, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct SessionId {
    peer:SocketAddr,
    timestamp:u128
}

pub struct Session {
    pub writer: SplitSink<Framed<TcpStream, CsCodeC>, CsExchange>,
    pub reader: SplitStream<Framed<TcpStream, CsCodeC>>,
    pub clt_addr:SocketAddr,
    rx:Receiver<Arc<CsInnerMail>>,
    tx:Sender<Arc<CsInnerMail>>,
    pub router: Arc<ServiceFn>,
    aspects:Arc<Vec<Box<dyn CsAspect>>>
}
impl Session {
    fn from(skt:TcpStream, addr:SocketAddr,sender: Sender<Arc<CsInnerMail>>, router:Arc<ServiceFn>, fo: Arc<Vec<Box<dyn CsAspect>>>) -> Session {
        let framed = Framed::new(skt, CsCodeC{});
        // 将 Framed 分成读和写两部分
        let (mut writer, mut reader) = framed.split();
        Session{
            writer,
            reader,
            clt_addr:addr,
            rx:sender.subscribe(),
            tx:sender,
            router,
            aspects:fo
        }
    }
    async fn start(mut self) {
        for f in self.aspects.iter() {
            f.on_connect(&self.clt_addr).await;
        }
        let mut bc_recv = self.tx.subscribe();
        let k = format!("{}",self.clt_addr);
        let mut writer = self.writer;
        let mut reader = self.reader;
        let aspects = self.aspects;
        let addr = self.clt_addr;
        let router = self.router;
        let addr = self.clt_addr;
        let tx = self.tx;
        tokio::spawn(async move {
            loop {
                select! {
                    to = sleep(Duration::from_secs(13)) =>{
                        // self.exit(CsDisconnectReason::ReadTimeout).await;
                        break;
                    },
                    ok = Self::try_read_one_msg(&mut reader) =>{
                        match ok {
                            Err(e) => {
                                // self.exit(CsDisconnectReason::ReadError(format!("{:?}",e))).await;
                                break;
                            },
                            Ok(cse) => {
                                match Self::on_message(&aspects,&addr,router.clone(),tx.clone(),cse).await {
                                    None => {},
                                    Some(msg) =>{
                                        Self::send_msg(&mut writer,&msg).await;
                                    }
                                }
                            }
                        }
                    },
                    mail = Self::proc_from_bcx(&k,&mut bc_recv) => {
                        match mail {
                            None => {},
                            Some(m) =>{
                                Self::send_msg(&mut writer,&m.mail).await;
                            }
                        }
                    }
                }
            }
        });
    }
    async fn exit(&mut self,reason:CsDisconnectReason) {
        for f in self.aspects.iter() {
            f.on_disconnect(&self.clt_addr,&reason).await;
        }
        match self.writer.close().await {
            Err(e) => {
                error!("close writer error:{:?}",e)
            }
            _ => {}
        }
    }

    async fn proc_from_bcx(key:&String,mut recv:&mut Receiver<Arc<CsInnerMail>>) -> Option<Arc<CsInnerMail>> {
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
    async fn on_message(aspects:&Arc<Vec<Box<dyn CsAspect>>>,clt_addr:& SocketAddr,router:Arc<ServiceFn>,tx:Sender<Arc<CsInnerMail>>, mut msg: CsExchange) -> Option<CsMessage> {
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
}

impl Hash for Session {
    fn hash<H: Hasher>(&self, state: &mut H) {
        format!("cs_session_{}",self.clt_addr).hash(state)
    }
}
impl PartialEq for Session {
    fn eq(&self, other: &Self) -> bool {
        let mut d1 = DefaultHasher::new();
        let mut d2 = DefaultHasher::new();
        self.hash(&mut d1);
        other.hash(&mut d2);
        d1.finish() == d2.finish()
    }
}
impl Eq for Session {

}