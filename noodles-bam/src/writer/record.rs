use std::io::{self, Write};

use byteorder::{LittleEndian, WriteBytesExt};
use noodles_sam as sam;

use super::sam_record::NULL_QUALITY_SCORE;
use crate::{
    record::{Cigar, ReferenceSequenceId},
    Record,
};

pub(super) fn write_record<W>(writer: &mut W, record: &Record) -> io::Result<()>
where
    W: Write,
{
    let block_size = u32::try_from(record.block_size())
        .map_err(|e| io::Error::new(io::ErrorKind::InvalidInput, e))?;
    writer.write_u32::<LittleEndian>(block_size)?;

    // ref_id
    write_reference_sequence_id(writer, record.reference_sequence_id())?;

    // pos
    write_position(writer, record.pos)?;

    write_l_read_name(writer, record.read_name())?;

    // mapq
    write_mapping_quality(writer, record.mapping_quality())?;

    writer.write_u16::<LittleEndian>(record.bin())?;

    let n_cigar_op = u16::try_from(record.cigar().len())
        .map_err(|e| io::Error::new(io::ErrorKind::InvalidInput, e))?;
    writer.write_u16::<LittleEndian>(n_cigar_op)?;

    let flag = u16::from(record.flags());
    writer.write_u16::<LittleEndian>(flag)?;

    let l_seq = u32::try_from(record.sequence().len())
        .map_err(|e| io::Error::new(io::ErrorKind::InvalidInput, e))?;
    writer.write_u32::<LittleEndian>(l_seq)?;

    // next_ref_id
    write_reference_sequence_id(writer, record.mate_reference_sequence_id())?;

    // next_pos
    write_position(writer, record.next_pos)?;

    writer.write_i32::<LittleEndian>(record.template_length())?;

    write_read_name(writer, record.read_name())?;

    write_cigar(writer, record.cigar())?;

    let sequence = record.sequence();
    let quality_scores = record.quality_scores();

    writer.write_all(sequence.as_ref())?;

    if sequence.len() == quality_scores.len() {
        writer.write_all(record.quality_scores().as_ref())?;
    } else if quality_scores.is_empty() {
        for _ in 0..sequence.len() {
            writer.write_u8(NULL_QUALITY_SCORE)?;
        }
    } else {
        return Err(io::Error::new(
            io::ErrorKind::InvalidInput,
            format!(
                "quality scores length mismatch: expected {}, got {}",
                sequence.len(),
                quality_scores.len()
            ),
        ));
    }

    writer.write_all(record.data().as_ref())?;

    Ok(())
}

fn write_reference_sequence_id<W>(
    writer: &mut W,
    reference_sequence_id: Option<ReferenceSequenceId>,
) -> io::Result<()>
where
    W: Write,
{
    use crate::record::reference_sequence_id::UNMAPPED;

    let ref_id = reference_sequence_id.map(i32::from).unwrap_or(UNMAPPED);
    writer.write_i32::<LittleEndian>(ref_id)
}

// pos is 0-based.
fn write_position<W>(writer: &mut W, pos: i32) -> io::Result<()>
where
    W: Write,
{
    writer.write_i32::<LittleEndian>(pos)
}

fn write_l_read_name<W>(writer: &mut W, read_name: &[u8]) -> io::Result<()>
where
    W: Write,
{
    let len = u8::try_from(read_name.len())
        .map_err(|e| io::Error::new(io::ErrorKind::InvalidInput, e))?;

    let l_read_name = len
        .checked_add(1)
        .ok_or_else(|| io::Error::new(io::ErrorKind::InvalidInput, "invalid l_read_name"))?;

    writer.write_u8(l_read_name)
}

fn write_mapping_quality<W>(
    writer: &mut W,
    mapping_quality: Option<sam::record::MappingQuality>,
) -> io::Result<()>
where
    W: Write,
{
    use sam::record::mapping_quality::MISSING;
    let mapq = mapping_quality.map(u8::from).unwrap_or(MISSING);
    writer.write_u8(mapq)
}

fn write_read_name<W>(writer: &mut W, read_name: &[u8]) -> io::Result<()>
where
    W: Write,
{
    const NUL: u8 = 0x00;

    writer.write_all(read_name)?;
    writer.write_all(&[NUL])?;

    Ok(())
}

