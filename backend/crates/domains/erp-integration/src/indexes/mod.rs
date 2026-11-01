//! 集成事实、供应商消息及技术凭据的索引入口；既有集合创建顺序保持不变。

mod integration_ops;
mod supplier_callback;
pub mod supplier_credential;
mod supplier_message;

pub use integration_ops::ensure;
