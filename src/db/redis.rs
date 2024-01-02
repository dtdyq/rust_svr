use bb8::{Pool, PooledConnection};
use bb8_redis::{bb8, RedisConnectionManager};
use lazy_static::lazy_static;
use sqlx::mysql::MySqlPoolOptions;
use tracing::info;
use crate::cfg::{DbConfig, SvrCfg};
use crate::db::{DbError, DbPool};
use crate::db::DbError::{InitFailed, Other, PoolNotFound};
use dashmap::DashMap;
use redis::aio::Connection;
use redis::{AsyncCommands, Client, Commands, RedisError};

lazy_static!(
  static ref DB_POOL_CACHE:DashMap<String,bb8::Pool<RedisConnectionManager>> = DashMap::new();
);

pub(super) async fn init_redis(sc:&SvrCfg) -> Result<(), DbError> {
    for db in &sc.db {
        match db {
            DbConfig::Redis { name,url } => {
                let m = RedisConnectionManager::new(url.as_str());
                match m {
                    Ok(sp) => {
                        match Pool::builder().build(sp).await {
                            Ok(p) => {
                                DB_POOL_CACHE.insert(name.clone(),p);
                            }
                            Err(e) => {
                                return Err(InitFailed(format!("{:?}",e)))
                            }
                        }
                    }
                    Err(e) => {
                        return Err(InitFailed(format!("{:?}",e)))
                    }
                }
            }

            _ => {}
        }
    }

    info!("redis init end:{:?}",DB_POOL_CACHE.iter().map(|i|i.key().clone()).collect::<Vec<_>>());
    Ok(())
}


pub(crate) async fn redis(ns:&str) -> Result<PooledConnection<RedisConnectionManager>,DbError> {
    match DB_POOL_CACHE.get(ns) {
        None =>Err(PoolNotFound(format!("redis pool not found:{:?}",ns))),
        Some(rf) =>{
            let c = rf.value().clone();
            match c.get_owned().await {
                Ok(c) => {
                    Ok(c)
                },
                Err(e) =>{
                    Err(Other(Box::from(e)))
                }
            }
        }
    }
}