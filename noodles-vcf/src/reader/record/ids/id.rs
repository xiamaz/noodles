use std::{error, fmt};

use noodles_core as core;

use crate::record::ids::Id;

/// An error when a raw VCF record ID fails to parse.
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum ParseError {
    /// The input is empty.
    Empty,
    /// The input is invalid.
    Invalid,
}

impl error::Error for ParseError {}

impl fmt::Display for ParseError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Empty => write!(f, "empty input"),
            Self::Invalid => write!(f, "invalid input"),
        }
    }
}

impl From<ParseError> for core::Error {
    fn from(e: ParseError) -> Self {
        Self::new(core::error::Kind::Parse, e)
    }
}

pub(super) fn parse_id(s: &str) -> Result<Id, ParseError> {
    if s.is_empty() {
        Err(ParseError::Empty)
    } else if is_valid_id(s) {
        Ok(Id(s.into()))
    } else {
        Err(ParseError::Invalid)
    }
}

fn is_valid_id(s: &str) -> bool {
    s.chars().all(|c| !c.is_whitespace())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_parse_id() -> Result<(), crate::record::ids::id::ParseError> {
        assert_eq!(parse_id("nd0"), Ok("nd0".parse()?));

        assert_eq!(parse_id(""), Err(ParseError::Empty));
        assert_eq!(parse_id("nd 0"), Err(ParseError::Invalid));

        Ok(())
    }
}
