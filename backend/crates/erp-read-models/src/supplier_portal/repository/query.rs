//! 有界分页、外部筛选及同执行器聚合。

use application_core::{page_or_default, page_size_or_default};
use futures_util::TryStreamExt;
use mongodb::Collection;
use mongodb::bson::{Document, deserialize_from_document};
use persistence_core::{Error as PersistenceError, Executor};
use serde::de::DeserializeOwned;

use super::super::PortalListParams;
use crate::{Error, Result};

/// 外部输入归一化后的固定分页边界。
pub(in crate::supplier_portal) struct PortalQuery {
    pub page: u64,
    pub page_size: u32,
    pub skip: i64,
    pub q: Option<String>,
    pub status: Option<String>,
}

impl PortalQuery {
    /// 拒绝非法尺寸及溢出，不让搜索打开其他供应商范围。
    pub fn new(params: &PortalListParams) -> Result<Self> {
        if params.page_size.is_some_and(|size| size == 0 || size > 100) {
            return Err(Error::ValidationError("每页条数须为 1 至 100".into()));
        }
        let page = page_or_default(params.page);
        let page_size = page_size_or_default(params.page_size);
        let skip = (page - 1)
            .checked_mul(u64::from(page_size))
            .and_then(|skip| i64::try_from(skip).ok())
            .ok_or_else(|| Error::ValidationError("分页范围过大".into()))?;
        let q = params.q.as_deref().map(str::trim).filter(|value| !value.is_empty()).map(str::to_string);
        if q.as_ref().is_some_and(|value| value.chars().count() > 128) {
            return Err(Error::ValidationError("搜索文本过长".into()));
        }
        let status = params
            .status
            .as_deref()
            .map(str::trim)
            .filter(|value| !value.is_empty())
            .map(str::to_ascii_uppercase);
        if status.as_ref().is_some_and(|status| {
            !matches!(
                status.as_str(),
                "DRAFT" | "PENDING" | "SUBMITTED" | "RETURNED" | "WITHDRAWN" | "EFFECTIVE"
            )
        }) {
            return Err(Error::ValidationError("申请状态无效".into()));
        }
        Ok(Self {
            page,
            page_size,
            skip,
            q,
            status: status.map(|status| if status == "PENDING" { "SUBMITTED".into() } else { status }),
        })
    }
}

/// 聚合始终复用读取执行器，避免授权与结果跨越事务快照。
pub(in crate::supplier_portal) async fn aggregate<T: DeserializeOwned>(
    collection: Collection<Document>,
    pipeline: Vec<Document>,
    executor: &mut dyn Executor,
) -> Result<Vec<T>> {
    let documents = match executor.session() {
        Some(session) => {
            let mut cursor = collection
                .aggregate(pipeline)
                .session(&mut *session)
                .await
                .map_err(PersistenceError::from)?;
            let mut documents = Vec::new();
            while let Some(document) =
                cursor.next(&mut *session).await.transpose().map_err(PersistenceError::from)?
            {
                documents.push(document);
            }
            documents
        },
        None => collection
            .aggregate(pipeline)
            .await
            .map_err(PersistenceError::from)?
            .try_collect::<Vec<Document>>()
            .await
            .map_err(PersistenceError::from)?,
    };
    documents
        .into_iter()
        .map(|document| {
            deserialize_from_document(document).map_err(|error| Error::Internal(error.to_string()))
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::{PortalListParams, PortalQuery};
    use crate::Error;

    #[test]
    fn bounded_query_normalizes_input_and_rejects_overflow_and_injection() {
        let query = PortalQuery::new(&PortalListParams {
            q: Some("  茶杯  ".into()),
            status: Some("pending".into()),
            page: Some(2),
            page_size: Some(10),
        })
        .unwrap();
        assert_eq!((query.page, query.page_size, query.skip), (2, 10, 10));
        assert_eq!(query.q.as_deref(), Some("茶杯"));
        assert_eq!(query.status.as_deref(), Some("SUBMITTED"));
        for size in [0, 101] {
            assert!(matches!(
                PortalQuery::new(&PortalListParams { page_size: Some(size), ..Default::default() }),
                Err(Error::ValidationError(_))
            ));
        }
        assert!(
            PortalQuery::new(&PortalListParams {
                page: Some(u64::MAX),
                page_size: Some(100),
                ..Default::default()
            })
            .is_err()
        );
        assert!(
            serde_json::from_value::<PortalListParams>(serde_json::json!({"supplier_id":"other"})).is_err()
        );
    }
}
