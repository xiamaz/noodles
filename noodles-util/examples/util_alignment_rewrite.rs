//! Rewrites an alignment format to another alignment format.
//!
//! The output format is determined from the extension of the destination.

use std::{env, fs::File, io, path::Path};

use noodles_util::alignment::{self, Format};

fn detect_format_from_extension<P>(path: P) -> Option<Format>
where
    P: AsRef<Path>,
{
    match path.as_ref().extension().and_then(|ext| ext.to_str()) {
        Some("sam") => Some(Format::Sam),
        Some("bam") => Some(Format::Bam),
        Some("cram") => Some(Format::Cram),
        _ => None,
    }
}

fn main() -> io::Result<()> {
    let mut args = env::args().skip(1);

    let src = args.next().expect("missing src");
    let dst = args.next().expect("missing dst");

    let mut reader = File::open(src).and_then(|f| alignment::Reader::builder(f).build())?;
    let header = reader.read_header()?;

    let output_format = detect_format_from_extension(&dst).expect("invalid dst extension");

    let mut writer = File::create(dst).map(|f| {
        alignment::Writer::builder(f)
            .set_format(output_format)
            .build()
    })?;

    writer.write_header(&header)?;

    for result in reader.records(&header) {
        let record = result?;
        writer.write_record(&header, &record)?;
    }

    Ok(())
}