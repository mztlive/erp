use std::sync::{Arc, Weak};

use super::Executor;

#[derive(Debug, Clone, Copy)]
enum Mode {
    Read,
    Snapshot,
}

/// 执行器独占阶段的存活权；凭证不能延长阶段或事务的生命周期。
#[derive(Debug)]
pub(crate) struct ExecutionScope {
    stage: Arc<Mode>,
}

impl Default for ExecutionScope {
    /// 为一次非事务读取创建独立阶段。
    fn default() -> Self {
        Self { stage: Arc::new(Mode::Read) }
    }
}

impl ExecutionScope {
    /// 仅在统一设施已开启快照事务后创建。
    pub(crate) fn snapshot() -> Self {
        Self { stage: Arc::new(Mode::Snapshot) }
    }

    /// 签发只读弱引用，无法由凭证重新构造执行器。
    pub(crate) fn boundary(&self) -> ExecutionBoundary {
        ExecutionBoundary { stage: Arc::downgrade(&self.stage) }
    }

    /// 丢弃旧阶段的唯一强引用，使全部旧凭证同时失效。
    pub(crate) fn advance(&mut self) {
        self.stage = Arc::new(*self.stage);
    }
}

/// 当前事务或读取阶段的凭证；复制凭证不能保活或恢复旧阶段。
#[derive(Debug, Clone)]
pub struct ExecutionBoundary {
    stage: Weak<Mode>,
}

impl ExecutionBoundary {
    /// 判断凭证是否仍属于存活的快照事务阶段。
    /// # 参数
    /// 无。
    /// # 返回
    /// 阶段推进、会话泄露或事务退出后返回 false。
    /// # 错误
    /// 无。
    pub fn is_snapshot(&self) -> bool {
        self.stage.upgrade().is_some_and(|mode| matches!(*mode, Mode::Snapshot))
    }

    /// 核对原事务及读取阶段，不访问数据库。
    /// # 参数
    /// executor 为持久化设施提供的结果使用执行器。
    /// # 返回
    /// 当前执行器仍持有签发凭证的阶段时返回 true。
    /// # 错误
    /// 无；已失效或其他执行器的边界返回 false。
    pub fn is_current(&self, executor: &dyn Executor) -> bool {
        self.stage.strong_count() > 0
            && executor.boundary().is_some_and(|current| Weak::ptr_eq(&self.stage, &current.stage))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn cloned_boundaries_expire_on_write_and_cannot_keep_a_transaction_alive() {
        let mut scope = ExecutionScope::snapshot();
        let first = scope.boundary();
        let copied = first.clone();
        assert!(first.is_snapshot());
        scope.advance();
        assert!(!first.is_snapshot());
        assert!(!copied.is_snapshot());
        let current = scope.boundary();
        assert!(current.is_snapshot());
        drop(scope);
        assert!(!current.is_snapshot());
    }
}
