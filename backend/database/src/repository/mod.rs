//! MongoDB仓储实现模块
//!
//! 提供基于MongoDB的数据访问层实现

pub mod extensions;

pub mod owned;

pub use extensions::DatabaseExt;
