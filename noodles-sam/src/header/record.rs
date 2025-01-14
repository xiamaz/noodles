//! SAM header record.

pub mod kind;
pub mod value;

pub use self::kind::Kind;

use self::value::{
    map::{self, Program, ReadGroup, ReferenceSequence},
    Map,
};

/// A SAM header record.
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum Record {
    /// A header (`HD`) record.
    Header(Map<map::Header>),
    /// A reference sequence (`SQ`) record.
    ReferenceSequence(map::reference_sequence::Name, Map<ReferenceSequence>),
    /// A read group (`RG`) record.
    ReadGroup(String, Map<ReadGroup>),
    /// A program (`PG`) record.
    Program(String, Map<Program>),
    /// A comment (`CO`) record.
    Comment(String),
}
