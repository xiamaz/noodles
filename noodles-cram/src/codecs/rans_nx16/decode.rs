mod order_0;
mod order_1;
pub mod pack;
mod rle;

use std::{
    io::{self, Cursor, Read},
    num::NonZeroUsize,
};

use byteorder::{LittleEndian, ReadBytesExt};

use super::Flags;
use crate::reader::num::read_uint7;

pub fn decode<R>(reader: &mut R, mut len: usize) -> io::Result<Vec<u8>>
where
    R: Read,
{
    let flags = reader.read_u8().map(Flags::from)?;

    if !flags.contains(Flags::NO_SIZE) {
        len = read_uint7(reader).and_then(|n| {
            usize::try_from(n).map_err(|e| io::Error::new(io::ErrorKind::InvalidData, e))
        })?;
    }

    let n = if flags.contains(Flags::N32) { 32 } else { 4 };

    if flags.contains(Flags::STRIPE) {
        return rans_decode_stripe(reader, len, n);
    }

    let mut p = None;
    let mut n_sym = None;
    let pack_len = len;

    if flags.contains(Flags::PACK) {
        let (q, n, new_len) = decode_pack_meta(reader)?;
        p = Some(q);
        n_sym = Some(n);
        len = new_len;
    }

    let mut l = None;
    let mut rle_meta = None;
    let rle_len = len;

    if flags.contains(Flags::RLE) {
        let (m, meta, new_len) = decode_rle_meta(reader, n)?;
        l = Some(m);
        rle_meta = Some(meta);
        len = new_len;
    }

    let mut data = vec![0; len];

    if flags.contains(Flags::CAT) {
        reader.read_exact(&mut data)?;
    } else if flags.contains(Flags::ORDER) {
        order_1::decode(reader, &mut data, n)?;
    } else {
        order_0::decode(reader, &mut data, n)?;
    };

    if flags.contains(Flags::RLE) {
        let l = l.unwrap();
        let mut rle_meta = rle_meta.unwrap();
        data = rle::decode(&data, &l, &mut rle_meta, rle_len)?;
    }

    if flags.contains(Flags::PACK) {
        let p = p.unwrap();
        let n_sym = n_sym.unwrap();
        data = pack::decode(&data, &p, n_sym, pack_len)?;
    }

    Ok(data)
}

fn read_alphabet<R>(reader: &mut R) -> io::Result<[bool; 256]>
where
    R: Read,
{
    let mut alphabet = [false; 256];

    let mut sym = reader.read_u8()?;
    let mut last_sym = sym;
    let mut rle = 0;

    loop {
        alphabet[usize::from(sym)] = true;

        if rle > 0 {
            rle -= 1;
            sym += 1;
        } else {
            sym = reader.read_u8()?;

            if last_sym < 255 && sym == last_sym + 1 {
                rle = reader.read_u8()?;
            }
        }

        last_sym = sym;

        if sym == 0 {
            break;
        }
    }

    Ok(alphabet)
}

fn rans_get_cumulative_freq_nx16(r: u32, bits: u32) -> u32 {
    r & ((1 << bits) - 1)
}

fn rans_get_symbol_from_freq(cumulative_freqs: &[u32], freq: u32) -> u8 {
    let mut sym = 0;

    while sym < 255 && freq >= cumulative_freqs[(sym + 1) as usize] {
        sym += 1;
    }

    sym
}

fn rans_advance_step_nx16(r: u32, c: u32, f: u32, bits: u32) -> u32 {
    f * (r >> bits) + (r & ((1 << bits) - 1)) - c
}

pub fn rans_renorm_nx16<R>(reader: &mut R, mut r: u32) -> io::Result<u32>
where
    R: Read,
{
    if r < (1 << 15) {
        r = (r << 16) + reader.read_u16::<LittleEndian>().map(u32::from)?;
    }

    Ok(r)
}

