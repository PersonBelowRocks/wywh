use std::marker::PhantomData;

use super::rotations::{BlockModelFace, BlockModelFaceMap};

impl<T: serde::Serialize> serde::Serialize for BlockModelFaceMap<T> {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: serde::Serializer,
    {
        use serde::ser::SerializeMap;

        let mut sermap = serializer.serialize_map(Some(self.len()))?;

        self.map(|face, value| sermap.serialize_entry(&face, value));

        sermap.end()
    }
}

impl<'de, T: serde::Deserialize<'de>> serde::Deserialize<'de> for BlockModelFaceMap<T> {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: serde::Deserializer<'de>,
    {
        deserializer.deserialize_map(BlockModelFaceMapVisitor(PhantomData))
    }
}

struct BlockModelFaceMapVisitor<T>(PhantomData<T>);

impl<'de, T: serde::Deserialize<'de>> serde::de::Visitor<'de> for BlockModelFaceMapVisitor<T> {
    type Value = BlockModelFaceMap<T>;

    fn expecting(&self, formatter: &mut std::fmt::Formatter) -> std::fmt::Result {
        formatter.write_str("a map with keyed with the faces of a cube")
    }

    fn visit_map<A>(self, mut map: A) -> Result<Self::Value, A::Error>
    where
        A: serde::de::MapAccess<'de>,
    {
        let mut out = BlockModelFaceMap::<T>::new();

        while let Some((face, value)) = map.next_entry::<BlockModelFace, T>()? {
            out.set(face, value);
        }

        Ok(out)
    }
}
