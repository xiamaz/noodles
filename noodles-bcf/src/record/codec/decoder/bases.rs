use std::io;

use noodles_vcf::record::{AlternateBases, ReferenceBases};

use super::read_value;
use crate::lazy::record::Value;

pub(crate) fn read_ref_alt(
    src: &mut &[u8],
    len: usize,
) -> io::Result<(ReferenceBases, AlternateBases)> {
    let mut alleles = Vec::with_capacity(len);

    for _ in 0..len {
        match read_value(src).map_err(|e| io::Error::new(io::ErrorKind::InvalidData, e))? {
            Some(Value::String(Some(s))) => alleles.push(s.into()),
            Some(Value::String(None)) => alleles.push(String::from(".")),
            v => {
                return Err(io::Error::new(
                    io::ErrorKind::InvalidData,
                    format!("invalid ref_alt: expected string, got {v:?}"),
                ))
            }
        }
    }

    let (raw_reference_bases, raw_alternate_bases) = alleles.split_at(1);

    let reference_bases = raw_reference_bases
        .first()
        .ok_or_else(|| io::Error::new(io::ErrorKind::InvalidInput, "missing reference bases"))
        .and_then(|s| {
            s.parse()
                .map_err(|e| io::Error::new(io::ErrorKind::InvalidInput, e))
        })?;

    let alternate_bases = raw_alternate_bases
        .iter()
        .map(|s| {
            s.parse()
                .map_err(|e| io::Error::new(io::ErrorKind::InvalidInput, e))
        })
        .collect::<Result<Vec<_>, _>>()
        .map(AlternateBases::from)?;

    Ok((reference_bases, alternate_bases))
}
