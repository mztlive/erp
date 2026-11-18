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
    // 非事务授权查询开始时创建独立阶段，使其他执行器不能共享该读凭证。
    fn default() -> Self {
        Self { stage: Arc::new(Mode::Read) }
    }
}

impl ExecutionScope {
    // 统一事务设施已开启快照事务后创建阶段；普通会话不能据此充当快照证明。
    pub(crate) fn snapshot() -> Self {
        Self { stage: Arc::new(Mode::Snapshot) }
    }

    // 为授权读取签发当前阶段的弱引用，复制凭证既不能保活阶段，也不能据此重建执行器。
    pub(crate) fn boundary(&self) -> ExecutionBoundary {
        ExecutionBoundary { stage: Arc::downgrade(&self.stage) }
    }

    // 用新阶段替换唯一强引用，使旧授权读取凭证一起失效，同时保留快照或非事务的模式。
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
    /// 判断授权证明是否仍指向存活的快照阶段；阶段推进、会话暴露或事务退出后，原凭证不再有效。
    pub fn is_snapshot(&self) -> bool {
        self.stage.upgrade().is_some_and(|mode| matches!(*mode, Mode::Snapshot))
    }

    /// 在交付查询结果前核对原阶段是否仍由所给执行器持有；不用访问数据库，过期或其他实例的边界均不匹配。
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
