use bson::Decimal128;
use serde::{Deserialize, Serialize};

pub struct Amount;

impl Serialize for Amount {
    fn serialize<S: Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        let _ = Decimal128::from_str("1.00");
        serializer.serialize_str("1.00")
    }
}
