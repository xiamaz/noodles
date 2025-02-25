mod field;

use std::{error, fmt};

use noodles_vcf as vcf;

pub(crate) use self::field::read_field;
use crate::header::string_maps::StringStringMap;

pub fn read_info(
    src: &mut &[u8],
    infos: &vcf::header::Infos,
    string_string_map: &StringStringMap,
    len: usize,
) -> Result<vcf::record::Info, DecodeError> {
    let mut info = vcf::record::Info::default();

    for _ in 0..len {
        let (key, value) =
            read_field(src, infos, string_string_map).map_err(DecodeError::InvalidField)?;

        if info.insert(key.clone(), value).is_some() {
            return Err(DecodeError::DuplicateKey(key));
        }
    }

    Ok(info)
}

#[derive(Debug, Eq, PartialEq)]
pub enum DecodeError {
    InvalidField(field::DecodeError),
    DuplicateKey(vcf::record::info::field::Key),
}

impl error::Error for DecodeError {
    fn source(&self) -> Option<&(dyn error::Error + 'static)> {
        match self {
            Self::InvalidField(e) => Some(e),
            Self::DuplicateKey(_) => None,
        }
    }
}

impl fmt::Display for DecodeError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::InvalidField(_) => write!(f, "invalid field"),
            Self::DuplicateKey(key) => write!(f, "duplicate key: {key}"),
        }
    }
}
