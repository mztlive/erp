//! XLSX 技术读取边界；模板列、公式准入和业务解释由调用方负责。

mod archive;
mod cells;
mod metadata;

#[cfg(test)]
pub(crate) use archive::MAX_XML_BYTES;
pub(crate) use archive::{Archive, ArchiveLimits};
pub(crate) use cells::{Cell, MergeRange, Row, SheetLimits, rows};
pub(crate) use metadata::{date1904, document, relationships, sheet_paths};
use roxmltree::Node;

use crate::Error;

fn invalid(message: impl Into<String>) -> Error {
    Error::ValidationError(message.into())
}

fn text(node: Node<'_, '_>) -> String {
    node.descendants().filter(|node| node.is_text()).filter_map(|node| node.text()).collect()
}

#[cfg(test)]
mod tests;
