use entity_core::NOT_DELETED_TIMESTAMP_BSON;
use erp_core::common::time::{BusinessDate, Instant};
use erp_core::ids::{PayableAccountId, SupplierAccountId};
use futures_util::TryStreamExt;
use mongodb::bson::{Document, doc};
use mongodb::options::FindOptions;
use persistence_core::{
    Executor, PageResult, Pagination, QueryFilter, Result, insert_literal_regex_filter, mongo_ops,
};
use serde::{Deserialize, Serialize};

use super::projection::{statement_sort_doc, supplier_settlement_statement_projection};
use crate::entity::supplier_settlement::{
    SettlementReviewResult, SettlementStatus, SupplierSettlementSourceEvidence, SupplierSettlementStatement,
};
use crate::repository::owned::{
    SupplierSettlementSourceEvidenceRepository, SupplierSettlementStatementRepository,
};

/// 供应商结算单列表投影行。
///
/// 列表接口只取必要字段，禁止返回整文档；金额以实体 `Amount`（Decimal128）原样
/// 透传，不做任何舍入或换算（P2 §2.4）。
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct SupplierSettlementStatementRow {
    /// 实体主键。
    pub id: String,
    /// ERP 结算单号。
    pub statement_no: String,
    /// 结算供应商。
    pub supplier_id: SupplierAccountId,
    /// 结算期间开始（含）。
    pub period_start: BusinessDate,
    /// 结算期间结束（含）。
    pub period_end: BusinessDate,
    /// 供应商结算期间策略。
    pub period_policy_id: String,
    /// 供应商结算期间策略版本。
    pub period_policy_version: String,
    /// 供应商结算期间策略时区。
    pub period_timezone: String,
    /// 供应商账单号。
    pub external_bill_no: Option<String>,
    /// 供应商账单版本。
    pub external_bill_version: Option<String>,
    /// ERP 金额。
    pub erp_amount: erp_core::money::Amount,
    /// 供应商金额。
    pub supplier_amount: erp_core::money::Amount,
    /// 双方金额差异（= 供应商金额 − ERP 金额）。
    pub difference_amount: erp_core::money::Amount,
    /// 结算状态。
    pub status: SettlementStatus,
    /// 正式复核主题摘要。
    pub subject_hash: String,
    /// 正式来源事实水位。
    pub source_as_of: Instant,
    /// 来源快照冻结时间。
    pub source_snapshot_at: Instant,
    /// 不可变来源快照摘要。
    pub source_snapshot_hash: String,
    /// 提交复核采用的刷新截止策略。
    pub refresh_cutoff_policy_id: String,
    /// 刷新截止策略冻结版本。
    pub refresh_cutoff_policy_version: String,
    /// 对账负责人。
    pub prepared_by: String,
    /// 业务组织。
    #[serde(default)]
    pub business_org_unit_id: String,
    /// 差异处理人。
    #[serde(default)]
    pub difference_handler_user_id: String,
    /// 实际复核人。
    pub reviewed_by: Option<String>,
    /// 最近一次正式复核决定。
    pub review_result: Option<SettlementReviewResult>,
    /// 最近一次驳回原因代码。
    pub review_reason_code: Option<String>,
    /// 最近一次复核说明。
    pub review_comment: Option<String>,
    /// 最近一次正式复核决定时间。
    pub reviewed_at: Option<Instant>,
    /// 确认时间。
    pub confirmed_at: Option<Instant>,
    /// 确认后形成的应付账户。
    pub payable_account_id: Option<PayableAccountId>,
    /// 乐观锁版本（`BaseModel.version` ≡ 数据模型 `lock_version`）。
    pub version: u64,
    /// 创建时间（秒级时间戳）。
    pub created_at: u64,
}

/// 结算单号最小事实行（FIN-R03 来源单号批量映射，只投影单号）。
#[derive(Debug, Clone, Deserialize)]
struct SupplierSettlementNoRow {
    /// 实体主键。
    id: String,
    /// 结算业务单号。
    statement_no: String,
}

