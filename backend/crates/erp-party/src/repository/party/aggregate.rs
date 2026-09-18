use std::collections::HashMap;

use erp_core::ids::PartyId;
use mongodb::Database;
use mongodb::bson::doc;
use persistence_core::{Executor, Result, mongo_ops};

use super::super::extensions::PartyExt;
use super::{
    PARTIES, PARTY_REVISIONS, PartyAddressRepositoryExt, PartyBankAccountRepositoryExt,
    PartyContactRepositoryExt, PartyDomainRepository, PartyRepositoryExt, PartyRevisionRepositoryExt,
    PartyTaxProfileRepositoryExt,
};
use crate::entity::party::{
    Party, PartyAddress, PartyBankAccount, PartyContact, PartyRevision, PartyTaxProfile,
};
use crate::repository::owned::{PartyRepository, PartyRevisionRepository};

impl<'a> PartyDomainRepository<'a> {
    /// 按稳定 ID 读取未删除主体。
    ///
    /// # 参数
    /// * `party_id` - 主体 ID
    /// * `executor` - 数据访问执行器，由 Service 决定是否位于事务中
    ///
    /// # 返回
    /// 返回匹配主体；不存在时返回 `None`。
    ///
    /// # 错误
    /// 当 MongoDB 查询或反序列化失败时返回错误。
    pub async fn party(&self, party_id: &PartyId, executor: &mut dyn Executor) -> Result<Option<Party>> {
        PartyRepository::new(self.db, PARTIES).find_party(party_id, executor).await
    }

    /// 按稳定 ID 读取联系人（erp-party-012）。
    ///
    /// 仅保留 Service 明确消费的单条读取；调用方已有集合访问器时优先直用
    /// 集合仓储，避免经本聚合再转发的无意义包装。
    ///
    /// # 参数
    /// * `record_id` - 联系人 ID
    /// * `executor` - 数据访问执行器，由 Service 决定是否位于事务中
    ///
    /// # 返回
    /// 返回匹配联系人；不存在时返回 `None`。
    ///
    /// # 错误
    /// 当 MongoDB 查询失败时返回错误。
    pub async fn contact(
        &self,
        record_id: &str,
        executor: &mut dyn Executor,
    ) -> Result<Option<PartyContact>> {
        self.db.party_contacts().find_by_id(record_id, executor).await
    }

    /// 按稳定 ID 读取地址（erp-party-012）。
    ///
    /// 同联系人读取：保留的单条聚合入口，优先直用集合仓储。
    ///
    /// # 参数
    /// * `record_id` - 地址 ID
    /// * `executor` - 数据访问执行器，由 Service 决定是否位于事务中
    ///
    /// # 返回
    /// 返回匹配地址；不存在时返回 `None`。
    ///
    /// # 错误
    /// 当 MongoDB 查询失败时返回错误。
    pub async fn address(
        &self,
        record_id: &str,
        executor: &mut dyn Executor,
    ) -> Result<Option<PartyAddress>> {
        self.db.party_addresses().find_by_id(record_id, executor).await
    }

    /// 按稳定 ID 读取银行账户（erp-party-012）。
    ///
    /// 同联系人读取：保留的单条聚合入口，优先直用集合仓储。
    ///
    /// # 参数
    /// * `record_id` - 银行账户 ID
    /// * `executor` - 数据访问执行器，由 Service 决定是否位于事务中
    ///
    /// # 返回
    /// 返回匹配银行账户；不存在时返回 `None`。
    ///
    /// # 错误
    /// 当 MongoDB 查询失败时返回错误。
    pub async fn bank_account(
        &self,
        record_id: &str,
        executor: &mut dyn Executor,
    ) -> Result<Option<PartyBankAccount>> {
        self.db.party_bank_accounts().find_by_id(record_id, executor).await
    }

