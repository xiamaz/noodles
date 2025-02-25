mod header;
mod query;

use std::str;

use futures::{stream, Stream};
use noodles_bgzf as bgzf;
use noodles_core::Region;
use noodles_csi as csi;
use tokio::io::{self, AsyncBufRead, AsyncBufReadExt, AsyncRead, AsyncSeek};

use self::{header::read_header, query::query};
use crate::{lazy, reader::resolve_region, Header, Record};

const LINE_FEED: char = '\n';
const CARRIAGE_RETURN: char = '\r';

/// An async VCF reader.
///
/// The VCF format has two main parts: 1) a header and 2) a list of VCF records.
///
/// Each header line is prefixed with a `#` (number sign) and is terminated by the header header
/// (`#CHROM`...; inclusive).
///
/// VCF records are line-based and follow directly after the header until EOF.
///
/// # Examples
///
/// ```no_run
/// # #[tokio::main]
/// # async fn main() -> std::io::Result<()> {
/// use futures::TryStreamExt;
/// use noodles_vcf as vcf;
/// use tokio::{fs::File, io::BufReader};
///
/// let mut reader = File::open("sample.vcf")
///     .await
///     .map(BufReader::new)
///     .map(vcf::AsyncReader::new)?;
///
/// let header = reader.read_header().await?;
///
/// let mut records = reader.records(&header);
///
/// while let Some(record) = records.try_next().await? {
///     // ...
/// }
/// # Ok(())
/// # }
/// ```
pub struct Reader<R> {
    inner: R,
    buf: String,
}

impl<R> Reader<R>
where
    R: AsyncBufRead + Unpin,
{
    /// Creates an async VCF reader.
    ///
    /// # Examples
    ///
    /// ```
    /// use noodles_vcf as vcf;
    /// let data = [];
    /// let reader = vcf::AsyncReader::new(&data[..]);
    /// ```
    pub fn new(inner: R) -> Self {
        Self {
            inner,
            buf: String::new(),
        }
    }

    /// Reads the VCF header.
    ///
    /// This reads all header lines prefixed with a `#` (number sign), which includes the header
    /// header (`#CHROM`...), and parses it as a [`crate::Header`].
    ///
    /// The position of the stream is expected to be at the start.
    ///
    /// # Examples
    ///
    /// ```
    /// # use std::io;
    /// #
    /// # #[tokio::main]
    /// # async fn main() -> io::Result<()> {
    /// use noodles_vcf as vcf;
    ///
    /// let data = b"##fileformat=VCFv4.3
    /// #CHROM\tPOS\tID\tREF\tALT\tQUAL\tFILTER\tINFO
    /// sq0\t1\t.\tA\t.\t.\tPASS\t.
    /// ";
    ///
    /// let mut reader = vcf::AsyncReader::new(&data[..]);
    /// let header = reader.read_header().await?;
    /// # Ok(())
    /// # }
    /// ```
    pub async fn read_header(&mut self) -> io::Result<Header> {
        read_header(&mut self.inner).await
    }

    /// Reads a single VCF record.
    ///
    /// This reads a line from the underlying stream until a newline is reached and parses that
    /// line into the given record.
    ///
    /// The stream is expected to be directly after the header or at the start of another record.
    ///
    /// It is more ergonomic to read records using a stream (see [`Self::records`] and
    /// [`Self::query`]), but using this method allows control of the record buffer.
    ///
    /// If successful, the number of bytes read is returned. If the number of bytes read is 0, the
    /// stream reached EOF.
    ///
    /// # Examples
    ///
    /// ```
    /// # use std::io;
    /// #
    /// # #[tokio::main]
    /// # async fn main() -> io::Result<()> {
    /// use noodles_vcf as vcf;
    ///
    /// let data = b"##fileformat=VCFv4.3
    /// #CHROM\tPOS\tID\tREF\tALT\tQUAL\tFILTER\tINFO
    /// sq0\t1\t.\tA\t.\t.\tPASS\t.
    /// ";
    ///
    /// let mut reader = vcf::AsyncReader::new(&data[..]);
    /// let header = reader.read_header().await?;
    ///
    /// let mut record = vcf::Record::default();
    /// reader.read_record(&header, &mut record).await?;
    /// # Ok(())
    /// # }
    /// ```
    pub async fn read_record(&mut self, header: &Header, record: &mut Record) -> io::Result<usize> {
        use crate::reader::parse_record;

        self.buf.clear();

        match read_line(&mut self.inner, &mut self.buf).await? {
            0 => Ok(0),
            n => {
                parse_record(&self.buf, header, record)
                    .map_err(|e| io::Error::new(io::ErrorKind::InvalidData, e))?;

                Ok(n)
            }
        }
    }

    /// Reads a single record without eagerly parsing its fields.
    ///
    /// The reads VCF record fields from the underlying stream into the given record's buffer until
    /// a newline is reached. No fields are parsed, meaning the record is not necessarily valid.
    /// However, the structure of the line is guaranteed to be record-like.
    ///
    /// The stream is expected to be directly after the header or at the start of another record.
    ///
    /// If successful, the number of bytes read is returned. If the number of bytes read is 0, the
    /// stream reached EOF.
    ///
    /// # Examples
    ///
    /// ```
    /// # #[tokio::main]
    /// # async fn main() -> std::io::Result<()> {
    /// use noodles_vcf as vcf;
    ///
    /// let data = b"##fileformat=VCFv4.3
    /// #CHROM\tPOS\tID\tREF\tALT\tQUAL\tFILTER\tINFO
    /// sq0\t1\t.\tA\t.\t.\tPASS\t.
    /// ";
    ///
    /// let mut reader = vcf::AsyncReader::new(&data[..]);
    /// reader.read_header().await?;
    ///
    /// let mut record = vcf::lazy::Record::default();
    /// reader.read_lazy_record(&mut record).await?;
    /// # Ok::<_, std::io::Error>(())
    /// # }
    /// ```
    pub async fn read_lazy_record(&mut self, record: &mut lazy::Record) -> io::Result<usize> {
        read_lazy_record(&mut self.inner, record).await
    }

    /// Returns an (async) stream over records starting from the current (input) stream position.
    ///
    /// The (input) stream is expected to be directly after the header or at the start of another
    /// record.
    ///
    /// Unlike [`Self::read_record`], each record is parsed as a [`Record`].
    ///
    /// # Examples
    ///
    /// ```
    /// # #[tokio::main]
    /// # async fn main() -> std::io::Result<()> {
    /// use futures::TryStreamExt;
    /// use noodles_vcf as vcf;
    ///
    /// let data = b"##fileformat=VCFv4.3
    /// #CHROM\tPOS\tID\tREF\tALT\tQUAL\tFILTER\tINFO
    /// sq0\t1\t.\tA\t.\t.\tPASS\t.
    /// ";
    ///
    /// let mut reader = vcf::AsyncReader::new(&data[..]);
    /// let header = reader.read_header().await?;
    ///
    /// let mut records = reader.records(&header);
    ///
    /// while let Some(record) = records.try_next().await? {
    ///     // ...
    /// }
    /// # Ok(())
    /// # }
    /// ```
    pub fn records<'r, 'h: 'r>(
        &'r mut self,
        header: &'h Header,
    ) -> impl Stream<Item = io::Result<Record>> + 'r {
        Box::pin(stream::try_unfold(self, move |reader| async move {
            let mut record = Record::default();

            reader
                .read_record(header, &mut record)
                .await
                .map(|n| match n {
                    0 => None,
                    _ => Some((record, reader)),
                })
        }))
    }
}

