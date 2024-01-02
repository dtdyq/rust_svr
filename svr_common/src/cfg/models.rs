use serde::Deserialize;

#[derive(Deserialize,Debug)]
pub struct SvrCfg {
    pub svr_id:i32,
    #[serde(default)]
    pub ports:Vec<i32>,
    pub log:LogConfig,
    pub cs:CsEndpointConfig,
    pub db:Vec<DbConfig>
}

#[derive(Deserialize,Debug)]
pub struct CsEndpointConfig {
    pub port:i32
}
#[derive(Deserialize,Debug)]
#[serde(tag = "type")] // 使用 "type" 字段来区分不同的枚举变体
pub enum DbConfig {
    Mysql{
        name:String,
        url:String,
        max_conn:u32,
    },
    Redis{
        name:String,
        url:String,
    }
}

#[derive(Deserialize,Debug)]
pub struct LogConfig {
    pub dir:String,
    pub file_name:String,
    pub level:String,
    pub to_stdout:bool,
    pub to_file:bool,
    pub with_thread_id:bool,
    pub with_thread_name:bool,
    pub with_line_number:bool
}
#[derive(Deserialize,Debug)]
pub struct MysqlConfig {
    pub ip:String
}
