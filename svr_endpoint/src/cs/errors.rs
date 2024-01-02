use std::error::Error;
use std::fmt::{Display, Formatter};
use crate::cs::errors::CsError::IOError;

#[derive(Debug)]
pub enum CsError {
    ServerInitError(String),
    IOError(String),
    CodeCError(String),
    HandlerNotFound(u32),
    HandleError{
        err_code:u32,
        err_msg:String,
    },
    Other(String)
}

impl Display for CsError {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        write!(f,"CS error:{:?}",self)
    }
}

impl Error for CsError {
}

impl From<std::io::Error> for CsError {
    fn from(value: std::io::Error) -> Self {
        IOError(format!("{:?}",value))
    }
}