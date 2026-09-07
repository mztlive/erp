//! 四个集成事实集合的冻结索引；顺序保持入站、错误任务、差异、决定。

mod integration_ops;

pub use integration_ops::ensure;