    /// 批量读取主体及其当前修订。
    ///
    /// 先批量读取 Party，再按 `current_revision_id` 一次性读取当前修订，
    /// 避免 Service 暴露修订指针查询或形成逐行 N+1。
    ///
    /// # 参数
    /// * `party_ids` - 稳定 Party ID 集合；为空时直接返回两个空集合
    /// * `executor` - 数据访问执行器，由 Service 决定是否位于事务中
    ///
    /// # 返回
    /// 返回主体集合与其命中的当前修订集合。
    ///
    /// # 错误
    /// 当 MongoDB 查询或游标读取失败时返回错误。
    pub async fn list_with_current_revisions(
        &self,
        party_ids: &[PartyId],
        executor: &mut dyn Executor,
    ) -> Result<(Vec<Party>, Vec<PartyRevision>)> {
        let parties = PartyRepository::new(self.db, PARTIES).list_by_ids(party_ids, executor).await?;
        let revision_ids: Vec<String> =
            parties.iter().filter_map(|party| party.stable.current_revision_id.clone()).collect();
        let revisions = PartyRevisionRepository::new(self.db, PARTY_REVISIONS)
            .list_by_ids(&revision_ids, executor)
            .await?;
        Ok((parties, revisions))
    }

    /// 读取单个主体及其当前修订。
    ///
    /// # 参数
    /// * `party_id` - 稳定 Party ID
    /// * `executor` - 数据访问执行器，由 Service 决定是否位于事务中
    ///
    /// # 返回
    /// 主体不存在时返回 `None`；存在时返回主体与当前修订候选，修订指针
    /// 缺失或目标不存在时第二项为 `None`。
    ///
    /// # 错误
    /// 当 MongoDB 查询失败时返回错误。
    pub async fn find_with_current_revision(
        &self,
        party_id: &PartyId,
        executor: &mut dyn Executor,
    ) -> Result<Option<(Party, Option<PartyRevision>)>> {
        let Some(party) = PartyRepository::new(self.db, PARTIES).find_party(party_id, executor).await? else {
            return Ok(None);
        };
        let revision = match party.stable.current_revision_id.as_deref() {
            Some(revision_id) => {
                PartyRevisionRepository::new(self.db, PARTY_REVISIONS)
                    .find_revision(revision_id, executor)
                    .await?
            },
            None => None,
        };
        Ok(Some((party, revision)))
    }

    /// 批量读取指定日期的当前 Party 从属事实。
    ///
    /// 联系人、地址、税务资料与银行账户分别执行一次按有效期筛选的排序
    /// 查询，Service 只消费聚合结果，不接触 MongoDB 条件与排序细节。
    ///
    /// # 参数
    /// * `party_id` - 稳定 Party ID
    /// * `as_of` - 业务日期
    /// * `executor` - 数据访问执行器，由 Service 决定是否位于事务中
    ///
    /// # 返回
    /// 返回联系人、地址、税务资料与银行账户四类当前事实。
    ///
    /// # 错误
    /// 当任一 MongoDB 查询或游标读取失败时返回错误。
    pub async fn load_current_facts(
        &self,
        party_id: &PartyId,
        as_of: erp_core::common::time::BusinessDate,
        executor: &mut dyn Executor,
    ) -> Result<(Vec<PartyContact>, Vec<PartyAddress>, Vec<PartyTaxProfile>, Vec<PartyBankAccount>)> {
        let contacts = self.db.party_contacts().list_current_on(party_id, as_of, executor).await?;
        let addresses = self.db.party_addresses().list_current_on(party_id, as_of, executor).await?;
        let tax_profiles = self.db.party_tax_profiles().list_current_on(party_id, as_of, executor).await?;
        let bank_accounts = self.db.party_bank_accounts().list_current_on(party_id, as_of, executor).await?;
        Ok((contacts, addresses, tax_profiles, bank_accounts))
    }

    /// 按当前法定名称或简称匹配主体 ID。
    ///
    /// 先匹配名称修订，再只保留 `parties.current_revision_id` 指向命中修订
    /// 的主体，避免历史名称误命中当前客户搜索。
    ///
    /// # 参数
    /// * `keyword` - 名称关键词，按字面量忽略大小写匹配
    /// * `executor` - 数据访问执行器，由 Service 决定是否位于事务中
    ///
    /// # 返回
    /// 返回去重并按稳定 ID 排序的当前主体 ID。
    ///
    /// # 错误
    /// 当 MongoDB 查询或游标读取失败时返回错误。
    pub async fn matching_current_party_ids_by_name(
        &self,
        keyword: &str,
        executor: &mut dyn Executor,
    ) -> Result<Vec<PartyId>> {
        let escaped = regex::escape(keyword);
        let revisions = PartyRevisionRepository::new(self.db, PARTY_REVISIONS)
            .find_many(
                doc! {
                    "$or": [
                        { "legal_name": { "$regex": &escaped, "$options": "i" } },
                        { "short_name": { "$regex": &escaped, "$options": "i" } },
                    ]
                },
                executor,
            )
            .await?;
        current_party_ids_for_revisions(
            self.db,
            revisions.into_iter().map(|revision| revision.base.id),
            executor,
        )
        .await
    }

