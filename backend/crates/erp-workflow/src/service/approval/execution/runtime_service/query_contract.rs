//! 运行实例列表查询契约：`RuntimeInstanceListQuery` 的纯 prepare/validate。
//!
//! 从 `query.rs` 拆出，Service 只保留编排与授权调用；新增视图先改此处契约。

use serde::{Deserialize, Serialize};

use super::super::runtime_query::{RuntimeInstanceListView, RuntimeInstanceStatusFilter};
use crate::entity::document_registry::DocumentType;
use crate::error::{Error, Result};

/// 实例列表默认页大小。
pub(crate) const DEFAULT_RUNTIME_INSTANCE_LIST_LIMIT: u32 = 20;
/// 实例列表最大页大小。
pub(crate) const MAX_RUNTIME_INSTANCE_LIST_LIMIT: u32 = 100;
/// 实例列表检索串与游标 ID 最大字符数。
pub(crate) const RUNTIME_INSTANCE_LIST_TEXT_MAX_LEN: usize = 128;

/// 实例列表查询。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RuntimeInstanceListQuery {
    /// 固定视图。
    pub view: RuntimeInstanceListView,
    /// 可选单据类型稳定码。
    pub document_type: Option<String>,
    /// 可选状态。
    pub status: Option<RuntimeInstanceStatusFilter>,
    /// 当前视图的稳定游标。
    pub cursor: Option<RuntimeInstanceListCursor>,
    /// 页大小。
    pub limit: u32,
    /// 可选字面量检索；空表示不按关键词过滤。
    pub query: Option<String>,
}

impl RuntimeInstanceListQuery {
    /// 由协议输入形成规范化查询。
    ///
    /// # 参数
    /// * `view` - 固定查询视图
    /// * `document_type` - 可选单据类型稳定码
    /// * `status` - 可选实例状态
    /// * `cursor` - HTTP 层已解码的稳定游标
    /// * `limit` - 可选页大小；省略时使用 20
    /// * `query` - 可选字面量检索串
    ///
    /// # 返回
    /// 返回已规范化并通过完整边界校验的查询。
    ///
    /// # 错误
    /// view/status、document_type、limit、cursor 或检索串不符合合同时返回校验错误。
    pub fn prepare(
        view: RuntimeInstanceListView,
        document_type: Option<String>,
        status: Option<RuntimeInstanceStatusFilter>,
        cursor: Option<RuntimeInstanceListCursor>,
        limit: Option<u32>,
        query: Option<String>,
    ) -> Result<Self> {
        let prepared = Self {
            view,
            document_type,
            status,
            cursor: cursor.map(Self::prepare_cursor),
            limit: limit.unwrap_or(DEFAULT_RUNTIME_INSTANCE_LIST_LIMIT),
            query: query.map(|value| value.trim().to_string()).filter(|value| !value.is_empty()),
        };
        prepared.validate()?;
        Ok(prepared)
    }

    /// 校验规范化查询的全部纯输入合同。
    ///
    /// # 返回
    /// 所有合同成立时返回 `Ok(())`。
    ///
    /// # 错误
    /// view/status、document_type、limit、cursor 或检索串不符合合同时返回校验错误。
    pub fn validate(&self) -> Result<()> {
        self.validate_view_status()?;
        if let Some(document_type) = self.document_type.as_deref() {
            parse_document_type(document_type)?;
        }
        if !(1..=MAX_RUNTIME_INSTANCE_LIST_LIMIT).contains(&self.limit) {
            return Err(Error::ValidationError(format!(
                "limit 必须在 1 到 {MAX_RUNTIME_INSTANCE_LIST_LIMIT} 之间"
            )));
        }
        self.validate_cursor()?;
        self.validate_query()
    }

    /// trim 游标 ID；排序时间的完整 `i64` 定义域保持不变。
    fn prepare_cursor(mut cursor: RuntimeInstanceListCursor) -> RuntimeInstanceListCursor {
        cursor.id = cursor.id.trim().to_string();
        cursor
    }

    /// 校验固定视图允许的状态集合。
    fn validate_view_status(&self) -> Result<()> {
        match (self.view, self.status) {
            (RuntimeInstanceListView::Mine, None | Some(RuntimeInstanceStatusFilter::Running))
            | (RuntimeInstanceListView::Blocked, None | Some(RuntimeInstanceStatusFilter::Blocked))
            | (RuntimeInstanceListView::Started | RuntimeInstanceListView::Managed, _) => Ok(()),
            (RuntimeInstanceListView::Mine, _) => {
                Err(Error::ValidationError("mine 只接受省略 status 或 status=RUNNING".to_string()))
            },
            (RuntimeInstanceListView::Blocked, _) => {
                Err(Error::ValidationError("blocked 只接受省略 status 或 status=BLOCKED".to_string()))
            },
        }
    }

