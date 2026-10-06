//! 条码读取与唯一占用写入加入正式 SKU 的同一事务。

use persistence_core::{Executor, mongo_ops};

use super::CatalogRepository;
use crate::entity::catalog::SkuRevision;
use crate::entity::catalog::sku_barcode_claim::SkuBarcodeClaim;
use crate::repository::CatalogExt;
use crate::{Error, Result};

impl CatalogRepository<'_> {
    /// 复验已有归属并写入共同唯一条码占用点。
    /// # 参数
    /// `revision` 为本次正式修订；`executor` 为正式建档事务。
    /// # 返回
    /// 空条码不产生占用；同一 SKU 的历次修订复用并更新占用版本。
    /// # 错误
    /// 不同 SKU 归属、历史歧义、唯一键争用、乐观锁或数据库错误时拒绝。
    pub async fn claim_sku_barcode(&self, revision: &SkuRevision, executor: &mut dyn Executor) -> Result<()> {
        let Some(barcode) = revision.barcode.as_deref().map(str::trim).filter(|value| !value.is_empty())
        else {
            return Ok(());
        };
        if executor.session().is_none() {
            return Err(Error::Internal("条码占用必须与正式 SKU 建档共用事务".into()));
        }
        let owners = self.barcode_owner_sku_ids(barcode, executor).await?;
        let repository = self.db.sku_barcode_claims();
        match repository.find_by_id(barcode, executor).await? {
            Some(mut claim) => {
                claim.ensure_owner(revision.sku_id.as_ref(), &owners)?;
                repository.update(&mut claim, executor).await?;
            },
            None => {
                let claim = SkuBarcodeClaim::new(barcode, revision.sku_id.as_ref())?;
                claim.ensure_owner(revision.sku_id.as_ref(), &owners)?;
                repository.create(&claim, executor).await?;
            },
        }
        Ok(())
    }

    /// 保存已有 SKU 的修订前同事务争用条码身份。
    /// # 参数
    /// `revision` 为新不可变修订；`executor` 为 SKU 主表共同使用的事务。
    /// # 返回
    /// 唯一归属核对与修订写入成功。
    /// # 错误
    /// 条码归属冲突、事务缺失或数据库错误。
    pub async fn create_sku_revision(
        &self,
        revision: &SkuRevision,
        executor: &mut dyn Executor,
    ) -> Result<()> {
        self.claim_sku_barcode(revision, executor).await?;
        mongo_ops::insert_one(&self.db.sku_revisions().collection(), revision, executor).await?;
        Ok(())
    }
}
