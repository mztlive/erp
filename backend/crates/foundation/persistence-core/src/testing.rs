//! 执行合同的无数据库替身。仅供 dev-dependency 启用，不能用于生产数据访问。
use mongodb::ClientSession;

use crate::executor::{ExecutionScope, sealed};
use crate::{ExecutionBoundary, Executor};

/// 模拟同一会话开启事务及阶段切换；不提供 MongoDB 会话或真实事务保证。
pub struct SnapshotExecutor {
    scope: Option<ExecutionScope>,
}

impl Default for SnapshotExecutor {
    fn default() -> Self {
        Self { scope: Some(ExecutionScope::snapshot()) }
    }
}

impl SnapshotExecutor {
    /// 在测试中模拟切换到下一笔快照事务，验证旧事务签发的边界凭证全部失效。
    pub fn restart(&mut self) {
        self.scope = Some(ExecutionScope::snapshot());
    }
}

impl sealed::Sealed for SnapshotExecutor {}

impl Executor for SnapshotExecutor {
    fn session(&mut self) -> Option<&mut ClientSession> {
        self.scope = None;
        None
    }

    fn boundary(&self) -> Option<ExecutionBoundary> {
        self.scope.as_ref().map(ExecutionScope::boundary)
    }

    fn next_read(&mut self) {
        if let Some(scope) = &mut self.scope {
            scope.advance();
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn restarting_exposing_session_and_dropping_revoke_every_cloned_boundary() {
        let mut executor = SnapshotExecutor::default();
        let first = executor.boundary().unwrap();
        let copied = first.clone();
        executor.restart();
        assert!(!first.is_snapshot());
        assert!(!copied.is_current(&executor));

        let second = executor.boundary().unwrap();
        assert!(second.is_current(&executor));
        executor.session();
        assert!(!second.is_snapshot());
        assert!(!second.is_current(&executor));

        executor.restart();
        let third = executor.boundary().unwrap();
        drop(executor);
        assert!(!third.is_snapshot());
    }
}