    /// 精确匹配当前法定名称，兼容空白及全半角括号。
    ///
    /// # Errors
    /// 名称查询或主体读取失败时返回仓储错误。
    pub async fn exact_current_party_ids_by_name(
        &self,
        name: &str,
        executor: &mut dyn Executor,
    ) -> Result<Vec<PartyId>> {
        let Some(pattern) = exact_name_pattern(name) else {
            return Ok(Vec::new());
        };
        let revisions = PartyRevisionRepository::new(self.db, PARTY_REVISIONS)
            .find_many(doc! { "legal_name": { "$regex": pattern, "$options": "i" } }, executor)
            .await?;
        let ids = revisions.into_iter().map(|r| r.base.id).collect::<Vec<_>>();
        if ids.is_empty() {
            return Ok(Vec::new());
        }
        Ok(PartyRepository::new(self.db, PARTIES)
            .find_many(doc! { "current_revision_id": { "$in": ids } }, executor)
            .await?
            .into_iter()
            .map(|p| PartyId::new(p.base.id))
            .collect())
    }

    /// 按主体 ID 批量读取当前修订的法定名称。
    ///
    /// 只返回未删除主体、其当前修订指针仍指向同主体未删除修订时形成的
    /// `主体 ID -> 法定名称` 投影。缺失主体、缺少当前修订指针或修订归属
    /// 与指针不一致时不生成键；法定名称按持久化原值返回（含空字符串），
    /// 不在仓储层执行空白回退等业务决策，供供应商列表等事实束按账号关联
    /// 后直接拼装展示。
    ///
    /// # 参数
    /// * `party_ids` - 稳定主体 ID 集合；允许重复，空集合直接返回空映射
    /// * `executor` - 数据访问执行器，由 Service 决定是否位于事务中
    ///
    /// # 返回
    /// 返回主体 ID 字符串到当前法定名称的映射；关联不完整的主体无键。
    ///
    /// # 错误
    /// 当主体或修订批量查询失败时返回错误。
    ///
    /// # 约束
    /// 只经主体域属主查询（`parties`/`party_revisions`）组装，不直查外域集合。
    pub async fn current_legal_names_by_party_ids(
        &self,
        party_ids: &[PartyId],
        executor: &mut dyn Executor,
    ) -> Result<HashMap<String, String>> {
        if party_ids.is_empty() {
            return Ok(HashMap::new());
        }
        let parties = PartyRepository::new(self.db, PARTIES).list_by_ids(party_ids, executor).await?;
        let revision_ids: Vec<String> =
            parties.iter().filter_map(|party| party.stable.current_revision_id.clone()).collect();
        if revision_ids.is_empty() {
            return Ok(HashMap::new());
        }
        let revisions = PartyRevisionRepository::new(self.db, PARTY_REVISIONS)
            .list_by_ids(&revision_ids, executor)
            .await?;
        let revision_names: HashMap<(String, String), &str> = revisions
            .iter()
            .map(|revision| {
                ((revision.party_id.to_string(), revision.base.id.clone()), revision.legal_name.as_str())
            })
            .collect();
        Ok(parties
            .iter()
            .filter_map(|party| {
                let revision_id = party.stable.current_revision_id.as_deref()?;
                let legal_name = revision_names.get(&(party.base.id.clone(), revision_id.to_string()))?;
                Some((party.base.id.clone(), (*legal_name).to_string()))
            })
            .collect())
    }

