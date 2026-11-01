//! 采购创建及选源批次的确定性审计事件顺序。

use crate::{Error, Result};

/// 一张采购单在原写入顺序中的提交与创建事件序号。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct PurchaseCreationEventSequence {
    submitted: u32,
    created: u32,
}

impl PurchaseCreationEventSequence {
    /// 独立创建命令先提交、后创建，不分配批次事件。
    /// # 参数
    /// 无。
    /// # 返回
    /// 返回提交为1、创建为2的已校验序号。
    /// # 错误
    /// 无。
    pub(crate) fn standalone() -> Self {
        Self { submitted: 1, created: 2 }
    }

    /// 返回先写入的提交事件序号。
    /// # 参数
    /// 无。
    /// # 返回
    /// 返回严格为正的提交序号。
    /// # 错误
    /// 无。
    pub(crate) fn submitted(self) -> u32 {
        self.submitted
    }

    /// 返回随后写入的创建事件序号。
    /// # 参数
    /// 无。
    /// # 返回
    /// 返回严格为正的创建序号。
    /// # 错误
    /// 无。
    pub(crate) fn created(self) -> u32 {
        self.created
    }
}

/// 首次业务写入前验证的整批事件序号边界。
#[derive(Debug, Clone, Copy)]
pub(crate) struct SourcingEventSequencePlan {
    order_count: usize,
    main: u32,
}

impl SourcingEventSequencePlan {
    /// 为全部采购单与最后批次主事件验证序号容量。
    /// # 参数
    /// * `order_count` - 精确拆分后的采购单数量。
    /// # 返回
    /// 返回不分配集合、不执行写入的序号计划。
    /// # 错误
    /// 采购数量无法表示或两类子事件加主事件溢出时拒绝。
    pub(crate) fn new(order_count: usize) -> Result<Self> {
        let count = u32::try_from(order_count).map_err(|_| sequence_overflow())?;
        let main =
            count.checked_mul(2).and_then(|value| value.checked_add(1)).ok_or_else(sequence_overflow)?;
        Ok(Self { order_count, main })
    }

    /// 按采购计划稳定索引取得先提交、后创建的已校验序号。
    /// # 参数
    /// * `index` - 从0开始的采购计划索引。
    /// # 返回
    /// 返回与原逐单写入顺序一致的序号对。
    /// # 错误
    /// 索引超出已验证采购计划或序号无法表示时拒绝。
    pub(crate) fn order(self, index: usize) -> Result<PurchaseCreationEventSequence> {
        if index >= self.order_count {
            return Err(Error::ValidationError("采购批次审计事件索引超出计划".into()));
        }
        let index = u32::try_from(index).map_err(|_| sequence_overflow())?;
        let submitted =
            index.checked_mul(2).and_then(|value| value.checked_add(1)).ok_or_else(sequence_overflow)?;
        let created = submitted.checked_add(1).ok_or_else(sequence_overflow)?;
        Ok(PurchaseCreationEventSequence { submitted, created })
    }

    /// 按生产采购计划的稳定遍历顺序返回全部序号对。
    /// # 参数
    /// 无。
    /// # 返回
    /// 返回与采购计划等长的逐单序号迭代器。
    /// # 错误
    /// 每项沿用已校验计划的索引与容量校验。
    pub(crate) fn orders(self) -> impl ExactSizeIterator<Item = Result<PurchaseCreationEventSequence>> {
        (0..self.order_count).map(move |index| self.order(index))
    }

    /// 返回最后写入的批次主事件序号。
    /// # 参数
    /// 无。
    /// # 返回
    /// 无采购子单时为1，否则位于最后一条创建事件之后。
    /// # 错误
    /// 无。
    pub(crate) fn main(self) -> u32 {
        self.main
    }
}

fn sequence_overflow() -> Error {
    Error::ValidationError("采购批次审计事件序号超出可用范围".into())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn plan_rejects_sequence_overflow_without_allocating_orders() {
        let max_orders = usize::try_from(u32::MAX / 2).unwrap();
        let plan = SourcingEventSequencePlan::new(max_orders).unwrap();
        assert_eq!(plan.main(), u32::MAX);
        let last = plan.order(max_orders - 1).unwrap();
        assert_eq!(last.created(), u32::MAX - 1);
        assert!(matches!(SourcingEventSequencePlan::new(max_orders + 1),
            Err(Error::ValidationError(message)) if message == "采购批次审计事件序号超出可用范围"));
        assert!(plan.order(max_orders).is_err());
        assert_eq!(SourcingEventSequencePlan::new(0).unwrap().main(), 1);
    }
}
