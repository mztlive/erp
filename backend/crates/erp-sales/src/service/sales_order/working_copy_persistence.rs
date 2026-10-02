//! 可编辑工作副本行复用既有唯一键；提交与正式版本仍由独立快照承载。

use std::collections::{HashMap, HashSet};

use async_trait::async_trait;
use erp_core::ids::SalesOrderWorkingCopyId;
use mongodb::Database;
use persistence_core::Executor;

use crate::entity::sales_order::SalesOrderWorkingCopyLine;
use crate::repository::SalesOrderExt;
use crate::repository::owned::SalesOrderWorkingCopyLineRepository;
use crate::{Error, Result};

/// 当前工作副本行的一次持久化动作。
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum WriteAction {
    Create,
    Update,
    SoftDelete,
    Restore,
}

/// 原行不存在、保留、恢复或被移除时采用不同写入次序。
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum ChangeKind {
    New,
    Existing,
    Restored,
    Removed,
}

struct RowChange {
    kind: ChangeKind,
    row: SalesOrderWorkingCopyLine,
}

/// 批量复验并保存草稿行；不创建事务且不得改变现有完整唯一索引。
///
/// # 参数
/// * `db` - 销售仓储来源
/// * `copy_id` - 已验证可编辑的工作副本
/// * `expected` - 准备阶段读取的活跃行及其版本
/// * `lines` - 本次重建的明细内容
/// * `executor` - 调用方根事务执行器
///
/// # 返回
/// 复用已存在的行身份，恢复重新加入的软删除行，仅插入全新稳定明细。
///
/// # 错误
/// 活跃行版本或身份变化、重复稳定行、工作副本归属错误以及仓储错误时失败。
pub(super) async fn replace_working_copy_lines(
    db: &Database,
    copy_id: &SalesOrderWorkingCopyId,
    expected: &[SalesOrderWorkingCopyLine],
    lines: &[SalesOrderWorkingCopyLine],
    executor: &mut dyn Executor,
) -> Result<()> {
    let repository = db.sales_order_working_copy_lines();
    let mut existing = repository
        .find_many_by_field_including_deleted("working_copy_id", copy_id.to_string(), executor)
        .await?;
    existing.sort_by_key(|row| row.line_no);
    validate_expected_rows(expected, &existing)?;
    let changes = plan_changes(copy_id, &existing, lines)?;
    persist_changes(&mut RepositoryWriter { repository }, changes, executor).await
}

/// 保留原准备阶段的行版本要求，事务内新增、删除或修改均返回并发冲突。
fn validate_expected_rows(
    expected: &[SalesOrderWorkingCopyLine],
    current: &[SalesOrderWorkingCopyLine],
) -> Result<()> {
    let active = current.iter().filter(|row| !row.base.is_deleted()).collect::<Vec<_>>();
    if active.len() != expected.len()
        || expected.iter().any(|expected| {
            !active
                .iter()
                .any(|row| row.base.id == expected.base.id && row.base.version == expected.base.version)
        })
    {
        return Err(Error::ConflictError("数据已被其他请求修改，请刷新后重试".into()));
    }
    Ok(())
}

/// 从已读取的当前行规划替换，稳定唯一键匹配成功时复用完整原元数据。
fn plan_changes(
    copy_id: &SalesOrderWorkingCopyId,
    existing: &[SalesOrderWorkingCopyLine],
    lines: &[SalesOrderWorkingCopyLine],
) -> Result<Vec<RowChange>> {
    let current = existing.iter().map(|row| (&row.sales_order_line_id, row)).collect::<HashMap<_, _>>();
    let mut selected = HashSet::new();
    let mut replacements = Vec::with_capacity(lines.len());
    for incoming in lines {
        if &incoming.working_copy_id != copy_id || !selected.insert(&incoming.sales_order_line_id) {
            return Err(Error::ValidationError("工作副本明细归属不一致或稳定明细重复".into()));
        }
        let mut row = incoming.clone();
        let kind = if let Some(previous) = current.get(&incoming.sales_order_line_id) {
            row.base = previous.base.clone();
            if previous.base.is_deleted() { ChangeKind::Restored } else { ChangeKind::Existing }
        } else {
            ChangeKind::New
        };
        replacements.push(RowChange { kind, row });
    }
    let mut changes = existing
        .iter()
        .filter(|row| !row.base.is_deleted() && !selected.contains(&row.sales_order_line_id))
        .map(|row| RowChange { kind: ChangeKind::Removed, row: row.clone() })
        .collect::<Vec<_>>();
    changes.extend(replacements);
    Ok(changes)
}

/// 保留同一个执行器，恢复后的最新 CAS 元数据直接供内容更新使用。
#[async_trait]
trait DraftRowWriter: Send {
    /// 执行一次行写入并在成功时同步行元数据；不得开启或提交事务。
    async fn apply(
        &mut self,
        action: WriteAction,
        row: &mut SalesOrderWorkingCopyLine,
        executor: &mut dyn Executor,
    ) -> Result<()>;
}

