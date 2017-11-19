/// EDNS0 Options from RFC 5001.

use std::fmt;
use bytes::{BufMut, Bytes};
use ::bits::compose::Composable;
use ::bits::error::ShortBuf;
use ::bits::message_builder::OptBuilder;
use ::bits::parse::Parser;
use ::iana::OptionCode;
use super::OptData;


//------------ Nsid ---------------------------------------------------------/

/// The Name Server Identifier (NSID) Option.
///
/// Specified in RFC 5001.
#[derive(Clone, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub struct Nsid {
    bytes: Bytes
}

impl Nsid {
    pub fn new(bytes: Bytes) -> Self {
        Nsid { bytes }
    }

    pub fn push<T: AsRef<[u8]>>(builder: &mut OptBuilder, data: &T)
                                -> Result<(), ShortBuf> {
        let data = data.as_ref();
        assert!(data.len() <= ::std::u16::MAX as usize);
        builder.build(OptionCode::Nsid, data.len() as u16, |buf| {
            buf.compose(data)
        })
    }
}

impl OptData for Nsid {
    type ParseErr = ShortBuf;

    fn code(&self) -> OptionCode {
        OptionCode::Nsid
    }

    fn parse(code: OptionCode, len: usize, parser: &mut Parser)
             -> Result<Option<Self>, Self::ParseErr> {
        if code != OptionCode::Nsid {
            return Ok(None)
        }
        parser.parse_bytes(len).map(|bytes| Some(Nsid::new(bytes)))
    }
}

impl Composable for Nsid {
    fn compose_len(&self) -> usize {
        self.bytes.len()
    }

    fn compose<B: BufMut>(&self, buf: &mut B) {
        assert!(self.bytes.len() < ::std::u16::MAX as usize);
        buf.put_slice(self.bytes.as_ref())
    }
}

impl fmt::Display for Nsid {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        // RFC 5001 § 2.4:
        // | User interfaces MUST read and write the contents of the NSID
        // | option as a sequence of hexadecimal digits, two digits per
        // | payload octet.
        for v in self.bytes.as_ref() {
            write!(f, "{:X}", *v)?
        }
        Ok(())
    }
}


