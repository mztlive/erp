//! 数据库执行器抽象
//!
//! Repository 的每个方法都接收一个执行器，由 Service 决定该次数据访问是否位于事务中。
//! 执行器承载会话及可核验的事务、读取阶段边界；事务生命周期由统一设施管理。

mod boundary;
pub use boundary::ExecutionBoundary;
pub(crate) use boundary::ExecutionScope;
use mongodb::ClientSession;

pub(crate) mod sealed {
    /// 只有持久化设施能够实现执行器，外部不能伪造会话与边界对应关系。
    pub trait Sealed {}
}

/// 仅持久化设施能够构造的会话访问许可。
///
/// 对外取得原始会话会永久撤销该执行器的快照证明；统一读写操作使用本许可。
pub struct SessionAccess(());

/// 在统一读写封装中使用会话，写操作必须先推进读取阶段。
pub(crate) fn session(executor: &mut dyn Executor) -> Option<&mut ClientSession> {
    executor.scoped_session(SessionAccess(()))
}

/// 数据访问执行器。
///
/// 实现者向 Repository 暴露本次操作应当使用的 MongoDB 会话：
/// 会话本身不构成活动快照证明；授权只使用设施签发的 boundary。
/// 业务和测试通过本库的执行器及窄 Port 替身接入，不自行实现本 trait。
pub trait Executor: Send + sealed::Sealed {
    /// 取得当前可核验的执行边界。
    /// # 参数
    /// 无。
    /// # 返回
    /// 未经设施管理的执行器返回 None，不能据此跳过复验。
    /// # 错误
    /// 无。
    fn boundary(&self) -> Option<ExecutionBoundary> {
        None
    }

    /// 开始下一读取阶段，或在任何写入尝试前使旧读取凭证失效。
    /// # 参数
    /// 无。
    /// # 返回
    /// 无返回值；事务身份保持不变。
    /// # 错误
    /// 无。
    fn next_read(&mut self) {}

    /// 供持久化设施执行受控的会话操作。
    /// # 参数
    /// access 仅由统一读写封装签发。
    /// # 返回
    /// 当前操作的会话。
    /// # 错误
    /// 无。
    #[doc(hidden)]
    fn scoped_session(&mut self, _: SessionAccess) -> Option<&mut ClientSession> {
        self.session()
    }
    /// 返回本次操作应当加入的 MongoDB 会话。
    ///
    /// # 参数
    /// 无。
    ///
    /// # 返回
    /// 事务执行器返回会话并撤销边界证明；非事务执行器返回 None。
    /// 仓储须使用统一读写封装，不能取得原始会话后继续复用授权凭证。
    ///
    /// # 错误
    /// 不返回错误。
    fn session(&mut self) -> Option<&mut ClientSession>;
}

/// 单次非事务读取用例的执行器；query、读取和 validate 必须复用此实例。
#[derive(Debug, Default)]
pub struct ReadExecutor {
    scope: ExecutionScope,
}

impl sealed::Sealed for ReadExecutor {}

impl Executor for ReadExecutor {
    fn session(&mut self) -> Option<&mut ClientSession> {
        None
    }

    fn boundary(&self) -> Option<ExecutionBoundary> {
        Some(self.scope.boundary())
    }

    fn next_read(&mut self) {
        self.scope.advance();
    }
}

/// 只在已开启快照事务的回调中存在；不得向业务暴露构造函数。
pub(crate) struct TransactionExecutor<'a> {
    session: &'a mut ClientSession,
    scope: Option<ExecutionScope>,
}

impl<'a> TransactionExecutor<'a> {
    /// 调用方已经以统一快照选项开启事务。
    pub(crate) fn new(session: &'a mut ClientSession) -> Self {
        Self { session, scope: Some(ExecutionScope::snapshot()) }
    }
}

impl sealed::Sealed for TransactionExecutor<'_> {}