    /// 校验游标 ID 为已 trim 的有界稳定标识。
    fn validate_cursor(&self) -> Result<()> {
        let Some(cursor) = self.cursor.as_ref() else {
            return Ok(());
        };
        let id = cursor.id.as_str();
        if id.is_empty() || id != id.trim() {
            return Err(Error::ValidationError("cursor id 不能为空".to_string()));
        }
        if id.chars().count() > RUNTIME_INSTANCE_LIST_TEXT_MAX_LEN {
            return Err(Error::ValidationError(format!(
                "cursor id 不能超过 {RUNTIME_INSTANCE_LIST_TEXT_MAX_LEN} 个字符"
            )));
        }
        Ok(())
    }

    /// 校验字面量检索串已规范化且长度有界。
    fn validate_query(&self) -> Result<()> {
        let Some(query) = self.query.as_deref() else {
            return Ok(());
        };
        if query.is_empty() || query != query.trim() {
            return Err(Error::ValidationError("q 必须是非空规范化文本".to_string()));
        }
        if query.chars().count() > RUNTIME_INSTANCE_LIST_TEXT_MAX_LEN {
            return Err(Error::ValidationError(format!(
                "q 不能超过 {RUNTIME_INSTANCE_LIST_TEXT_MAX_LEN} 个字符"
            )));
        }
        Ok(())
    }
}

/// 实例列表稳定游标。
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct RuntimeInstanceListCursor {
    /// 当前视图排序时间。
    pub sort_time: i64,
    /// 并列时的实例主键。
    pub id: String,
}

impl RuntimeInstanceListCursor {
    /// 以必填实例构造稳定游标；排序时间默认为 0。
    ///
    /// # 参数
    /// * `id` - 并列时的实例主键
    ///
    /// # 返回
    /// 返回零排序时间的游标。
    ///
    /// # 错误
    /// 无。
    pub fn new(id: String) -> Self {
        Self { sort_time: 0, id }
    }
}

/// 解析实例列表筛选中的单据类型稳定码。
///
/// # 参数
/// * `code` - 调用方提供的单据类型稳定代码
///
/// # 返回
/// 精确命中登记代码时返回对应单据类型。
///
/// # 错误
/// 未登记代码返回原有校验错误文本。
///
/// # 关键业务约束
/// Service 不裁剪、不接受别名，也不维护第二份代码注册表。
pub(crate) fn parse_document_type(code: &str) -> Result<DocumentType> {
    DocumentType::try_from_code(code).map_err(|_| Error::ValidationError(format!("未登记单据类型: {code}")))
}

#[cfg(test)]
mod runtime_instance_list_query_tests {

    use super::{
        DEFAULT_RUNTIME_INSTANCE_LIST_LIMIT, MAX_RUNTIME_INSTANCE_LIST_LIMIT,
        RUNTIME_INSTANCE_LIST_TEXT_MAX_LEN, RuntimeInstanceListCursor, RuntimeInstanceListQuery,
        RuntimeInstanceListView, RuntimeInstanceStatusFilter,
    };
    use crate::entity::document_registry::DocumentType;

    fn prepare(
        view: RuntimeInstanceListView,
        status: Option<RuntimeInstanceStatusFilter>,
    ) -> crate::error::Result<RuntimeInstanceListQuery> {
        RuntimeInstanceListQuery::prepare(view, None, status, None, None, None)
    }

    /// 四种视图逐一覆盖省略状态及全部固定状态。
    #[test]
    fn view_status_matrix_is_complete() {
        let views = [
            RuntimeInstanceListView::Mine,
            RuntimeInstanceListView::Blocked,
            RuntimeInstanceListView::Started,
            RuntimeInstanceListView::Managed,
        ];
        let statuses = [
            None,
            Some(RuntimeInstanceStatusFilter::Running),
            Some(RuntimeInstanceStatusFilter::Approved),
            Some(RuntimeInstanceStatusFilter::Cancelled),
            Some(RuntimeInstanceStatusFilter::Blocked),
        ];
        for view in views {
            for status in statuses {
                let expected = match view {
                    RuntimeInstanceListView::Mine => {
                        matches!(status, None | Some(RuntimeInstanceStatusFilter::Running))
                    },
                    RuntimeInstanceListView::Blocked => {
                        matches!(status, None | Some(RuntimeInstanceStatusFilter::Blocked))
                    },
                    RuntimeInstanceListView::Started | RuntimeInstanceListView::Managed => true,
                };
                assert_eq!(prepare(view, status).is_ok(), expected, "{view:?} {status:?}");
            }
        }
    }

