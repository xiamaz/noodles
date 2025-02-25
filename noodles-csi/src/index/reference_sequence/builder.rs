use std::collections::HashMap;

use noodles_bgzf as bgzf;
use noodles_core::Position;

use super::{
    bin::{self, Chunk},
    Bin, Metadata, ReferenceSequence,
};

/// A CSI reference sequence builder.
#[derive(Debug)]
pub struct Builder {
    bin_builders: HashMap<usize, bin::Builder>,
    linear_index: Vec<Option<bgzf::VirtualPosition>>,
    start_position: bgzf::VirtualPosition,
    end_position: bgzf::VirtualPosition,
    mapped_record_count: u64,
    unmapped_record_count: u64,
}

impl Builder {
    /// Adds a record.
    pub fn add_record(
        &mut self,
        min_shift: u8,
        depth: u8,
        start: Position,
        end: Position,
        is_mapped: bool,
        chunk: Chunk,
    ) {
        self.update_bins(min_shift, depth, start, end, chunk);
        self.update_linear_index(start, end, chunk);
        self.update_metadata(is_mapped, chunk);
    }

    /// Builds a CSI reference sequence.
    pub fn build(mut self) -> ReferenceSequence {
        use super::parent_id;

        if self.bin_builders.is_empty() {
            return ReferenceSequence::new(Default::default(), Vec::new(), None);
        }

        let builders: Vec<_> = self
            .bin_builders
            .iter()
            .map(|(id, builder)| (*id, builder.loffset))
            .collect();

        for (mut id, loffset) in builders {
            while let Some(pid) = parent_id(id) {
                if let Some(builder) = self.bin_builders.get_mut(&pid) {
                    if loffset < builder.loffset {
                        builder.loffset = loffset;
                    }
                }

                id = pid;
            }
        }

        let bins = self
            .bin_builders
            .into_iter()
            .map(|(id, builder)| (id, builder.build()))
            .collect();

        let linear_index = self
            .linear_index
            .into_iter()
            .map(|p| p.unwrap_or_default())
            .collect();

        let metadata = Metadata::new(
            self.start_position,
            self.end_position,
            self.mapped_record_count,
            self.unmapped_record_count,
        );

        ReferenceSequence::new(bins, linear_index, Some(metadata))
    }

    fn update_bins(
        &mut self,
        min_shift: u8,
        depth: u8,
        start: Position,
        end: Position,
        chunk: Chunk,
    ) {
        use super::reg2bin;

        let bin_id = reg2bin(start, end, min_shift, depth);
        let builder = self.bin_builders.entry(bin_id).or_insert(Bin::builder());
        builder.add_chunk(chunk);
    }

    fn update_linear_index(&mut self, start: Position, end: Position, chunk: Chunk) {
        use super::LINEAR_INDEX_WINDOW_SIZE;

        let linear_index_start_offset = (usize::from(start) - 1) / LINEAR_INDEX_WINDOW_SIZE;
        let linear_index_end_offset = (usize::from(end) - 1) / LINEAR_INDEX_WINDOW_SIZE;

        if linear_index_end_offset >= self.linear_index.len() {
            self.linear_index.resize(linear_index_end_offset + 1, None);
        }

        for i in linear_index_start_offset..=linear_index_end_offset {
            self.linear_index[i].get_or_insert(chunk.start());
        }
    }

    fn update_metadata(&mut self, is_mapped: bool, chunk: Chunk) {
        if is_mapped {
            self.mapped_record_count += 1;
        } else {
            self.unmapped_record_count += 1;
        }

        self.start_position = self.start_position.min(chunk.start());
        self.end_position = self.end_position.max(chunk.end());
    }
}

impl Default for Builder {
    fn default() -> Self {
        Self {
            bin_builders: HashMap::new(),
            linear_index: Vec::new(),
            start_position: bgzf::VirtualPosition::MAX,
            end_position: bgzf::VirtualPosition::MIN,
            mapped_record_count: 0,
            unmapped_record_count: 0,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_build() -> Result<(), Box<dyn std::error::Error>> {
        const MIN_SHIFT: u8 = 14;
        const DEPTH: u8 = 5;

        let mut builder = Builder::default();

        builder.add_record(
            MIN_SHIFT,
            DEPTH,
            Position::try_from(8)?,
            Position::try_from(13)?,
            true,
            Chunk::new(
                bgzf::VirtualPosition::from(0),
                bgzf::VirtualPosition::from(9),
            ),
        );

        builder.add_record(
            MIN_SHIFT,
            DEPTH,
            Position::try_from(121393)?,
            Position::try_from(196418)?,
            false,
            Chunk::new(
                bgzf::VirtualPosition::from(9),
                bgzf::VirtualPosition::from(3473408),
            ),
        );

        let actual = builder.build();

        let expected = {
            let bins = [
                (
                    4681,
                    Bin::new(
                        bgzf::VirtualPosition::from(0),
                        vec![Chunk::new(
                            bgzf::VirtualPosition::from(0),
                            bgzf::VirtualPosition::from(9),
                        )],
                    ),
                ),
                (
                    73,
                    Bin::new(
                        bgzf::VirtualPosition::from(0),
                        vec![Chunk::new(
                            bgzf::VirtualPosition::from(9),
                            bgzf::VirtualPosition::from(3473408),
                        )],
                    ),
                ),
            ]
            .into_iter()
            .collect();

            let linear_index = vec![
                bgzf::VirtualPosition::from(0),
                bgzf::VirtualPosition::from(0),
                bgzf::VirtualPosition::from(0),
                bgzf::VirtualPosition::from(0),
                bgzf::VirtualPosition::from(0),
                bgzf::VirtualPosition::from(0),
                bgzf::VirtualPosition::from(0),
                bgzf::VirtualPosition::from(9),
                bgzf::VirtualPosition::from(9),
                bgzf::VirtualPosition::from(9),
                bgzf::VirtualPosition::from(9),
                bgzf::VirtualPosition::from(9),
            ];

            let metadata = Metadata::new(
                bgzf::VirtualPosition::from(0),
                bgzf::VirtualPosition::from(3473408),
                1,
                1,
            );

            ReferenceSequence::new(bins, linear_index, Some(metadata))
        };

        for (id, expected_bin) in expected.bins() {
            let actual_bin = actual.bins().get(id).expect("missing bin");
            assert_eq!(actual_bin, expected_bin);
        }

        assert_eq!(actual.linear_index(), expected.linear_index());
        assert_eq!(actual.metadata(), expected.metadata());

        Ok(())
    }
}
