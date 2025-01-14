use std::{io, iter};

use crate::{reader::record::cigar::op, record::cigar::Op};

/// Raw SAM record CIGAR operations.
#[derive(Debug, Eq, PartialEq)]
pub struct Cigar<'a>(&'a [u8]);

impl<'a> Cigar<'a> {
    pub(super) fn new(src: &'a [u8]) -> Self {
        Self(src)
    }

    /// Returns whether there are any CIGAR operations.
    pub fn is_empty(&self) -> bool {
        self.0.is_empty()
    }

    /// Returns an iterator over CIGAR operations.
    pub fn iter(&self) -> impl Iterator<Item = Result<Op, op::ParseError>> + '_ {
        use crate::reader::record::cigar::op::parse_op;

        let mut src = self.0;

        iter::from_fn(move || {
            if src.is_empty() {
                None
            } else {
                Some(parse_op(&mut src))
            }
        })
    }
}

impl<'a> AsRef<[u8]> for Cigar<'a> {
    fn as_ref(&self) -> &[u8] {
        self.0
    }
}

impl<'a> TryFrom<Cigar<'a>> for crate::record::Cigar {
    type Error = io::Error;

    fn try_from(Cigar(src): Cigar<'a>) -> Result<Self, Self::Error> {
        use crate::reader::record::parse_cigar;

        let mut cigar = crate::record::Cigar::default();

        if !src.is_empty() {
            parse_cigar(src, &mut cigar)
                .map_err(|e| io::Error::new(io::ErrorKind::InvalidData, e))?;
        }

        Ok(cigar)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_iter() -> Result<(), op::ParseError> {
        use crate::record::cigar::op::Kind;

        let cigar = Cigar::new(b"");
        assert!(cigar.iter().next().is_none());

        let cigar = Cigar::new(b"8M13N");
        let actual: Vec<_> = cigar.iter().collect::<Result<_, _>>()?;
        let expected = [Op::new(Kind::Match, 8), Op::new(Kind::Skip, 13)];
        assert_eq!(actual, expected);

        Ok(())
    }
}
