use std::fmt;
use std::marker::PhantomData;

use crate::reexports::serde;
use crate::Bloom;

use serde::{
    de::{Error as DeError, Visitor},
    Deserializer, Serializer,
};

pub fn serialize<Ser: Serializer, T: ?Sized>(
    bloom: &Bloom<T>,
    serializer: Ser,
) -> Result<Ser::Ok, Ser::Error> {
    serializer.serialize_bytes(bloom.as_slice())
}

struct BloomVisitor<T: ?Sized> {
    _phantom: PhantomData<T>,
}

impl<T: ?Sized> Visitor<'_> for BloomVisitor<T> {
    type Value = Bloom<T>;

    fn expecting(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str("Blom filter")
    }

    fn visit_bytes<E>(self, v: &[u8]) -> Result<Self::Value, E>
    where
        E: DeError,
    {
        Bloom::from_slice(v).map_err(E::custom)
    }

    fn visit_byte_buf<E>(self, v: Vec<u8>) -> Result<Self::Value, E>
    where
        E: DeError,
    {
        Bloom::from_bytes(v).map_err(E::custom)
    }
}

pub fn deserialize<'de, D: Deserializer<'de>, T: ?Sized>(
    deserializer: D,
) -> Result<Bloom<T>, D::Error> {
    deserializer.deserialize_bytes(BloomVisitor {
        _phantom: PhantomData,
    })
}