/// 供应商结算单列表筛选条件。
#[derive(Debug, Clone)]
pub struct SupplierSettlementStatementFilter {
    /// 多字段关键词。
    pub q: Option<String>,
    /// 当前供应商名称命中身份。
    pub keyword_supplier_ids: Vec<erp_core::ids::SupplierAccountId>,
    /// 结算单号（按字面量部分匹配，忽略大小写）；`None` 表示不筛选。
    pub statement_no: Option<String>,
    /// 结算供应商；`None` 表示不筛选。
    pub supplier_id: Option<SupplierAccountId>,
    /// 结算状态；`None` 表示不筛选。
    pub status: Option<SettlementStatus>,
    /// 结算期间开始下界（含）。
    pub period_from: Option<BusinessDate>,
    /// 结算期间结束上界（含）。
    pub period_to: Option<BusinessDate>,
    /// 已证明的授权条件；`None` 表示调用方尚未接入范围。
    pub authorized_scope: Option<super::SettlementReadScope>,
    /// 对账负责人筛选，只收窄授权结果。
    pub owner_user_ids: Option<Vec<String>>,
    /// 差异处理人筛选，只收窄授权结果。
    pub operator_user_ids: Option<Vec<String>>,
    /// 实际复核人筛选，只收窄授权结果。
    pub handler_user_ids: Option<Vec<String>>,
    /// 当前开放复核任务命中的结算单，与 `handler_user_ids` 组成 OR。
    pub handler_open_statement_ids: Vec<String>,
    /// 业务组织筛选，只收窄授权结果。
    pub business_org_unit_ids: Option<Vec<String>>,
    /// 页码（1 起）。
    pub page: u64,
    /// 单页条数。
    pub page_size: u32,
    /// 排序字段（白名单内生效，白名单外回退 `created_at`）。
    pub sort_by: Option<String>,
    /// 是否升序；`false` 表示降序（默认）。
    pub sort_ascending: bool,
}

impl Default for SupplierSettlementStatementFilter {
    fn default() -> Self {
        Self {
            q: None,
            keyword_supplier_ids: Vec::new(),
            statement_no: None,
            supplier_id: None,
            status: None,
            period_from: None,
            period_to: None,
            authorized_scope: None,
            owner_user_ids: None,
            operator_user_ids: None,
            handler_user_ids: None,
            handler_open_statement_ids: Vec::new(),
            business_org_unit_ids: None,
            page: 1,
            page_size: 20,
            sort_by: None,
            sort_ascending: false,
        }
    }
}

/// 与结算单列表同一筛选水位计算的服务端汇总。
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct SupplierSettlementStatementStatsRow {
    pub pending_reconciliation_count: i64,
    pub has_difference_count: i64,
    pub pending_review_count: i64,
    pub confirmed_amount: erp_core::money::Amount,
}

impl QueryFilter for SupplierSettlementStatementFilter {
    /// 转换为 MongoDB 查询条件（自动追加未删除过滤）。
    ///
    /// # 返回
    /// 返回查询条件文档。
    fn to_doc(&self) -> Document {
        let mut filter = doc! { "deleted_at": NOT_DELETED_TIMESTAMP_BSON };
        if let Some(supplier_id) = &self.supplier_id {
            filter.insert("supplier_id", supplier_id.to_string());
        }
        if let Some(status) = self.status {
            filter.insert("status", status.as_str());
        }
        if let Some(period_from) = self.period_from {
            filter.insert("period_start", doc! { "$gte": period_from.to_string() });
        }
        if let Some(period_to) = self.period_to {
            filter.insert("period_end", doc! { "$lte": period_to.to_string() });
        }
        insert_literal_regex_filter(&mut filter, "statement_no", self.statement_no.as_deref());
        let ands = self.narrowing_conditions();
        if !ands.is_empty() {
            filter.insert("$and", ands);
        }
        filter
    }
}

