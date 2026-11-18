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

// 统一读写封装借会话时保留当前执行边界；写入方必须先推进读取阶段，再借会话。
pub(crate) fn session(executor: &mut dyn Executor) -> Option<&mut ClientSession> {
    executor.scoped_session(SessionAccess(()))
}

/// 数据访问执行器。
///
/// 实现者向 Repository 暴露本次操作应当使用的 MongoDB 会话：
/// 会话本身不构成活动快照证明；授权只使用设施签发的 boundary。
/// 业务和测试通过本库的执行器及窄 Port 替身接入，不自行实现本 trait。
pub trait Executor: Send + sealed::Sealed {
    /// 取得授权读取和结果复验可比对的当前事务或读取阶段。无受管阶段时返回 `None`，不能凭它跳过复验。
    fn boundary(&self) -> Option<ExecutionBoundary> {
        None
    }

    /// 开始新的读取阶段，或在尝试写入前让所有旧凭证失效；不因此结束或重启当前事务。
    fn next_read(&mut self) {}

    /// 统一持久化操作持内部许可借用会话，避免仅因读操作就撤销受管执行器的边界。业务仓储应使用读写封装。
    #[doc(hidden)]
    fn scoped_session(&mut self, _: SessionAccess) -> Option<&mut ClientSession> {
        self.session()
    }
    /// 直接取得操作应加入的原始 MongoDB 会话；对受管事务执行器，这会永久撤销当前边界证明。
    /// 仓储应使用统一封装，不能拿到原始会话后继续复用授权凭证。
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
    // 事务设施用已经开启快照事务的会话建立执行器和首个读取阶段；不要以普通会话直接签发快照证明。
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
    // 独立自动提交操作不附加驱动会话，也不提供事务边界。
    fn session(&mut self) -> Option<&mut ClientSession> {
        None
    }
}

impl sealed::Sealed for ClientSession {}

impl Executor for ClientSession {
    // 原始会话可用于独立数据访问，返回自身但不证明事务仍活动或阶段一致；统一事务回调用受管执行器。
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
    // 借用执行器仍沿用内层会话及其暴露语义，不另建会话。
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