impl Executor for TransactionExecutor<'_> {
    fn boundary(&self) -> Option<ExecutionBoundary> {
        self.scope.as_ref().map(ExecutionScope::boundary)
    }

    fn next_read(&mut self) {
        if let Some(scope) = &mut self.scope {
            scope.advance();
        }
    }

    fn session(&mut self) -> Option<&mut ClientSession> {
        // 原始会话允许结束或重启事务，此后不能再声称处于原快照。
        self.scope = None;
        Some(self.session)
    }

    fn scoped_session(&mut self, _: SessionAccess) -> Option<&mut ClientSession> {
        Some(self.session)
    }
}

/// 非事务执行器。
///
/// 用于单集合读写等不需要跨集合原子性的场景，操作按 MongoDB 自动提交语义执行。
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct NoTransaction;

impl sealed::Sealed for NoTransaction {}

impl Executor for NoTransaction {
    /// 返回空会话，表示本次操作不加入任何事务。
    ///
    /// # 参数
    /// 无。
    ///
    /// # 返回
    /// 恒为 `None`。
    ///
    /// # 错误
    /// 不返回错误。
    fn session(&mut self) -> Option<&mut ClientSession> {
        None
    }
}

impl sealed::Sealed for ClientSession {}

impl Executor for ClientSession {
    /// 返回自身作为本次操作的事务会话。
    ///
    /// 原始会话可用于独立数据访问，但不提供活动事务或读取阶段证明。
    /// `with_transaction` 回调使用设施管理的执行器，不直接暴露本实现。
    ///
    /// # 参数
    /// 无。
    ///
    /// # 返回
    /// 恒为 `Some(self)`。
    ///
    /// # 错误
    /// 不返回错误。
    fn session(&mut self) -> Option<&mut ClientSession> {
        Some(self)
    }
}

impl<E: Executor + ?Sized> sealed::Sealed for &mut E {}

impl<E> Executor for &mut E
where
    E: Executor + ?Sized,
{
    fn boundary(&self) -> Option<ExecutionBoundary> {
        (**self).boundary()
    }

    fn next_read(&mut self) {
        (**self).next_read();
    }

    fn scoped_session(&mut self, access: SessionAccess) -> Option<&mut ClientSession> {
        (**self).scoped_session(access)
    }
    /// 透传被借用执行器的会话。
    ///
    /// # 参数
    /// 无。
    ///
    /// # 返回
    /// 返回内层执行器的会话。
    ///
    /// # 错误
    /// 不返回错误。
    fn session(&mut self) -> Option<&mut ClientSession> {
        (**self).session()
    }
}

#[cfg(test)]
mod tests {
    use super::{Executor, NoTransaction, ReadExecutor};

    #[test]
    fn no_transaction_executor_never_provides_session() {
        let mut executor = NoTransaction;

        assert!(executor.session().is_none());
    }

    #[test]
    fn borrowed_executor_forwards_inner_session() {
        let mut inner = NoTransaction;
        let mut borrowed: &mut dyn Executor = &mut inner;

        assert!(Executor::session(&mut borrowed).is_none());
    }

    #[test]
    fn read_boundaries_bind_instances_and_reject_the_next_stage() {
        let mut executor = ReadExecutor::default();
        let boundary = executor.boundary().unwrap();
        assert!(!boundary.is_snapshot());
        assert!(boundary.is_current(&executor));
        assert!(!boundary.is_current(&ReadExecutor::default()));
        assert!(!boundary.is_current(&NoTransaction));
        let mut borrowed: &mut dyn Executor = &mut executor;
        assert!(boundary.is_current(&borrowed));
        Executor::next_read(&mut borrowed);
        assert!(!boundary.is_current(&executor));
        assert!(executor.boundary().unwrap().is_current(&executor));
    }

    #[test]
    fn moved_read_executor_keeps_its_stage_until_the_owner_is_dropped() {
        let executor = ReadExecutor::default();
        let proof = executor.boundary().unwrap();
        let moved = Box::new(executor);
        assert!(proof.is_current(moved.as_ref()));
        drop(moved);
        assert!(!proof.is_current(&ReadExecutor::default()));
    }
}