impl SupplierSettlementStatementFilter {
    fn narrowing_conditions(&self) -> Vec<Document> {
        let mut ands = Vec::new();
        if let Some(q) = &self.q {
            ands.push(keyword_clause(q, &self.keyword_supplier_ids));
        }
        if let Some(scope) = &self.authorized_scope {
            ands.push(scope.document());
        }
        if let Some(ids) = &self.owner_user_ids {
            ands.push(doc! { "prepared_by": { "$in": ids } });
        }
        if let Some(ids) = &self.operator_user_ids {
            ands.push(operator_clause(ids));
        }
        if self.handler_user_ids.is_some() {
            ands.push(handler_clause(&self.handler_open_statement_ids));
        }
        if let Some(ids) = &self.business_org_unit_ids {
            ands.push(doc! { "business_org_unit_id": { "$in": ids } });
        }
        ands
    }
}

fn keyword_clause(q: &str, supplier_ids: &[erp_core::ids::SupplierAccountId]) -> Document {
    let mut clauses = ["statement_no", "external_bill_no"]
        .into_iter()
        .map(|field| {
            let mut clause = Document::new();
            insert_literal_regex_filter(&mut clause, field, Some(q));
            clause
        })
        .collect::<Vec<_>>();
    clauses.push(doc! {
        "supplier_id": { "$in": supplier_ids.iter().map(ToString::to_string).collect::<Vec<_>>() }
    });
    doc! { "$or": clauses }
}

fn operator_clause(ids: &[String]) -> Document {
    doc! {
        "$or": [
            { "difference_handler_user_id": { "$in": ids } },
            {
                "$and": [
                    { "$or": [
                        { "difference_handler_user_id": "" },
                        { "difference_handler_user_id": { "$exists": false } },
                    ]},
                    { "prepared_by": { "$in": ids } },
                ]
            },
        ]
    }
}

fn handler_clause(open_ids: &[String]) -> Document {
    if open_ids.is_empty() {
        doc! { "$expr": false }
    } else {
        doc! { "id": { "$in": open_ids } }
    }
}

impl Pagination for SupplierSettlementStatementFilter {
    /// 返回页码与单页条数。
    ///
    /// # 返回
    /// 返回 `(page, page_size)` 元组。
    fn page_and_size(&self) -> (u64, u64) {
        (self.page, u64::from(self.page_size))
    }
}

impl<'a> SupplierSettlementStatementRepository<'a> {
    /// 按明确授权条件读取单个结算单。
    ///
    /// # 参数
    /// * `id` - 结算单稳定主键
    /// * `scope` - 已证明的授权条件
    /// * `executor` - 调用方执行器
    ///
    /// # 返回
    /// 不存在或不在范围内均返回 `None`。
    ///
    /// # 关键业务约束
    /// ID 条件不能替换范围交集；空授权不得返回文档。
    pub async fn find_authorized(
        &self,
        id: &str,
        scope: &super::SettlementReadScope,
        executor: &mut dyn Executor,
    ) -> Result<Option<SupplierSettlementStatement>> {
        self.find_one(doc! { "$and": [{ "id": id }, scope.document()] }, executor).await
    }

    /// 按结算单 ID 集合批量读取结算单。
    ///
    /// # 参数
    /// * `statement_ids` - 结算单 ID 字符串集合；空集合直接返回空结果
    /// * `executor` - 数据访问执行器，由 Service 决定是否位于事务中
    ///
    /// # 返回
    /// 返回全部匹配且未删除的结算单；返回顺序不承诺与输入一致。
    ///
    /// # 错误
    /// 当 MongoDB 查询或游标读取失败时返回错误。
    pub async fn find_statements_by_ids(
        &self,
        statement_ids: &[String],
        executor: &mut dyn Executor,
    ) -> Result<Vec<SupplierSettlementStatement>> {
        if statement_ids.is_empty() {
            return Ok(Vec::new());
        }
        self.find_many(doc! { "id": { "$in": statement_ids } }, executor).await
    }

