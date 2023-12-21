use super::ResourcePath;

impl serde::Serialize for ResourcePath {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: serde::Serializer,
    {
        let string: String = self.clone().into();
        serializer.serialize_str(string.as_str())
    }
}

impl<'de> serde::Deserialize<'de> for ResourcePath {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: serde::Deserializer<'de>,
    {
        deserializer.deserialize_str(ResourcePathVisitor)
    }
}

struct ResourcePathVisitor;

impl<'de> serde::de::Visitor<'de> for ResourcePathVisitor {
    type Value = ResourcePath;

    fn expecting(&self, formatter: &mut std::fmt::Formatter) -> std::fmt::Result {
        formatter.write_str("a path-like string with no file extension")
    }

    fn visit_str<E>(self, v: &str) -> Result<Self::Value, E>
    where
        E: serde::de::Error,
    {
        Self::Value::try_from(v).map_err(|err| serde::de::Error::custom(err))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[derive(
        serde::Serialize, serde::Deserialize, Debug, dm::From, dm::Constructor, Clone, PartialEq, Eq,
    )]
    struct Test {
        path: ResourcePath,
    }

    #[derive(
        serde::Serialize, serde::Deserialize, Debug, dm::From, dm::Constructor, Clone, PartialEq, Eq,
    )]
    struct TestDe {
        path: String,
    }

    #[test]
    fn json_serialize() {
        let rpath = ResourcePath::try_from("i/love/serde/owo").unwrap();

        let string = serde_json::to_string(&Test::new(rpath)).unwrap();

        assert_eq!(
            TestDe::new("i/love/serde/owo".into()),
            serde_json::from_str::<TestDe>(string.as_str()).unwrap()
        )
    }

    #[test]
    fn json_deserialize() {
        let json = r#"{"path": "silly/little/path"}"#;

        let test = serde_json::from_str::<Test>(json).unwrap();
        assert_eq!(
            ResourcePath::try_from("silly/little/path").unwrap(),
            test.path
        )
    }
}
