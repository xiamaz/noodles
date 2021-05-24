use std::{
    convert::TryFrom,
    io::{self, Write},
};

use noodles_vcf as vcf;

use crate::{
    header::StringMap,
    record::value::{Float, Int16, Int32, Int8, Value},
    writer::{string_map::write_string_map_index, value::write_value},
};

pub fn write_info<W>(
    writer: &mut W,
    string_map: &StringMap,
    info: &vcf::record::Info,
) -> io::Result<()>
where
    W: Write,
{
    for field in info.values() {
        write_info_field_key(writer, string_map, field.key())?;
        write_info_field_value(writer, field.value())?;
    }

    Ok(())
}

fn write_info_field_key<W>(
    writer: &mut W,
    string_map: &StringMap,
    key: &vcf::record::info::field::Key,
) -> io::Result<()>
where
    W: Write,
{
    string_map
        .get_index_of(key.as_ref())
        .ok_or_else(|| {
            io::Error::new(
                io::ErrorKind::InvalidInput,
                format!("info key missing from string map: {:?}", key),
            )
        })
        .and_then(|i| write_string_map_index(writer, i))
}

fn write_info_field_value<W>(
    writer: &mut W,
    value: &vcf::record::info::field::Value,
) -> io::Result<()>
where
    W: Write,
{
    use vcf::record::info::field;

    match value {
        field::Value::Integer(n) => write_info_field_integer_value(writer, *n),
        field::Value::Float(n) => write_value(writer, Some(Value::Float(Some(Float::Value(*n))))),
        field::Value::Flag => write_value(writer, None),
        field::Value::String(s) => write_value(writer, Some(Value::String(Some(s.into())))),
        v => todo!("unhandled INFO field value: {:?}", v),
    }
}

fn write_info_field_integer_value<W>(writer: &mut W, n: i32) -> io::Result<()>
where
    W: Write,
{
    if let Ok(m) = i8::try_from(n) {
        write_value(writer, Some(Value::Int8(Some(Int8::Value(m)))))
    } else if let Ok(m) = i16::try_from(n) {
        write_value(writer, Some(Value::Int16(Some(Int16::Value(m)))))
    } else {
        write_value(writer, Some(Value::Int32(Some(Int32::Value(n)))))
    }
}

#[cfg(test)]
mod test {
    use super::*;

    #[test]
    fn test_write_info_field_value() -> io::Result<()> {
        use vcf::record::info::field;

        fn t(buf: &mut Vec<u8>, value: &field::Value, expected: &[u8]) -> io::Result<()> {
            buf.clear();
            write_info_field_value(buf, value)?;
            assert_eq!(buf, expected);
            Ok(())
        }

        let mut buf = Vec::new();

        let value = field::Value::Integer(8);
        t(&mut buf, &value, &[0x11, 0x08])?;

        let value = field::Value::Integer(144);
        t(&mut buf, &value, &[0x12, 0x90, 0x00])?;

        let value = field::Value::Integer(46368);
        t(&mut buf, &value, &[0x13, 0x20, 0xb5, 0x00, 0x00])?;

        Ok(())
    }
}