    /// 按结算单 ID 集合一次批量返回来源 ID 到结算单号的事实映射（FIN-R03）。
    ///
    /// 只投影 `id` 与 `statement_no`；空输入不访问数据库；仓储内去重后单次
    /// `$in` 查询。空单号按缺失处理，不进入映射，Service 保持 `None`
    /// 且不得回退内部 ID。
    ///
    /// # 参数
    /// * `statement_ids` - 结算单 ID 字符串集合
    /// * `executor` - 数据访问执行器，由 Service 决定是否位于事务中
    ///
    /// # 返回
    /// 返回来源 ID 到非空结算单号的映射；缺失来源不上表。
    ///
    /// # 错误
    /// 当 MongoDB 查询或游标读取失败时返回错误。
    pub async fn statement_nos_by_ids(
        &self,
        statement_ids: &[String],
        executor: &mut dyn Executor,
    ) -> Result<std::collections::HashMap<String, String>> {
        use std::collections::{HashMap, HashSet};
        let mut seen = HashSet::new();
        let mut deduped = Vec::new();
        for id in statement_ids {
            if !id.trim().is_empty() && seen.insert(id.clone()) {
                deduped.push(id.clone());
            }
        }
        if deduped.is_empty() {
            return Ok(HashMap::new());
        }
        let rows = mongo_ops::find_many(
            &self.collection().clone_with_type::<SupplierSettlementNoRow>(),
            doc! { "id": { "$in": deduped } },
            FindOptions::builder().projection(doc! { "id": 1, "statement_no": 1 }).build(),
            executor,
        )
        .await?;
        let mut map = HashMap::new();
        for row in rows {
            if !row.statement_no.trim().is_empty() {
                map.insert(row.id, row.statement_no);
            }
        }
        Ok(map)
    }

    /// 分页检索供应商结算单列表（投影查询）。
    ///
    /// 只返回 [`SupplierSettlementStatementRow`] 所需的列表字段，不加载整文档；
    /// 排序字段走白名单映射（`STATEMENT_SORT_FIELDS`），白名单外一律回退 `created_at`。
    ///
    /// # 参数
    /// * `filter` - 筛选与分页条件
    /// * `executor` - 数据访问执行器，由 Service 决定是否位于事务中
    ///
    /// # 返回
    /// 返回当前页投影行与满足筛选条件的总数。
    ///
    /// # 错误
    /// 当 MongoDB 查询、游标读取或计数失败时返回错误。
    pub async fn search_supplier_settlement_statements(
        &self,
        filter: &SupplierSettlementStatementFilter,
        executor: &mut dyn Executor,
    ) -> Result<PageResult<SupplierSettlementStatementRow>> {
        let options = FindOptions::builder()
            .sort(statement_sort_doc(filter.sort_by.as_deref(), filter.sort_ascending))
            .skip(filter.skip())
            .limit(filter.limit())
            .projection(supplier_settlement_statement_projection())
            .build();
        let collection = self.collection().clone_with_type::<SupplierSettlementStatementRow>();
        let items = mongo_ops::find_many(&collection, filter.to_doc(), options, executor).await?;
        let total = mongo_ops::count_documents(&self.collection(), filter.to_doc(), executor).await?;

        Ok(PageResult { items, total: total as i64 })
    }

    /// 按 ERP 结算单号查找唯一结算单。
    ///
    /// 唯一性由 `uk_supplier_settlement_statements_statement_no` 唯一索引保证；
    /// 该方法用于结算单号幂等判定与外部账单回填定位，服务层不得做
    /// 「先查后插」的重复性判断（§6.20）。
    ///
    /// # 参数
    /// * `statement_no` - ERP 结算单号
    /// * `executor` - 数据访问执行器，由 Service 决定是否位于事务中
    ///
    /// # 返回
    /// 返回匹配的未删除结算单；无匹配时返回 `None`。
    ///
    /// # 错误
    /// 当 MongoDB 查询失败时返回错误。
    pub async fn find_by_statement_no(
        &self,
        statement_no: &str,
        executor: &mut dyn Executor,
    ) -> Result<Option<SupplierSettlementStatement>> {
        self.find_one(doc! { "statement_no": statement_no }, executor).await
    }

