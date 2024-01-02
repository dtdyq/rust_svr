use std::collections::HashMap;
use std::net::SocketAddr;
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, AtomicU32, Ordering};
use std::time::Duration;
use bytes::{Bytes, BytesMut};
use dashmap::DashMap;
use futures_util::stream::{SplitSink, SplitStream};
use futures_util::{SinkExt, StreamExt};
use prost::Message;
use tokio::net::tcp::WriteHalf;
use tokio::net::TcpStream;
use tokio::sync::{Mutex, oneshot};
use tokio::sync::broadcast::error::RecvError;
use tokio::sync::oneshot::{channel, Sender};
use tokio::time::sleep;
use tokio_util::codec::{Framed, FramedWrite};
use tracing::error;
use crate::cs::codec::{CsCodeC, CsExchange};
use crate::cs::{CsError};
use crate::cs::CsError::Other;
use crate::cs::proto::meta::CsMessage;

#[derive(Debug)]
pub struct ClientResponse<T:Message>{
    pub msg_id:i64,
    pub seq:i64,
    pub header:HashMap<String,String>,
    pub msg: Option<T>
}
impl<T:Message> ClientResponse<T> {
    fn from_err(msg_id:i64,seq:i64,e:CsError) -> ClientResponse<T> {
        let hm = match e {
            CsError::HandleError { err_code,err_msg } => {
                let mut h = HashMap::new();
                h.insert("err_code".to_string(),err_code.to_string());
                h.insert("err_msg".to_string(),err_msg.to_string());
                h
            }
            e => {
                let mut h = HashMap::new();
                h.insert("err_code".to_string(),"-1".to_string());
                h.insert("err_msg".to_string(),format!("{:?}",e));
                h
            }
        };
        ClientResponse{msg_id,seq,header:hm, msg: None }
    }
}


#[derive(Clone)]
pub struct CsClient {
    local:SocketAddr,
    rw: Arc<Mutex<(SplitSink<Framed<TcpStream, CsCodeC>, CsExchange>)>>,
    seq:Arc<AtomicU32>,
    msg_ped:Arc<DashMap<u32,Sender<CsMessage>>>,
    ab:Arc<AtomicBool>,
    bc_pair:Arc<Mutex<tokio::sync::mpsc::Receiver<CsMessage>>>,
}

impl CsClient {
    pub async fn new<F>(addr:&str,f: F) -> Result<CsClient,CsError> where F:Fn(CsMessage) + Send + Sync + 'static{
        let mut stream = TcpStream::connect(addr).await.map_err(|e|Other(format!("connect error:{:?}",e)))?;
        let sa = stream.local_addr().unwrap();
        let framed = Framed::new(stream, CsCodeC{});
        // 将 Framed 分成读和写两部分
        let (mut writer, mut reader) = framed.split();
        let c:Arc<DashMap<u32,Sender<CsMessage>>> = Arc::new(DashMap::new());
        let ab = Arc::new(AtomicBool::new(false));
        let cc = c.clone();
        let abc = ab.clone();
        let (us,ur) = tokio::sync::mpsc::channel(1024);
        let c = CsClient{local:sa,rw:Arc::new(Mutex::new(writer)),seq:Arc::new(AtomicU32::new(0)),msg_ped:c,ab,bc_pair:Arc::new(Mutex::new(ur))};
        tokio::spawn(async move{
            while !abc.load(Ordering::SeqCst) {
                let msg = reader.next().await;
                match msg {
                    None => {
                        error!("peer read none:");
                        break
                    }
                    Some(ret) => {
                        match ret {
                            Ok(cse) => {
                                let mut bytes = cse.data;
                                let csm:CsMessage = CsMessage::decode(&mut bytes).map_err(|e|Other(format!("decode error:{:?}",e))).unwrap();
                                let seq= &csm.seq;
                                {
                                    if cc.contains_key(seq) {
                                        let mut s = cc.remove(seq).unwrap();
                                        s.1.send(csm).unwrap();
                                    } else {
                                        f(csm);
                                    }
                                }
                            }
                            Err(e) => {
                                println!("recv msg error:{:?}",e);
                                break
                            }
                        }
                    }
                }
            }
        });
        Ok(c)
    }

    pub async fn get_broadcast(&mut self) -> Option<CsMessage> {
        self.bc_pair.lock().await.recv().await
    }

    pub async fn send<RSP:Message + Default>(&mut self,msg_id:u32,req:impl Message) -> ClientResponse<RSP>{
        let mut b = BytesMut::new();
        match req.encode(&mut b).map_err(|e|Other(format!("encode error:{:?}",e))) {
            Ok(_)=>{},
            Err(e)=>{
                return ClientResponse::<RSP>::from_err(-1,-1,e);
            }
        }
        let seq = self.seq.fetch_add(1,Ordering::SeqCst);
        let mm = CsMessage{
            msg_id,
            seq,
            ext: Default::default(),
            body: Vec::from(b.freeze()),
        };
        let mut bb = BytesMut::new();
        match mm.encode(&mut bb).map_err(|e|Other(format!("encode error:{:?}",e))) {
            Ok(_)=>{},
            Err(e)=>{
                return ClientResponse::<RSP>::from_err(-1,-1,e);
            }
        }
        let message = CsExchange {
            len: bb.len() as u32,
            data: bb.freeze(),
        };
        let (tx,rx) = channel();
        {
            self.msg_ped.insert(seq,tx);
        }
        {
            let mut writer = self.rw.lock().await;
            match writer.send(message).await {
                Ok(_)=>{},
                Err(e)=>{
                    return ClientResponse::<RSP>::from_err(-1,-1,e);
                }
            }
        }
        let msg = match rx.await.map_err(|e|Other(format!("recv error:{:?}",e))) {
            Ok(v)=> v,
            Err(e)=>{
                return ClientResponse::<RSP>::from_err(-1,-1,e);
            }
        };
        match msg.ext.get("err_code") {
            None => {}
            Some(v) => {
                if v != "0" {
                    return ClientResponse{
                        msg_id:msg.msg_id as i64,seq:msg.seq as i64,header:msg.ext,msg:None
                    }
                }
            }
        }
        let msg_r = match RSP::decode(msg.body.iter().as_slice()).map_err(|e|Other(format!("decode err:{:?}", e))) {
            Ok(m)=>m,
            Err(e)=>{
                return ClientResponse::<RSP>::from_err(-1,-1,e);
            }
        };
        ClientResponse{
            msg_id:msg.msg_id as i64,seq:msg.seq as i64,header:msg.ext,msg:Some(msg_r)
        }
    }

}

