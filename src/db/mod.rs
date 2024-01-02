mod test;
mod model;
mod mysql;
mod redis;

use std::fmt::{Display, format, Formatter};
use std::ops::Deref;
use dashmap::DashMap;
use lazy_static::lazy_static;
use sqlx::mysql::{MySqlPoolOptions, MySqlRow};
use sqlx::{Database, Error, Execute, Executor, FromRow, MySql, MySqlPool, Pool, Row};
use sqlx::database::HasStatement;
use tracing::info;
use crate::db::DbError::InitFailed;

pub(crate) enum DbPool{
    MysqlPool(Pool<MySql>)
}

impl From<Pool<MySql>> for DbPool {
    fn from(value: Pool<MySql>) -> Self {
        DbPool::MysqlPool(value)
    }
}

pub(crate) async  fn init() -> Result<(),DbError>{
    //
    // // let sc = cfg::svr_cfg().unwrap();
    // mysql::init_mysql(&sc).await?;
    // redis::init_redis(&sc).await?;
    Ok(())
}

pub(crate) use mysql::mysql;
pub(crate) use redis::redis;
use svr_common::cfg;


#[derive(Debug)]
pub(crate) enum DbError {
    InitFailed(String),
    PoolNotFound(String),
    Other(Box<dyn std::error::Error>)
}


impl Display for DbError {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        write!(f,"cfg error:{:?}",self)
    }
}


impl std::error::Error for DbError {
}

impl From<std::io::Error> for DbError {
    fn from(error: std::io::Error) -> Self {
        DbError::Other(Box::try_from(error).unwrap())
    }
}