    /// 按稳定 ID 读取供应商结算岗位分离事实。
    ///
    /// 工作项入口的历史名称；纯主键读取，直接委托基类单条查询。
    ///
    /// # 参数
    /// * `id` - 供应商结算单 ID
    /// * `executor` - 数据访问执行器，由 Service 决定是否位于事务中
    ///
    /// # 返回
    /// 返回未删除结算单；不存在时返回 `None`。
    ///
    /// # 错误
    /// 当 MongoDB 查询或反序列化失败时返回错误。
    ///
    /// # 约束
    /// 仅查询本仓储拥有的结算单集合，不访问结算明细集合。
    pub async fn find_work_item_supplier_settlement(
        &self,
        id: &str,
        executor: &mut dyn Executor,
    ) -> Result<Option<SupplierSettlementStatement>> {
        self.find_by_id(id, executor).await
    }

    /// 按列表完全相同的过滤条件计算跨页状态和确认金额汇总。
    pub async fn aggregate_supplier_settlement_statement_stats(
        &self,
        filter: &SupplierSettlementStatementFilter,
        executor: &mut dyn Executor,
    ) -> Result<Option<SupplierSettlementStatementStatsRow>> {
        let pipeline = vec![
            doc! { "$match": filter.to_doc() },
            doc! {
                "$group": {
                    "_id": mongodb::bson::Bson::Null,
                    "pending_reconciliation_count": {
                        "$sum": { "$cond": [{ "$eq": ["$status", "PENDING_RECONCILIATION"] }, 1, 0] }
                    },
                    "has_difference_count": {
                        "$sum": { "$cond": [{ "$eq": ["$status", "HAS_DIFFERENCE"] }, 1, 0] }
                    },
                    "pending_review_count": {
                        "$sum": { "$cond": [{ "$eq": ["$status", "PENDING_REVIEW"] }, 1, 0] }
                    },
                    "confirmed_amount": {
                        "$sum": {
                            "$toDecimal": {
                                "$cond": [{ "$eq": ["$status", "CONFIRMED"] }, "$erp_amount", "0.00"]
                            }
                        }
                    }
                }
            },
            doc! { "$project": { "_id": 0 } },
        ];
        let collection = self.collection();
        let rows = match executor.session() {
            Some(session) => {
                collection
                    .aggregate(pipeline)
                    .with_type::<SupplierSettlementStatementStatsRow>()
                    .session(&mut *session)
                    .await?
                    .stream(session)
                    .try_collect::<Vec<_>>()
                    .await?
            },
            None => {
                collection
                    .aggregate(pipeline)
                    .with_type::<SupplierSettlementStatementStatsRow>()
                    .await?
                    .try_collect::<Vec<_>>()
                    .await?
            },
        };
        Ok(rows.into_iter().next())
    }
}

impl<'a> SupplierSettlementSourceEvidenceRepository<'a> {
    /// 按稳定请求 ID 查找不可变来源证据批次。
    pub async fn find_by_request_id(
        &self,
        request_id: &str,
        executor: &mut dyn Executor,
    ) -> Result<Option<SupplierSettlementSourceEvidence>> {
        self.find_one(doc! { "request_id": request_id }, executor).await
    }

    /// 读取供应商、周期与策略版本下最新的完整来源证据批次。
    pub async fn latest_for_period(
        &self,
        supplier_id: &SupplierAccountId,
        period_start: BusinessDate,
        period_end: BusinessDate,
        period_policy_id: &str,
        period_policy_version: &str,
        executor: &mut dyn Executor,
    ) -> Result<Option<SupplierSettlementSourceEvidence>> {
        let options = FindOptions::builder()
            .sort(doc! { "source_version": -1, "created_at": -1, "id": -1 })
            .limit(1)
            .build();
        let mut values = mongo_ops::find_many(
            &self.collection(),
            doc! {
                "supplier_id": supplier_id.to_string(),
                "period_start": period_start.to_string(),
                "period_end": period_end.to_string(),
                "period_policy_id": period_policy_id,
                "period_policy_version": period_policy_version,
                "deleted_at": NOT_DELETED_TIMESTAMP_BSON,
            },
            options,
            executor,
        )
        .await?;
        Ok(values.pop())
    }