    /// 追加主体修订并切换当前生效版本（跨集合多步骤写入）。
    ///
    /// 依次写入 `party_revisions` 并 CAS 更新 `parties.current_revision_id`
    /// （基类乐观锁按 `id + version` 判定），保证「修订 + 生效指针」原子可见
    /// （数据模型 §6.2 稳定基础资料 + 不可变修订）。
    /// **必须收到事务执行器**：本方法不构成原子边界，传入 `NoTransaction`
    /// 时修订先各自提交，后续主体版本冲突会留下「新修订存在但主体指针未更新」
    /// 的半成品；Service 必须通过 `persistence_core::Transactional::with_transaction`
    /// 传入事务会话。
    ///
    /// # 参数
    /// * `party` - 待更新生效指针的主体（按当前版本做 CAS）
    /// * `revision` - 待写入的修订
    /// * `updated_by` - 本次变更执行人
    /// * `executor` - 数据访问执行器，必须位于事务中
    ///
    /// # 错误
    /// 当修订违反 `(party_id, revision_no)` 唯一索引（透出
    /// [`persistence_core::Error::DuplicateKey`]）、主体版本冲突（返回
    /// [`persistence_core::Error::OptimisticLockingError`]）或 MongoDB 写入失败时返回错误。
    pub async fn append_party_revision(
        &self,
        party: &mut Party,
        revision: &PartyRevision,
        updated_by: &str,
        executor: &mut dyn Executor,
    ) -> Result<()> {
        mongo_ops::insert_one(&self.db.collection::<PartyRevision>(PARTY_REVISIONS), revision, executor)
            .await?;
        party.stable.current_revision_id = Some(revision.base.id.clone());
        party.stable.touch(updated_by);
        PartyRepository::new(self.db, PARTIES).update(party, executor).await
    }
}

impl PartyDomainRepository<'_> {
    /// 按当前名称或统一社会信用代码查找未删除主体。
    ///
    /// 返回去重主体 ID；名称复用当前修订规则。数据库错误向调用方传播。
    pub async fn matching_current_party_ids(
        &self,
        keyword: &str,
        executor: &mut dyn Executor,
    ) -> Result<Vec<PartyId>> {
        let mut ids = self.matching_current_party_ids_by_name(keyword, executor).await?;
        let collection = self.db.collection::<mongodb::bson::Document>(PARTIES);
        let mut filter = doc! { "deleted_at": entity_core::NOT_DELETED_TIMESTAMP_BSON };
        persistence_core::insert_literal_regex_filter(&mut filter, "unified_credit_code", Some(keyword));
        let mut query = collection.distinct("id", filter);
        if let Some(session) = executor.session() {
            query = query.session(session);
        }
        ids.extend(query.await?.into_iter().filter_map(|id| id.as_str().map(|s| PartyId::new(s.to_owned()))));
        ids.sort_by_key(ToString::to_string);
        ids.dedup();
        Ok(ids)
    }
}

/// 构造精确名称匹配的正则并兼容空白及全半角括号（erp-party-005）。
///
/// 字面量转义、全半角括号展开与 `\s*` 包裹只在此一处实现；空白名称返回 `None`。
fn exact_name_pattern(name: &str) -> Option<String> {
    let parts = name
        .chars()
        .filter(|c| !c.is_whitespace())
        .map(|c| match c {
            '(' | '（' => "[(（]".to_string(),
            ')' | '）' => "[)）]".to_string(),
            c => regex::escape(&c.to_string()),
        })
        .collect::<Vec<_>>();
    if parts.is_empty() {
        return None;
    }
    Some(format!(r"^\s*{}\s*$", parts.join(r"\s*")))
}

/// 修订命中映射到当前主体并排序去重（erp-party-005）。
///
/// “修订→主体（按 `current_revision_id` 回指）→排序去重”两步查询组装的
/// 唯一入口；查询语义与排序去重保持不变。
///
/// # 参数
/// * `revision_ids` - 命中的修订 ID
/// * `executor` - 数据访问执行器，由调用方决定是否位于事务中
///
/// # 返回
/// 返回去重并按稳定 ID 排序的当前主体 ID。
///
/// # 错误
/// 当 MongoDB 查询失败时返回错误。
async fn current_party_ids_for_revisions(
    db: &Database,
    revision_ids: impl IntoIterator<Item = String>,
    executor: &mut dyn Executor,
) -> Result<Vec<PartyId>> {
    let revision_ids = revision_ids.into_iter().collect::<Vec<_>>();
    if revision_ids.is_empty() {
        return Ok(Vec::new());
    }
    let parties = PartyRepository::new(db, PARTIES)
        .find_many(doc! { "current_revision_id": { "$in": revision_ids } }, executor)
        .await?;
    let mut party_ids: Vec<PartyId> = parties.into_iter().map(|party| PartyId::new(party.base.id)).collect();
    party_ids.sort_by_key(ToString::to_string);
    party_ids.dedup();
    Ok(party_ids)
}
