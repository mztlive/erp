use bson::{doc, Document};

pub struct CustomerAccount;

impl CustomerAccount {
    pub fn to_document(&self) -> Document {
        doc! { "id": "x" }
    }
}