fn rans_decode_stripe<R>(reader: &mut R, len: usize, n: u32) -> io::Result<Vec<u8>>
where
    R: Read,
{
    let x = reader.read_u8().map(usize::from)?;
    let mut clens = Vec::with_capacity(x);

    for _ in 0..x {
        let clen = read_uint7(reader).and_then(|n| {
            usize::try_from(n).map_err(|e| io::Error::new(io::ErrorKind::InvalidData, e))
        })?;

        clens.push(clen);
    }

    let mut ulens = Vec::with_capacity(x);
    let mut t = Vec::with_capacity(x);

    for j in 0..x {
        let mut ulen = len / x;

        if len % (n as usize) > j {
            ulen += 1;
        }

        let chunk = decode(reader, ulen)?;

        ulens.push(ulen);
        t.push(chunk);
    }

    let mut dst = vec![0; len];

    for j in 0..x {
        for i in 0..ulens[j] {
            dst[i * (n as usize) + j] = t[j][i];
        }
    }

    Ok(dst)
}

fn decode_rle_meta<R>(reader: &mut R, n: u32) -> io::Result<([bool; 256], Cursor<Vec<u8>>, usize)>
where
    R: Read,
{
    let rle_meta_len = read_uint7(reader).and_then(|n| {
        usize::try_from(n).map_err(|e| io::Error::new(io::ErrorKind::InvalidData, e))
    })?;

    let len = read_uint7(reader).and_then(|n| {
        usize::try_from(n).map_err(|e| io::Error::new(io::ErrorKind::InvalidData, e))
    })?;

    let rle_meta = if rle_meta_len & 1 == 1 {
        let mut buf = vec![0; rle_meta_len / 2];
        reader.read_exact(&mut buf)?;
        buf
    } else {
        let comp_meta_len = read_uint7(reader).and_then(|n| {
            usize::try_from(n).map_err(|e| io::Error::new(io::ErrorKind::InvalidData, e))
        })?;

        let mut buf = vec![0; comp_meta_len];
        reader.read_exact(&mut buf)?;

        let mut buf_reader = &buf[..];
        let mut dst = vec![0; rle_meta_len / 2];
        order_0::decode(&mut buf_reader, &mut dst, n)?;

        dst
    };

    let mut rle_meta_reader = Cursor::new(rle_meta);

    let mut m = rle_meta_reader.read_u8().map(u16::from)?;

    if m == 0 {
        m = 256;
    }

    let mut l = [false; 256];

    for _ in 0..m {
        let s = rle_meta_reader.read_u8()?;
        l[usize::from(s)] = true;
    }

    Ok((l, rle_meta_reader, len))
}