impl<R> Reader<bgzf::AsyncReader<R>>
where
    R: AsyncRead,
{
    /// Returns the current virtual position of the underlying BGZF reader.
    ///
    /// # Examples
    ///
    /// ```
    /// # use std::io;
    /// use noodles_bgzf as bgzf;
    /// use noodles_vcf as vcf;
    ///
    /// let data = [];
    /// let reader = vcf::AsyncReader::new(bgzf::AsyncReader::new(&data[..]));
    ///
    /// assert_eq!(reader.virtual_position(), bgzf::VirtualPosition::default());
    /// # Ok::<(), io::Error>(())
    /// ```
    pub fn virtual_position(&self) -> bgzf::VirtualPosition {
        self.inner.virtual_position()
    }
}

impl<R> Reader<bgzf::AsyncReader<R>>
where
    R: AsyncRead + AsyncSeek + Unpin,
{
    /// Seeks the underlying BGZF stream to the given virtual position.
    ///
    /// Virtual positions typically come from an associated index.
    ///
    /// # Examples
    ///
    /// ```
    /// # use std::io::{self, Cursor};
    /// #
    /// # #[tokio::main]
    /// # async fn main() -> io::Result<()> {
    /// use noodles_bgzf as bgzf;
    /// use noodles_vcf as vcf;
    ///
    /// let data = Cursor::new([]);
    /// let mut reader = vcf::AsyncReader::new(bgzf::AsyncReader::new(data));
    ///
    /// let virtual_position = bgzf::VirtualPosition::default();
    /// reader.seek(virtual_position).await?;
    /// # Ok(())
    /// # }
    /// ```
    pub async fn seek(&mut self, pos: bgzf::VirtualPosition) -> io::Result<bgzf::VirtualPosition> {
        self.inner.seek(pos).await
    }

    /// Returns a stream over records that intersects the given region.
    ///
    /// The position of the (input) stream is expected to after the header or at the start of
    /// another record.
    ///
    /// # Examples
    ///
    /// ```no_run
    /// # #[tokio::main]
    /// # async fn main() -> Result<(), Box<dyn std::error::Error>> {
    /// use futures::TryStreamExt;
    /// use noodles_bgzf as bgzf;
    /// use noodles_core::Region;
    /// use noodles_tabix as tabix;
    /// use noodles_vcf as vcf;
    /// use tokio::fs::File;
    ///
    /// let mut reader = File::open("sample.vcf.gz")
    ///     .await
    ///     .map(bgzf::AsyncReader::new)
    ///     .map(vcf::AsyncReader::new)?;
    ///
    /// let header = reader.read_header().await?;
    ///
    /// let index = tabix::read("sample.vcf.gz.tbi")?;
    /// let region = "sq0:8-13".parse()?;
    /// let mut query = reader.query(&header, &index, &region)?;
    ///
    /// while let Some(record) = query.try_next().await? {
    ///     // ...
    /// }
    /// # Ok(())
    /// # }
    /// ```
    pub fn query<'r>(
        &'r mut self,
        header: &'r Header,
        index: &csi::Index,
        region: &Region,
    ) -> io::Result<impl Stream<Item = io::Result<Record>> + 'r> {
        let (reference_sequence_id, reference_sequence_name) = resolve_region(index, region)?;

        let chunks = index.query(reference_sequence_id, region.interval())?;

        Ok(query(
            self,
            chunks,
            reference_sequence_name,
            region.interval(),
            header,
        ))
    }
}