fn write_cigar<W>(writer: &mut W, cigar: &Cigar) -> io::Result<()>
where
    W: Write,
{
    for &raw_op in cigar.as_ref() {
        writer.write_u32::<LittleEndian>(raw_op)?;
    }

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_write_record_with_default_fields() -> Result<(), Box<dyn std::error::Error>> {
        let mut buf = Vec::new();
        let record = Record::default();
        write_record(&mut buf, &record)?;

        let expected = [
            0x22, 0x00, 0x00, 0x00, // block_size = 34
            0xff, 0xff, 0xff, 0xff, // ref_id = -1
            0xff, 0xff, 0xff, 0xff, // pos = -1
            0x02, // l_read_name = 2
            0xff, // mapq = 255
            0x48, 0x12, // bin = 4680
            0x00, 0x00, // n_cigar_op = 0
            0x04, 0x00, // flag = 4
            0x00, 0x00, 0x00, 0x00, // l_seq = 0
            0xff, 0xff, 0xff, 0xff, // next_ref_id = -1
            0xff, 0xff, 0xff, 0xff, // next_pos = -1
            0x00, 0x00, 0x00, 0x00, // tlen = 0
            0x2a, 0x00, // read_name = "*\x00"
        ];

        assert_eq!(buf, expected);

        Ok(())
    }

    #[test]
    fn test_write_record_with_all_fields() -> Result<(), Box<dyn std::error::Error>> {
        use sam::record::{
            cigar::op::Kind, data::field::Tag, quality_scores::Score, Flags, MappingQuality,
            Position,
        };

        use crate::record::{
            cigar::Op,
            data::{field::Value, Field},
            sequence::Base,
            Cigar, Data, QualityScores, ReferenceSequenceId, Sequence,
        };

        let reference_sequence_id = ReferenceSequenceId::try_from(1)?;

        let record = Record::builder()
            .set_reference_sequence_id(reference_sequence_id)
            .set_position(Position::try_from(9)?)
            .set_mapping_quality(MappingQuality::try_from(13)?)
            .set_flags(Flags::SEGMENTED | Flags::FIRST_SEGMENT)
            .set_mate_reference_sequence_id(reference_sequence_id)
            .set_mate_position(Position::try_from(22)?)
            .set_template_length(144)
            .set_read_name(b"r0".to_vec())
            .set_cigar(Cigar::from(vec![
                Op::new(Kind::Match, 36)?,
                Op::new(Kind::SoftClip, 8)?,
            ]))
            .set_sequence(Sequence::from(vec![Base::A, Base::C, Base::G, Base::T]))
            .set_quality_scores(QualityScores::from(vec![
                Score::try_from('N')?,
                Score::try_from('D')?,
                Score::try_from('L')?,
                Score::try_from('S')?,
            ]))
            .set_data(Data::try_from(vec![Field::new(
                Tag::AlignmentHitCount,
                Value::UInt8(1),
            )])?)
            .build()?;

        let mut buf = Vec::new();
        write_record(&mut buf, &record)?;

        let expected = [
            0x35, 0x00, 0x00, 0x00, // block_size = 53
            0x01, 0x00, 0x00, 0x00, // ref_id = 1
            0x08, 0x00, 0x00, 0x00, // pos = 8
            0x03, // l_read_name = 3
            0x0d, // mapq = 13
            0x49, 0x12, // bin = 4681
            0x02, 0x00, // n_cigar_op = 2
            0x41, 0x00, // flag = 65
            0x04, 0x00, 0x00, 0x00, // l_seq = 4
            0x01, 0x00, 0x00, 0x00, // next_ref_id = 1
            0x15, 0x00, 0x00, 0x00, // next_pos = 21
            0x90, 0x00, 0x00, 0x00, // tlen = 144
            b'r', b'0', 0x00, // read_name = "r0\x00"
            0x40, 0x02, 0x00, 0x00, // cigar[0] = 36M
            0x84, 0x00, 0x00, 0x00, // cigar[1] = 8S
            0x12, 0x48, // seq = ACGT
            0x2d, 0x23, 0x2b, 0x32, // qual = NDLS
            b'N', b'H', b'C', 0x01, // data[0] = NH:i:1
        ];

        assert_eq!(buf, expected);

        Ok(())
    }
}
