//! Inner VCF header FORMAT map value.

pub(crate) mod definition;
pub(crate) mod tag;
mod ty;

pub use self::{tag::Tag, ty::Type};

use std::fmt;

use self::tag::StandardTag;
use super::{builder, Described, Indexed, Inner, Map, OtherFields, Typed};
use crate::{
    header::{FileFormat, Number},
    record::genotypes::keys::Key,
};

/// An inner VCF header format map value.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct Format {
    pub(crate) number: Number,
    pub(crate) ty: Type,
    pub(crate) description: String,
    pub(crate) idx: Option<usize>,
}

impl Inner for Format {
    type StandardTag = StandardTag;
    type Builder = builder::TypedDescribedIndexed<Self>;
}

impl Typed for Format {
    type Type = Type;

    fn number(&self) -> Number {
        self.number
    }

    fn number_mut(&mut self) -> &mut Number {
        &mut self.number
    }

    fn ty(&self) -> Self::Type {
        self.ty
    }

    fn type_mut(&mut self) -> &mut Self::Type {
        &mut self.ty
    }
}

impl Described for Format {
    fn description(&self) -> &str {
        &self.description
    }

    fn description_mut(&mut self) -> &mut String {
        &mut self.description
    }
}

impl Indexed for Format {
    fn idx(&self) -> Option<usize> {
        self.idx
    }

    fn idx_mut(&mut self) -> &mut Option<usize> {
        &mut self.idx
    }
}

impl Map<Format> {
    /// Creates a VCF header format map value.
    ///
    /// # Examples
    ///
    /// ```
    /// use noodles_vcf::{
    ///     header::{record::value::{map::{format::Type, Format}, Map}, Number},
    ///     record::genotypes::keys::key,
    /// };
    ///
    /// let id = key::GENOTYPE;
    /// let map = Map::<Format>::new(Number::Count(1), Type::String, "Genotype");
    /// ```
    pub fn new<D>(number: Number, ty: Type, description: D) -> Self
    where
        D: Into<String>,
    {
        Self {
            inner: Format {
                number,
                ty,
                description: description.into(),
                idx: None,
            },
            other_fields: OtherFields::new(),
        }
    }
}

impl fmt::Display for Map<Format> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        super::fmt_display_type_fields(f, self.number(), self.ty())?;
        super::fmt_display_description_field(f, self.description())?;
        super::fmt_display_other_fields(f, self.other_fields())?;

        if let Some(idx) = self.idx() {
            super::fmt_display_idx_field(f, idx)?;
        }

        Ok(())
    }
}

impl From<&Key> for Map<Format> {
    fn from(key: &Key) -> Self {
        Self::from((FileFormat::default(), key))
    }
}

impl From<(FileFormat, &Key)> for Map<Format> {
    fn from((file_format, key): (FileFormat, &Key)) -> Self {
        let (number, ty, description) =
            definition::definition(file_format, key).unwrap_or_default();

        Self {
            inner: Format {
                number,
                ty,
                description: description.into(),
                idx: None,
            },
            other_fields: OtherFields::new(),
        }
    }
}

impl builder::Inner<Format> for builder::TypedDescribedIndexed<Format> {
    fn build(self) -> Result<Format, builder::BuildError> {
        let number = self
            .number
            .ok_or(builder::BuildError::MissingField("Number"))?;

        let ty = self.ty.ok_or(builder::BuildError::MissingField("Type"))?;

        let description = self
            .description
            .ok_or(builder::BuildError::MissingField("Description"))?;

        Ok(Format {
            number,
            ty,
            description,
            idx: self.idx,
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::record::genotypes::keys::key;

    #[test]
    fn test_fmt() {
        let map = Map::<Format>::from(&key::GENOTYPE);
        let expected = r#",Number=1,Type=String,Description="Genotype""#;
        assert_eq!(map.to_string(), expected);
    }
}