    /// limit 省略、两端点及越界值必须得到唯一结果。
    #[test]
    fn limit_defaults_and_rejects_outside_closed_range() {
        let default = prepare(RuntimeInstanceListView::Managed, None).expect("默认 limit");
        assert_eq!(default.limit, DEFAULT_RUNTIME_INSTANCE_LIST_LIMIT);
        for limit in [1, MAX_RUNTIME_INSTANCE_LIST_LIMIT] {
            let query = RuntimeInstanceListQuery::prepare(
                RuntimeInstanceListView::Managed,
                None,
                None,
                None,
                Some(limit),
                None,
            )
            .expect("闭区间端点");
            assert_eq!(query.limit, limit);
        }
        for limit in [0, MAX_RUNTIME_INSTANCE_LIST_LIMIT + 1] {
            assert!(
                RuntimeInstanceListQuery::prepare(
                    RuntimeInstanceListView::Managed,
                    None,
                    None,
                    None,
                    Some(limit),
                    None,
                )
                .is_err()
            );
        }
    }

    /// document_type 只接受注册表中的精确稳定码，prepare 与直接 validate 必须同样失败关闭。
    #[test]
    fn document_type_requires_an_exact_registered_code() {
        let registered = DocumentType::SalesOrder.as_str();
        let prepared = RuntimeInstanceListQuery::prepare(
            RuntimeInstanceListView::Managed,
            Some(registered.to_string()),
            None,
            None,
            None,
            None,
        )
        .expect("精确登记码");
        assert_eq!(prepared.document_type.as_deref(), Some(registered));

        for document_type in ["", "   ", "unknown", "SALES_ORDER", "sales_order "] {
            assert!(
                RuntimeInstanceListQuery::prepare(
                    RuntimeInstanceListView::Managed,
                    Some(document_type.to_string()),
                    None,
                    None,
                    None,
                    None,
                )
                .is_err(),
                "prepare 必须拒绝 {document_type:?}"
            );

            let mut direct = prepared.clone();
            direct.document_type = Some(document_type.to_string());
            assert!(direct.validate().is_err(), "validate 必须拒绝 {document_type:?}");
        }
    }

    /// cursor 保留完整 i64 时间域，ID 则 trim、非空且最多 128 字符。
    #[test]
    fn cursor_prepares_id_and_preserves_i64_time_domain() {
        for sort_time in [i64::MIN, i64::MAX] {
            let query = RuntimeInstanceListQuery::prepare(
                RuntimeInstanceListView::Managed,
                None,
                None,
                Some(RuntimeInstanceListCursor { sort_time, id: "  inst-1  ".to_string() }),
                None,
                None,
            )
            .expect("合法 i64 时间与可规范化 ID");
            let cursor = query.cursor.expect("游标");
            assert_eq!(cursor.sort_time, sort_time);
            assert_eq!(cursor.id, "inst-1");
        }
        let max_id = "a".repeat(RUNTIME_INSTANCE_LIST_TEXT_MAX_LEN);
        assert!(
            RuntimeInstanceListQuery::prepare(
                RuntimeInstanceListView::Managed,
                None,
                None,
                Some(RuntimeInstanceListCursor { sort_time: 0, id: max_id }),
                None,
                None,
            )
            .is_ok()
        );
        for id in ["   ".to_string(), "a".repeat(RUNTIME_INSTANCE_LIST_TEXT_MAX_LEN + 1)] {
            assert!(
                RuntimeInstanceListQuery::prepare(
                    RuntimeInstanceListView::Managed,
                    None,
                    None,
                    Some(RuntimeInstanceListCursor { sort_time: 0, id }),
                    None,
                    None,
                )
                .is_err()
            );
        }
    }

    /// q 空白归 None，文本 trim，字符上限不得按字节数误判。
    #[test]
    fn query_text_is_trimmed_and_character_bounded() {
        let blank = RuntimeInstanceListQuery::prepare(
            RuntimeInstanceListView::Started,
            None,
            None,
            None,
            None,
            Some("   ".to_string()),
        )
        .expect("空白 q");
        assert_eq!(blank.query, None);

        let trimmed = RuntimeInstanceListQuery::prepare(
            RuntimeInstanceListView::Started,
            None,
            None,
            None,
            None,
            Some("  SO-1  ".to_string()),
        )
        .expect("trim q");
        assert_eq!(trimmed.query.as_deref(), Some("SO-1"));

        let max = "界".repeat(RUNTIME_INSTANCE_LIST_TEXT_MAX_LEN);
        assert!(
            RuntimeInstanceListQuery::prepare(
                RuntimeInstanceListView::Started,
                None,
                None,
                None,
                None,
                Some(max),
            )
            .is_ok()
        );
        assert!(
            RuntimeInstanceListQuery::prepare(
                RuntimeInstanceListView::Started,
                None,
                None,
                None,
                None,
                Some("界".repeat(RUNTIME_INSTANCE_LIST_TEXT_MAX_LEN + 1)),
            )
            .is_err()
        );
    }
}
