use std::fmt;
use serde::{Serialize, Serializer};
use serde::de::{Deserialize, Deserializer, Visitor, Error as DeError};
use InlinableString;

impl Serialize for InlinableString {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error> 
        where S: Serializer
    {
        serializer.serialize_str(self)
    }
}

impl<'de> Deserialize<'de> for InlinableString {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
        where D: Deserializer<'de>
    {
        struct InlinableStringVisitor;

        impl<'de> Visitor<'de> for InlinableStringVisitor {
            type Value = InlinableString;

            fn expecting(&self, formatter: &mut fmt::Formatter) -> fmt::Result {
                formatter.write_str("a string")
            }

            fn visit_str<E>(self, v: &str) -> Result<Self::Value, E>
                where E: DeError
            {
                Ok(v.into())
            }
        }

        deserializer.deserialize_any(InlinableStringVisitor)
    }
}

#[cfg(test)]
mod tests {
    use InlinableString;
    use serde_test::{Token, assert_tokens};

    #[test]
    fn test_ser_de() {
        let s = InlinableString::from("small");

        assert_tokens(&s, &[Token::String("small")]);
    }
}