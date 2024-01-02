use sea_orm::{DeriveActiveModel, DeriveEntityModel};
use serde::{Deserialize, Serialize};
use sqlx::FromRow;
use sea_orm::entity::prelude::*;

#[derive(FromRow,Debug,Serialize,Deserialize)]
pub(crate) struct Test {
    pub id:i32,
    pub text:Option<String>
}
