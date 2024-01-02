#[derive(svr_macro::IntoCsResponse)]
#[allow(clippy::derive_partial_eq_without_eq)]
#[derive(Clone, PartialEq, ::prost::Message)]
pub struct HelloReq {
    #[prost(int32, tag = "1")]
    pub addr: i32,
    #[prost(string, tag = "2")]
    pub name: ::prost::alloc::string::String,
}
#[derive(svr_macro::IntoCsResponse)]
#[allow(clippy::derive_partial_eq_without_eq)]
#[derive(Clone, PartialEq, ::prost::Message)]
pub struct HelloResp {
    #[prost(string, tag = "1")]
    pub echo: ::prost::alloc::string::String,
    #[prost(int64, tag = "2")]
    pub sequence: i64,
}
#[derive(svr_macro::IntoCsResponse)]
#[allow(clippy::derive_partial_eq_without_eq)]
#[derive(Clone, PartialEq, ::prost::Message)]
pub struct CsMessage {
    #[prost(uint32, tag = "1")]
    pub msg_id: u32,
    #[prost(uint32, tag = "2")]
    pub seq: u32,
    #[prost(map = "string, string", tag = "3")]
    pub ext: ::std::collections::HashMap<
        ::prost::alloc::string::String,
        ::prost::alloc::string::String,
    >,
    #[prost(bytes = "vec", tag = "4")]
    pub body: ::prost::alloc::vec::Vec<u8>,
}