#[cfg(test)]
mod test_cs_client {
    use std::env::current_dir;
    use std::sync::{Arc, Mutex};
    use std::time::{Duration, SystemTime};
    use rand::RngCore;
    use tokio::task::JoinHandle;
    use tokio::time::sleep;
    use tracing::{error, info};
    use svr_common::cfg::Configure;
    use svr_common::trace;
    use crate::cs::client::CsClient;
    use crate::cs::proto::meta::{HelloReq, HelloResp};

    #[tokio::test]
    async fn test_broadcast() {
        let cfg  =Configure::from_dir("../config".into()).unwrap();
        let t = trace::init(&cfg);
        let mut csc = CsClient::new("127.0.0.1:8899",|csm|{
            info!("recv broad cast from self:{:?}",csm)
        }).await.unwrap();
        let mut csc_bc = CsClient::new("127.0.0.1:8899",|csm|{
            info!("recv broad cast from other:{:?}",csm)
        }).await.unwrap();


        tokio::spawn(async move{
            info!("send port:{}",csc.local);
            csc.send::<HelloResp>(1,HelloReq{ addr: 12, name: "haha".to_string() }).await
        });

        sleep(Duration::from_millis(500)).await;
    }

    #[tokio::test]
    async fn test() {
        println!("curr dir:{:?}",current_dir());
        let cfg  =Configure::from_dir("../config".into()).unwrap();

        let t = trace::init(&cfg);
        let mut csc = CsClient::new("127.0.0.1:8899",|_|{}).await.unwrap();
        let mut v = Vec::new();
        let vec = Arc::new(Mutex::new(Vec::with_capacity(10000)));
        for i in 0..1000 {
            v.push(many_req_with_new_csc(i,vec.clone()).await);
        }
        for vv in v {
            vv.await.expect("TODO: panic message");
        }
        let ret = vec.lock().unwrap();
        info!("max cost:{:?}",ret.iter().max().unwrap());
        info!("min cost:{:?}",ret.iter().min().unwrap());
        info!("avg cost:{:?}",ret.iter().sum::<u128>() / ret.len() as u128);
        // 200 client 5000 times:
        //max 454
        //min 0
        //avg 22
        // 500 client 2000 times:
        //max 811
        //min 0
        //avg 57
    }
    async fn many_req_with_new_csc(id:u32,times_cost:Arc<Mutex<Vec<u128>>>) -> JoinHandle<()> {
        let mut csc = CsClient::new("127.0.0.1:8899",move |csm|{
            info!("recv broad cast from other:{:?} {:?}",id, csm)
        }).await.unwrap();
        tokio::spawn(async move{
            for i in 0..1000 {
                let r = rand::thread_rng().next_u32();
                let t = SystemTime::now();
                let resp = if r % 2 == 0 {
                    csc.send::<HelloResp>(0,HelloReq{ addr: 12, name: "haha".to_string() }).await
                } else {
                    csc.send::<HelloResp>(1,HelloReq{ addr: 12, name: "haha".to_string() }).await
                };
                let elp = t.elapsed().unwrap().as_millis();
                times_cost.lock().unwrap().push(elp);
                info!("{} {} resp cost {} -- {:?}",id,i,elp,resp);
            }
        })
    }
    async fn many_req(id:u32, mut csc: CsClient) -> JoinHandle<()> {
        tokio::spawn(async move{
            for i in 0..200 {
                let t = SystemTime::now();
                let r = rand::thread_rng().next_u32();
                let resp = if r % 2 == 0 {
                    csc.send::<HelloResp>(0,HelloReq{ addr: 12, name: "haha".to_string() }).await
                } else {
                    csc.send::<HelloResp>(1,HelloReq{ addr: 12, name: "haha".to_string() }).await
                };

                info!("{} {} resp cost {} -- {:?}",id,i,t.elapsed().unwrap().as_millis(),resp);
            }
        })
    }
}