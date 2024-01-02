
#[cfg(test)]
mod test {
    use std::fmt::Error;
    use lazy_static::lazy_static;
    use redis::{AsyncCommands, RedisError, JsonAsyncCommands};
    use sqlx::{MySql, Pool};
    use tokio::sync::{Mutex, OnceCell};
    use tracing::info;
    use crate::{cfg, common, db};
    use crate::db::model::Test;


    static OC: OnceCell<()> = OnceCell::const_new();

    async fn setup() {
        // Perform initialization here.
        // Since we're inside an async function, we can await other async operations.
        println!("8718204t371");
        cfg::init().expect("TODO: panic message");
        common::trace::init();
        db::init().await.expect("TODO: panic message");

        // Initialization is done, release the lock.
    }
    #[tokio::test]
    async fn test_query_mysql() {
        OC.get_or_init(setup).await;
        let ts = db::mysql("global").await.unwrap();
        let tts =sqlx::query_as::<_,Test>("select * from test").fetch_all(&ts).await;
        assert!(tts.is_ok());
        info!("query res:{:?}",tts)
    }
    #[tokio::test]
    async fn test_redis_query() {
        OC.get_or_init(setup).await;
        let mut ts = db::redis("global").await.unwrap();
        let _ = ts.set::<&str,&str, bool>("key", "value").await.unwrap();
        let tts = ts.get::<&str, String>("key").await;
        info!("query res:{:?}",tts);
        assert_eq!(Ok("value"),tts.as_deref());

        let t = Test{id:12,text:Some("hello".to_string())};

        let r = serde_json::to_string(&t).unwrap();
        let _: Result<bool, RedisError> = ts.set("test",r).await;

        let tts = ts.get::<&str, String>("test").await.unwrap();
        let rt = serde_json::from_str::<Test>(tts.as_str()).unwrap();
        assert_eq!(rt.text,Some("hello".to_string()))
    }
}