pub fn decode_pack_meta<R>(reader: &mut R) -> io::Result<(Vec<u8>, NonZeroUsize, usize)>
where
    R: Read,
{
    // n_sym
    let symbol_count = reader.read_u8().and_then(|n| {
        NonZeroUsize::try_from(usize::from(n))
            .map_err(|e| io::Error::new(io::ErrorKind::InvalidData, e))
    })?;

    let mut p = vec![0; symbol_count.get()];
    reader.read_exact(&mut p)?;

    let len = read_uint7(reader).and_then(|n| {
        usize::try_from(n).map_err(|e| io::Error::new(io::ErrorKind::InvalidData, e))
    })?;

    Ok((p, symbol_count, len))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_decode_order_0() -> io::Result<()> {
        let data = [
            0x00, // flags = {empty}
            0x07, // uncompressed len = 7
            0x64, 0x65, 0x00, 0x6c, 0x6e, 0x6f, 0x00, 0x73, 0x00, 0x01, 0x01, 0x01, 0x01, 0x03,
            0x01, 0x00, 0x26, 0x20, 0x00, 0x00, 0xb8, 0x0a, 0x00, 0x00, 0xd8, 0x0a, 0x00, 0x00,
            0x00, 0x04, 0x00,
        ];
        let mut reader = &data[..];

        assert_eq!(decode(&mut reader, 0)?, b"noodles");

        Ok(())
    }

    #[test]
    fn test_decode_order_1() -> io::Result<()> {
        let data = [
            0x01, // flags = ORDER
            0x07, // uncompressed len = 7
            0xc0, 0x00, 0x64, 0x65, 0x00, 0x6c, 0x6e, 0x6f, 0x00, 0x73, 0x00, 0x00, 0x00, 0x01,
            0x00, 0x01, 0x01, 0x02, 0x00, 0x00, 0x00, 0x02, 0x01, 0x00, 0x02, 0x00, 0x05, 0x01,
            0x00, 0x01, 0x01, 0x00, 0x03, 0x00, 0x04, 0x01, 0x00, 0x00, 0x00, 0x00, 0x01, 0x00,
            0x02, 0x01, 0x00, 0x00, 0x01, 0x00, 0x05, 0x00, 0x04, 0x02, 0x00, 0x00, 0x08, 0x01,
            0x00, 0x00, 0x08, 0x01, 0x00, 0x00, 0x00, 0x02, 0x00,
        ];

        let mut reader = &data[..];
        assert_eq!(decode(&mut reader, 0)?, b"noodles");

        Ok(())
    }

    #[test]
    fn test_decode_stripe() -> io::Result<()> {
        let data = [
            0x08, // flags = STRIPE
            0x07, // uncompressed len = 7
            0x04, 0x17, 0x17, 0x17, 0x15, 0x00, 0x02, 0x6c, 0x6e, 0x00, 0x01, 0x01, 0x00, 0x08,
            0x01, 0x00, 0x00, 0x00, 0x01, 0x00, 0x00, 0x80, 0x00, 0x00, 0x00, 0x80, 0x00, 0x00,
            0x00, 0x02, 0x65, 0x6f, 0x00, 0x01, 0x01, 0x00, 0x08, 0x01, 0x00, 0x00, 0x00, 0x01,
            0x00, 0x00, 0x80, 0x00, 0x00, 0x00, 0x80, 0x00, 0x00, 0x00, 0x02, 0x6f, 0x73, 0x00,
            0x01, 0x01, 0x00, 0x00, 0x01, 0x00, 0x00, 0x08, 0x01, 0x00, 0x00, 0x80, 0x00, 0x00,
            0x00, 0x80, 0x00, 0x00, 0x00, 0x01, 0x64, 0x00, 0x01, 0x00, 0x80, 0x00, 0x00, 0x00,
            0x80, 0x00, 0x00, 0x00, 0x80, 0x00, 0x00, 0x00, 0x80, 0x00, 0x00, 0x00, 0x02, 0x00,
            0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x22, 0x00, 0x81, 0x11, 0x01, 0x7f, 0x00,
        ];

        let mut reader = &data[..];
        assert_eq!(decode(&mut reader, 0)?, b"noodles");

        Ok(())
    }

    #[test]
    fn test_decode_uncompressed() -> io::Result<()> {
        let data = [
            0x20, // flags = CAT
            0x07, // uncompressed len = 7
            0x6e, 0x6f, 0x6f, 0x64, 0x6c, 0x65, 0x73,
        ];

        let mut reader = &data[..];
        assert_eq!(decode(&mut reader, 0)?, b"noodles");

        Ok(())
    }

    #[test]
    fn test_decode_rle() -> io::Result<()> {
        let data = [
            0x40, // flags = RLE
            0x0d, // uncompressed len = 13
            0x06, 0x06, 0x17, 0x01, 0x07, 0x6f, 0x00, 0x02, 0x01, 0x01, 0x00, 0x00, 0x01, 0x00,
            0x00, 0x0c, 0x02, 0x00, 0x00, 0x08, 0x02, 0x00, 0x00, 0x80, 0x00, 0x00, 0x64, 0x65,
            0x00, 0x6c, 0x6e, 0x6f, 0x00, 0x73, 0x00, 0x03, 0x01, 0x01, 0x01, 0x01, 0x01, 0x00,
            0x3a, 0x20, 0x00, 0x00, 0x7c, 0x20, 0x00, 0x00, 0x52, 0x01, 0x00, 0x00, 0x08, 0x04,
            0x00,
        ];

        let mut reader = &data[..];
        assert_eq!(decode(&mut reader, 0)?, b"noooooooodles");

        Ok(())
    }

    #[test]
    fn test_decode_bit_packing_with_6_symbols() -> io::Result<()> {
        let data = [
            0x80, // flags = PACK
            0x07, // uncompressed len = 7
            0x06, 0x64, 0x65, 0x6c, 0x6e, 0x6f, 0x73, 0x04, 0x04, 0x05, 0x00, 0x12, 0x43, 0x00,
            0x01, 0x01, 0x01, 0x01, 0x00, 0x0c, 0x02, 0x00, 0x00, 0x00, 0x02, 0x00, 0x00, 0x08,
            0x02, 0x00, 0x00, 0x04, 0x02, 0x00,
        ];

        let mut reader = &data[..];
        assert_eq!(decode(&mut reader, 0)?, b"noodles");

        Ok(())
    }
}
