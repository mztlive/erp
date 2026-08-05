//! 数据库执行器抽象
//!
//! Repository 的每个方法都接收一个执行器，由 Service 决定该次数据访问是否位于事务中。
//! 执行器只承载“用哪个 MongoDB 会话执行”这一件事，不负责启动、提交或回滚事务。

use mongodb::ClientSession;

/// 数据访问执行器。
///
/// 实现者向 Repository 暴露本次操作应当使用的 MongoDB 会话：
/// 返回 `Some` 时操作加入调用方事务，返回 `None` 时按自动提交方式独立执行。
pub trait Executor: Send {
    /// 返回本次操作应当加入的 MongoDB 会话。
    ///
    /// # 返回值
    /// 事务执行器返回 `Some(&mut ClientSession)`；非事务执行器返回 `None`。
    ///
    /// # 错误
    /// 无。
    fn session(&mut self) -> Option<&mut ClientSession>;
}

/// 非事务执行器。
///
/// 用于单集合读写等不需要跨集合原子性的场景，操作按 MongoDB 自动提交语义执行。
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct NoTransaction;

impl Executor for NoTransaction {
    /// 返回空会话，表示本次操作不加入任何事务。
    ///
    /// # 返回值
    /// 恒为 `None`。
    ///
    /// # 错误
    /// 无。
    fn session(&mut self) -> Option<&mut ClientSession> {
        None
    }
}

impl Executor for ClientSession {
    /// 返回自身作为本次操作的事务会话。
    ///
    /// 该实现使 `Transactional::with_transaction` 回调拿到的 `&mut ClientSession`
    /// 可以直接作为执行器传入 Repository。
    ///
    /// # 返回值
    /// 恒为 `Some(self)`。
    ///
    /// # 错误
    /// 无。
    fn session(&mut self) -> Option<&mut ClientSession> {
        Some(self)
    }
}

impl<E> Executor for &mut E
where
    E: Executor + ?Sized,
{
    /// 透传被借用执行器的会话。
    ///
    /// # 返回值
    /// 返回内层执行器的会话。
    ///
    /// # 错误
    /// 无。
    fn session(&mut self) -> Option<&mut ClientSession> {
        (**self).session()
    }
}

#[cfg(test)]
mod tests {
    use super::{Executor, NoTransaction};

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
}