    /// 读取供应商与周期下最近登记的完整来源证据，用于创建前服务端预检。
    pub async fn latest_for_scope(
        &self,
        supplier_id: &SupplierAccountId,
        period_start: BusinessDate,
        period_end: BusinessDate,
        executor: &mut dyn Executor,
    ) -> Result<Option<SupplierSettlementSourceEvidence>> {
        let options = FindOptions::builder()
            .sort(doc! { "created_at": -1, "source_version": -1, "id": -1 })
            .limit(1)
            .build();
        let mut values = mongo_ops::find_many(
            &self.collection(),
            doc! {
                "supplier_id": supplier_id.to_string(),
                "period_start": period_start.to_string(),
                "period_end": period_end.to_string(),
                "deleted_at": NOT_DELETED_TIMESTAMP_BSON,
            },
            options,
            executor,
        )
        .await?;
        Ok(values.pop())
    }

    /// 按冻结来源摘要批量读取不可变来源证据。
    ///
    /// # 参数
    /// * `source_hashes` - 结算单持有的来源快照摘要
    /// * `executor` - 数据访问执行器
    ///
    /// # 返回
    /// 返回匹配且未软删除的来源证据批次。
    ///
    /// # 错误
    /// MongoDB 查询或反序列化失败时返回错误。
    pub async fn list_by_source_hashes(
        &self,
        source_hashes: &[String],
        executor: &mut dyn Executor,
    ) -> Result<Vec<SupplierSettlementSourceEvidence>> {
        if source_hashes.is_empty() {
            return Ok(Vec::new());
        }
        self.find_many_sorted(
            doc! { "source_hash": { "$in": source_hashes } },
            doc! { "source_version": -1, "id": 1 },
            executor,
        )
        .await
    }
}

impl SupplierSettlementStatementRepository<'_> {
    /// 按业务编号返回全部匹配身份，供跨域列表在分页前筛选。
    ///
    /// 只投影 ID、排除软删除；数据库错误向上返回。
    pub async fn matching_ids_by_number(
        &self,
        keyword: &str,
        executor: &mut dyn Executor,
    ) -> Result<Vec<String>> {
        let clauses = ["statement_no", "external_bill_no"]
            .into_iter()
            .map(|field| {
                let mut clause = Document::new();
                insert_literal_regex_filter(&mut clause, field, Some(keyword));
                clause
            })
            .collect::<Vec<_>>();
        let collection = self.collection();
        let mut query =
            collection.distinct("id", doc! { "deleted_at": NOT_DELETED_TIMESTAMP_BSON, "$or": clauses });
        if let Some(session) = executor.session() {
            query = query.session(session);
        }
        Ok(query.await?.into_iter().filter_map(|id| id.as_str().map(str::to_owned)).collect())
    }
}

#[cfg(test)]
mod keyword_regression_tests {
    use super::*;
    #[test]
    fn keyword_preserves_structural_scope() {
        let mut filter = SupplierSettlementStatementFilter {
            q: None,
            keyword_supplier_ids: Vec::new(),
            statement_no: None,
            supplier_id: None,
            status: None,
            period_from: None,
            period_to: None,
            ..Default::default()
        };

        filter.q = Some("BILL.[1]".into());
        filter.supplier_id = Some(erp_core::ids::SupplierAccountId::new("selected"));
        let query = filter.to_doc();
        assert_eq!(query.get_str("supplier_id").unwrap(), "selected");
        let text = format!("{query:?}");
        assert!(text.contains("external_bill_no"));
        assert!(text.contains("statement_no"));
    }
}
