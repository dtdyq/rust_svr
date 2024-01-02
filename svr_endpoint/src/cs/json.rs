use async_trait::async_trait;
use serde::{Deserialize, Serialize};
use serde_json::Value;
use crate::cs::CsExchange;
use crate::cs::errors::CsError;

#[derive(Debug,Serialize,Deserialize)]
pub struct JsonMsg {
    pub msg_id:u64,
    pub data:String,
}
impl JsonMsg {
    pub fn from_json<T>(id:u64, data:T) -> Self where for<'a> T :Sized + Serialize + Deserialize<'a>{
        JsonMsg{
            msg_id:id,
            data:serde_json::to_string(&data).unwrap()
        }
    }
}

pub struct Json<T>(pub T);
#[async_trait]
pub trait FromJsonMsg: Sized {
    async fn from_json_msg(req: JsonMsg) -> Result<Self, String>;
}


// 为 Body<T> 实现 FromRequest，其中 T 是可反序列化的
#[async_trait]
impl<T> FromJsonMsg for Json<T>
    where
        T: for<'de> Deserialize<'de> + Send,
{
    async fn from_json_msg(req: JsonMsg) -> Result<Self, String> {
        let data = serde_json::from_str::<T>(&req.data).map_err(|e| e.to_string())?;
        Ok(Json(data))
    }
}