struct RepositoryWriter<'a> {
    repository: SalesOrderWorkingCopyLineRepository<'a>,
}

#[async_trait]
impl DraftRowWriter for RepositoryWriter<'_> {
    async fn apply(
        &mut self,
        action: WriteAction,
        row: &mut SalesOrderWorkingCopyLine,
        executor: &mut dyn Executor,
    ) -> Result<()> {
        match action {
            WriteAction::Create => self.repository.create(row, executor).await?,
            WriteAction::Update => self.repository.update(row, executor).await?,
            WriteAction::SoftDelete => self.repository.soft_delete(row, executor).await?,
            WriteAction::Restore => self.repository.restore(row, executor).await?,
        }
        Ok(())
    }
}

/// 只有重新加入的软删除行执行恢复再更新；任一失败均停止后续动作。
async fn persist_changes(
    writer: &mut impl DraftRowWriter,
    changes: Vec<RowChange>,
    executor: &mut dyn Executor,
) -> Result<()> {
    for mut change in changes {
        let action = match change.kind {
            ChangeKind::New => WriteAction::Create,
            ChangeKind::Existing => WriteAction::Update,
            ChangeKind::Removed => WriteAction::SoftDelete,
            ChangeKind::Restored => {
                writer.apply(WriteAction::Restore, &mut change.row, executor).await?;
                WriteAction::Update
            },
        };
        writer.apply(action, &mut change.row, executor).await?;
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use std::slice::from_ref;

    use erp_core::ids::SalesOrderWorkingCopyLineId;
    use persistence_core::NoTransaction;

    use super::*;
    use crate::entity::sales_order::{SalesOrderWorkingCopyLineData, SalesPricingMode};

    /// 构造真实草稿行，显式指定稳定行身份及成交价模式。
    fn row(id: &str, line_no: u32, mode: SalesPricingMode, price: &str) -> SalesOrderWorkingCopyLine {
        let data: SalesOrderWorkingCopyLineData = serde_json::from_value(serde_json::json!({
            "sales_order_line_id": format!("stable-{line_no}"), "line_no": line_no,
            "line_type": "GOODS_SERVICE", "sales_tax_rate": "0.13", "item_name_snapshot": "茶礼",
            "spec_snapshot": null, "unit_snapshot": "件", "voucher": null,
            "goods": { "sku_id": "sku-1", "sku_revision_id": "rev-1", "welfare_scenario": null,
                "service_region": "上海", "fulfillment_due_at": 1800000000, "quantity": "3",
                "base_unit_code": "件", "unit_price_gross": price, "pricing_mode": mode },
        }))
        .unwrap();
        let mut row = SalesOrderWorkingCopyLine::new(
            SalesOrderWorkingCopyLineId::new(id),
            SalesOrderWorkingCopyId::new("copy-1"),
            data,
        )
        .unwrap();
        row.base.version = 7;
        row.base.created_at = 123;
        row
    }

    struct TestExecutor(u8);
    impl Executor for TestExecutor {
        /// 非零大小执行器仅提供身份，不接触真实数据库会话。
        fn session(&mut self) -> Option<&mut mongodb::ClientSession> {
            assert_eq!(self.0, 71);
            None
        }
    }

    struct RecordingWriter {
        calls: Vec<(WriteAction, String, u64, usize, bool)>,
        fail_at: Option<usize>,
    }

    #[async_trait]
    impl DraftRowWriter for RecordingWriter {
        /// 模拟 CAS 成功回填的元数据，记录真实编排动作与执行器。
        async fn apply(
            &mut self,
            action: WriteAction,
            row: &mut SalesOrderWorkingCopyLine,
            executor: &mut dyn Executor,
        ) -> Result<()> {
            self.calls.push((
                action,
                row.base.id.clone(),
                row.base.version,
                executor as *mut dyn Executor as *mut () as usize,
                row.base.is_deleted(),
            ));
            if self.fail_at == Some(self.calls.len()) {
                return Err(Error::ConflictError("模拟写入失败".into()));
            }
            row.base.version += 1;
            if action == WriteAction::Restore {
                row.base.deleted_at = 0;
            }
            Ok(())
        }
    }

    /// 保留行复用原ID和CAS版本，更新内容包括AUTO切回MANUAL的显式序列化字段。
    #[test]
    fn retained_row_reuses_identity_and_serializes_manual_over_auto() {
        let existing = row("persisted-row", 1, SalesPricingMode::Auto, "119");
        let incoming = row("unused-new-id", 1, SalesPricingMode::Manual, "125");
        let changes =
            plan_changes(&SalesOrderWorkingCopyId::new("copy-1"), from_ref(&existing), from_ref(&incoming))
                .unwrap();
        assert_eq!(changes.len(), 1);
        assert_eq!(changes[0].kind, ChangeKind::Existing);
        assert_eq!(changes[0].row.base, existing.base);
        assert_eq!(changes[0].row.unit_price_gross, incoming.unit_price_gross);
        assert_eq!(changes[0].row.gross_amount, incoming.gross_amount);
        assert_eq!(changes[0].row.pricing_mode, SalesPricingMode::Manual);
        let document = serde_json::to_value(&changes[0].row).unwrap();
        assert_eq!(document["pricing_mode"], "MANUAL");
    }

    /// 重新加入的软删除行先恢复，随后使用恢复返回的新CAS版本更新新内容。
    #[tokio::test]
    async fn readded_row_restores_old_identity_then_updates_new_version() {
        let mut existing = row("persisted-row", 1, SalesPricingMode::Manual, "125");
        existing.base.deleted_at = 456;
        let incoming = row("unused-new-id", 1, SalesPricingMode::Auto, "119");
        validate_expected_rows(&[], &[existing.clone()]).unwrap();
        let changes =
            plan_changes(&SalesOrderWorkingCopyId::new("copy-1"), &[existing], &[incoming]).unwrap();
        assert_eq!(changes[0].kind, ChangeKind::Restored);
        let mut writer = RecordingWriter { calls: Vec::new(), fail_at: None };
        let mut executor = TestExecutor(71);
        let pointer = &mut executor as *mut TestExecutor as usize;
        persist_changes(&mut writer, changes, &mut executor).await.unwrap();
        assert_eq!(
            writer.calls.iter().map(|call| (call.0, call.1.as_str(), call.2, call.4)).collect::<Vec<_>>(),
            [
                (WriteAction::Restore, "persisted-row", 7, true),
                (WriteAction::Update, "persisted-row", 8, false)
            ]
        );
        assert!(writer.calls.iter().all(|call| call.3 == pointer));
    }

    /// 只软删被移除行，保留行执行更新，新稳定行才执行插入。
    #[tokio::test]
    async fn replacement_soft_deletes_removed_rows_and_only_inserts_new_keys() {
        let existing = vec![
            row("old-1", 1, SalesPricingMode::Auto, "119"),
            row("old-2", 2, SalesPricingMode::Manual, "125"),
        ];
        let incoming = vec![
            row("new-1", 1, SalesPricingMode::Manual, "125"),
            row("new-3", 3, SalesPricingMode::Auto, "119"),
        ];
        let changes = plan_changes(&SalesOrderWorkingCopyId::new("copy-1"), &existing, &incoming).unwrap();
        let mut writer = RecordingWriter { calls: Vec::new(), fail_at: None };
        persist_changes(&mut writer, changes, &mut NoTransaction).await.unwrap();
        assert_eq!(
            writer.calls.iter().map(|call| (call.0, call.1.as_str())).collect::<Vec<_>>(),
            [
                (WriteAction::SoftDelete, "old-2"),
                (WriteAction::Update, "old-1"),
                (WriteAction::Create, "new-3")
            ]
        );
    }

    /// 活跃集合或准备阶段CAS版本变化时，在写入前拒绝命令。
    #[test]
    fn expected_rows_reject_changed_versions_or_active_membership() {
        let old = row("old-1", 1, SalesPricingMode::Manual, "125");
        let mut changed = old.clone();
        changed.base.version += 1;
        assert!(validate_expected_rows(from_ref(&old), &[changed]).is_err());
        assert!(validate_expected_rows(from_ref(&old), &[]).is_err());
        assert!(validate_expected_rows(&[], from_ref(&old)).is_err());
        validate_expected_rows(from_ref(&old), from_ref(&old)).unwrap();
    }

    /// 重复稳定行与错误工作副本归属不能产生写入计划。
    #[test]
    fn replacement_rejects_duplicate_keys_and_wrong_copy() {
        let first = row("new-1", 1, SalesPricingMode::Auto, "119");
        assert!(
            plan_changes(&SalesOrderWorkingCopyId::new("copy-1"), &[], &[first.clone(), first.clone()])
                .is_err()
        );
        assert!(plan_changes(&SalesOrderWorkingCopyId::new("wrong-copy"), &[], &[first]).is_err());
        let empty = plan_changes(&SalesOrderWorkingCopyId::new("copy-1"), &[], &[]).unwrap();
        assert!(empty.is_empty());
    }

    /// 恢复或后续更新失败时停止其他行写入，并保持原错误类别。
    #[tokio::test]
    async fn restore_or_update_failure_stops_later_row_writes() {
        for fail_at in [1, 2] {
            let mut removed = row("old-1", 1, SalesPricingMode::Manual, "125");
            removed.base.deleted_at = 456;
            let next = vec![
                row("new-1", 1, SalesPricingMode::Auto, "119"),
                row("new-2", 2, SalesPricingMode::Auto, "119"),
            ];
            let changes = plan_changes(&SalesOrderWorkingCopyId::new("copy-1"), &[removed], &next).unwrap();
            let mut writer = RecordingWriter { calls: Vec::new(), fail_at: Some(fail_at) };
            assert!(matches!(
                persist_changes(&mut writer, changes, &mut NoTransaction).await,
                Err(Error::ConflictError(_))
            ));
            assert_eq!(writer.calls.len(), fail_at);
            assert!(writer.calls.iter().all(|call| call.1 == "old-1"));
        }
    }
}
