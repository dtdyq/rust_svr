use lazy_static::lazy_static;
use sqlx::{MySql, Pool};
use sqlx::mysql::MySqlPoolOptions;
use tracing::info;
use crate::cfg::{DbConfig, SvrCfg};
use crate::db::{DbError, DbPool};
use crate::db::DbError::{InitFailed, PoolNotFound};
use dashmap::DashMap;
use crate::db;
use crate::db::model::Test;
lazy_static!(
  static ref DB_POOL_CACHE:DashMap<String,Pool<MySql>> = DashMap::new();
);

pub(super) async fn init_mysql(sc:&SvrCfg) -> Result<(), DbError> {
    for db in &sc.db {
        match db {
            DbConfig::Mysql { name,url,max_conn } => {
                let p = MySqlPoolOptions::new().max_connections(max_conn.clone()).connect(url.as_str()).await;
                match p {
                    Ok(sp) => {
                        DB_POOL_CACHE.insert(name.clone(),sp);
                    }
                    Err(e) => {
                        return Err(InitFailed(format!("{:?}",e)))
                    }
                }
            }
            _ => {}
        }
    }
    info!("mysql init end:{:?}",DB_POOL_CACHE.iter().map(|i|i.key().clone()).collect::<Vec<_>>());
    Ok(())
}

pub(crate) async fn mysql(p0: &str) -> Result<Pool<MySql>, DbError> {
    DB_POOL_CACHE.get(p0).ok_or(PoolNotFound(format!("not found mysql pool:{}",p0)))
        .map(|r|r.value().clone())
}