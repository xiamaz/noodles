use std::{
    ffi::{OsStr, OsString},
    fs::File,
    io::{self, Read},
    path::{Path, PathBuf},
};

use noodles_tabix as tabix;

use super::IndexedReader;

/// An indexed VCF reader builder.
#[derive(Default)]
pub struct Builder {
    index: Option<tabix::Index>,
}

impl Builder {
    /// Sets an index.
    ///
    /// # Examples
    ///
    /// ```
    /// use noodles_tabix as tabix;
    /// use noodles_vcf::indexed_reader::Builder;
    ///
    /// let index = tabix::Index::default();
    /// let builder = Builder::default().set_index(index);
    /// ```
    pub fn set_index(mut self, index: tabix::Index) -> Self {
        self.index = Some(index);
        self
    }

    /// Builds an indexed VCF reader from a path.
    ///
    /// # Examples
    ///
    /// ```no_run
    /// use noodles_vcf::indexed_reader::Builder;
    /// let reader = Builder::default().build_from_path("sample.vcf.gz")?;
    /// # Ok::<_, std::io::Error>(())
    /// ```
    pub fn build_from_path<P>(mut self, src: P) -> io::Result<IndexedReader<File>>
    where
        P: AsRef<Path>,
    {
        let src = src.as_ref();

        if self.index.is_none() {
            let index_src = build_index_src(src);
            self.index = tabix::read(index_src).map(Some)?;
        }

        let file = File::open(src)?;
        self.build_from_reader(file)
    }

    /// Builds an indexed VCF reader from a reader.
    ///
    /// # Examples
    ///
    /// ```
    /// use noodles_tabix as tabix;
    /// use noodles_vcf::indexed_reader::Builder;
    ///
    /// let index = tabix::Index::default();
    /// let data = [];
    /// let reader = Builder::default()
    ///     .set_index(index)
    ///     .build_from_reader(&data[..])?;
    /// # Ok::<_, std::io::Error>(())
    /// ```
    pub fn build_from_reader<R>(self, reader: R) -> io::Result<IndexedReader<R>>
    where
        R: Read,
    {
        let index = self
            .index
            .ok_or_else(|| io::Error::new(io::ErrorKind::InvalidInput, "missing index"))?;

        Ok(IndexedReader::new(reader, index))
    }
}

fn build_index_src<P>(src: P) -> PathBuf
where
    P: AsRef<Path>,
{
    const EXT: &str = "tbi";
    push_ext(src.as_ref().into(), EXT)
}

fn push_ext<S>(path: PathBuf, ext: S) -> PathBuf
where
    S: AsRef<OsStr>,
{
    let mut s = OsString::from(path);
    s.push(".");
    s.push(ext);
    PathBuf::from(s)
}

#[cfg(test)]
mod tests {
    use std::path::PathBuf;

    use super::*;

    #[test]
    fn test_build_index_src() {
        assert_eq!(
            build_index_src("sample.vcf.gz"),
            PathBuf::from("sample.vcf.gz.tbi")
        );
    }
}