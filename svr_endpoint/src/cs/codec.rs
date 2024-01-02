use bytes::{Buf, BufMut, Bytes, BytesMut};
use prost::Message;
use tokio_util::codec::{Decoder, Encoder};
use crate::cs::{CsError};

#[derive(Debug)]
pub struct CsCodeC;

#[derive(Debug,Clone)]
pub struct CsExchange {
    pub len:u32,
    pub data: Bytes,
}


impl CsExchange {
    pub fn from_msg<T>(msg:T) -> CsExchange where T:Message + Default {
        let mut bm = BytesMut::new();
        msg.encode(&mut bm).unwrap();
        CsExchange{
            len:bm.len() as u32,
            data:bm.freeze()
        }
    }
}

impl Decoder for CsCodeC {
    type Item = CsExchange;
    type Error = CsError;

    fn decode(&mut self, src: &mut BytesMut) -> Result<Option<Self::Item>, Self::Error> {
        // Check if there's enough data to read the type and length of the message.
        if src.len() < 4 {
            // Not enough data to read the type and length.
            return Ok(None);
        }
        let len = (&src[0..4]).get_u32();

        // Check if the full message has arrived, including the type, length, and data.
        if (src.len() as u32) < 4 + len {
            // The full message has not yet arrived.
            return Ok(None);
        }

        // Now we can consume the type and length fields.
        let _ = src.split_to(4);

        // Read the message data.
        // Read the message data.
        let data = if len > 0 {
            src.split_to(len as usize).freeze()
        } else {
            Bytes::new()
        };

        let message = CsExchange {
            len,
            data,
        };

        Ok(Some(message))
    }
}

impl Encoder<CsExchange> for CsCodeC {
    type Error = CsError;

    fn encode(&mut self, item: CsExchange, dst: &mut BytesMut) -> Result<(), Self::Error> {
        // Ensure there's enough space in the buffer for the length, type, and data.
        dst.reserve((4 + item.len) as usize);
        dst.put_u32(item.len);
        dst.extend(item.data);
        Ok(())
    }
}