async fn read_line<R>(reader: &mut R, buf: &mut String) -> io::Result<usize>
where
    R: AsyncBufRead + Unpin,
{
    match reader.read_line(buf).await {
        Ok(0) => Ok(0),
        Ok(n) => {
            if buf.ends_with(LINE_FEED) {
                buf.pop();

                if buf.ends_with(CARRIAGE_RETURN) {
                    buf.pop();
                }
            }

            Ok(n)
        }
        Err(e) => Err(e),
    }
}

async fn read_lazy_record<R>(reader: &mut R, record: &mut lazy::Record) -> io::Result<usize>
where
    R: AsyncBufRead + Unpin,
{
    record.buf.clear();

    let mut len = 0;

    len += read_field(reader, &mut record.buf).await?;
    record.bounds.chromosome_end = record.buf.len();

    len += read_field(reader, &mut record.buf).await?;
    record.bounds.position_end = record.buf.len();

    len += read_field(reader, &mut record.buf).await?;
    record.bounds.ids_end = record.buf.len();

    len += read_field(reader, &mut record.buf).await?;
    record.bounds.reference_bases_end = record.buf.len();

    len += read_field(reader, &mut record.buf).await?;
    record.bounds.alternate_bases_end = record.buf.len();

    len += read_field(reader, &mut record.buf).await?;
    record.bounds.quality_score_end = record.buf.len();

    len += read_field(reader, &mut record.buf).await?;
    record.bounds.filters_end = record.buf.len();

    len += read_field(reader, &mut record.buf).await?;
    record.bounds.info_end = record.buf.len();

    len += read_line(reader, &mut record.buf).await?;

    Ok(len)
}

async fn read_field<R>(reader: &mut R, dst: &mut String) -> io::Result<usize>
where
    R: AsyncBufRead + Unpin,
{
    const DELIMITER: u8 = b'\t';

    let mut is_delimiter = false;
    let mut len = 0;

    loop {
        let src = reader.fill_buf().await?;

        if is_delimiter || src.is_empty() {
            break;
        }

        let (buf, n) = match src.iter().position(|&b| b == DELIMITER) {
            Some(i) => {
                is_delimiter = true;
                (&src[..i], i + 1)
            }
            None => (src, src.len()),
        };

        let s = str::from_utf8(buf).map_err(|e| io::Error::new(io::ErrorKind::InvalidData, e))?;
        dst.push_str(s);

        len += n;

        reader.consume(n);
    }

    Ok(len)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn test_read_line() -> io::Result<()> {
        async fn t(buf: &mut String, mut data: &[u8], expected: &str) -> io::Result<()> {
            buf.clear();
            read_line(&mut data, buf).await?;
            assert_eq!(buf, expected);
            Ok(())
        }

        let mut buf = String::new();

        t(&mut buf, b"noodles\n", "noodles").await?;
        t(&mut buf, b"noodles\r\n", "noodles").await?;
        t(&mut buf, b"noodles", "noodles").await?;

        Ok(())
    }

    #[tokio::test]
    async fn test_read_lazy_record() -> io::Result<()> {
        let mut src = &b"sq0\t1\t.\tA\t.\t.\tPASS\t."[..];

        let mut record = lazy::Record::default();
        read_lazy_record(&mut src, &mut record).await?;

        assert_eq!(record.buf, "sq01.A..PASS.");

        assert_eq!(record.bounds.chromosome_end, 3);
        assert_eq!(record.bounds.position_end, 4);
        assert_eq!(record.bounds.ids_end, 5);
        assert_eq!(record.bounds.reference_bases_end, 6);
        assert_eq!(record.bounds.alternate_bases_end, 7);
        assert_eq!(record.bounds.quality_score_end, 8);
        assert_eq!(record.bounds.filters_end, 12);
        assert_eq!(record.bounds.info_end, 13);

        Ok(())
    }
}
