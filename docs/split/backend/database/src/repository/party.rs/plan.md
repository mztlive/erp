# `backend/database/src/repository/party.rs` 拆分分析

## 文件信息

| 项目 | 内容 |
|---|---|
| 源文件 | `backend/database/src/repository/party.rs` |
| 扫描行数 | 1151 |
| 分析状态 | 已完成深入分析 |
| 拆分结论 | split |
| 预估工作量 | M |
| 风险 | medium |
| 生成来源 | workflow `analyze-large-files` |
| 生成日期 | 2026-08-11 |

## 拆分方案

- 结论：**split**（工作量 M，风险 medium）
- 摘要：建议参照 database/src/repository/purchase_order/ 的目录模块模式，把 1151 行单文件改为以 party/mod.rs 为模块根的 6 文件结构。主体、修订、联系人、地址分别按集合形成独立内聚模块；税务资料和银行账户因都属于带有效期、默认标记和状态的财务从属事实，合并到 financial.rs，以满足 proposal 最多 6 个文件的限制。PartyRepository、集合常量、共享排序 helper 和统一 re-export 留在 mod.rs。拆分后预计最大文件约 300 行，所有文件均明显低于 800 行，并可保持 PartyExt 关联类型、Repository 固有方法及 Service 调用方式不变。
- 拆分建议：
  - **backend/database/src/repository/party/mod.rs**：作为 D07 party 仓储模块根：声明 mod party、mod revision、mod contact、mod address、mod financial；通过 pub use 重新导出 PartyRow、PartyFilter、PartyRevisionRow、PartyRevisionFilter、PartyContactRow、PartyContactFilter、PartyAddressRow、PartyAddressFilter、PartyTaxProfileRow、PartyTaxProfileFilter、PartyBankAccountRow、PartyBankAccountFilter；保留 PARTIES、PARTY_REVISIONS 常量；放置 PartyRepository<'a>、PartyRepository::new、PartyRepository::append_party_revision；放置共享私有 helper sort_doc 及其测试 sort_doc_falls_back_to_created_at_when_field_is_not_whitelisted。
    - 依赖/注意：继续从 super::extensions::PartyExt 获取集合名的唯一权威常量，并依赖 Database、Party、PartyRevision、Executor、mongo_ops、Repository、Result。extensions/party.rs 当前通过 super::super::party 导入 Filter 和 PartyRepository，因此这些名称必须在模块根 re-export。该双向模块名称依赖与 purchase_order 模式一致，不构成运行时循环；不要让 extensions/party.rs 改为直接引用私有子模块。sort_doc 保持模块私有，子模块使用 super::sort_doc，避免扩大 crate 公共 API。
  - **backend/database/src/repository/party/party.rs**：放置主体稳定主数据查询簇：PartyRow、PartyFilter、impl QueryFilter for PartyFilter、impl Pagination for PartyFilter、impl Repository<'a, Party>；具体方法为 search_parties、find_by_party_no、find_by_party_no_including_deleted、find_by_unified_credit_code_including_deleted；同时放置私有 helper party_projection 和测试 party_filter_applies_keyword_regex_and_status。
    - 依赖/注意：依赖 PartyKind、PartyStatus、Party、NOT_DELETED_TIMESTAMP_BSON、insert_literal_regex_filter、PageResult、Pagination、QueryFilter、Repository、Executor、mongo_ops。排序通过 super::sort_doc 调用模块根 helper；party_projection 必须留在本文件并保持私有。不要依赖 revision 子模块，主体与修订的跨集合协调仍统一由模块根 PartyRepository 完成。
  - **backend/database/src/repository/party/revision.rs**：放置不可变主体修订查询簇：PartyRevisionRow、PartyRevisionFilter、impl QueryFilter for PartyRevisionFilter、impl Pagination for PartyRevisionFilter、impl Repository<'a, PartyRevision>；具体方法为 search_party_revisions、find_by_party_and_revision、list_revision_history；同时放置私有 helper party_revision_projection。
    - 依赖/注意：依赖 PartyId、PartyRevision、NOT_DELETED_TIMESTAMP_BSON、insert_literal_regex_filter、PageResult、Pagination、QueryFilter、Repository、Executor、mongo_ops。排序使用 super::sort_doc。该文件只负责单集合查询，不应迁入 append_party_revision；后者同时写 party_revisions 和 parties，必须留在模块根并继续要求事务执行器。
  - **backend/database/src/repository/party/contact.rs**：放置联系人查询簇：PartyContactRow、PartyContactFilter、impl QueryFilter for PartyContactFilter、impl Pagination for PartyContactFilter、impl Repository<'a, PartyContact> 中的 search_party_contacts，以及私有 helper party_contact_projection。
    - 依赖/注意：依赖 PartyId、PartyContact、EffectiveRecordStatus、NOT_DELETED_TIMESTAMP_BSON、insert_literal_regex_filter、PageResult、Pagination、QueryFilter、Repository、Executor、mongo_ops。排序使用 super::sort_doc。mobile_query_hmac 只进入精确过滤，mobile_ciphertext 和 mobile_query_hmac 仍不得进入列表投影；投影 helper 跟随本模块可避免敏感字段规则被其他模块误用。
  - **backend/database/src/repository/party/address.rs**：放置地址查询簇：PartyAddressRow、PartyAddressFilter、impl QueryFilter for PartyAddressFilter、impl Pagination for PartyAddressFilter、impl Repository<'a, PartyAddress> 中的 search_party_addresses，以及私有 helper party_address_projection。
    - 依赖/注意：依赖 PartyId、PartyAddress、AddressType、EffectiveRecordStatus、NOT_DELETED_TIMESTAMP_BSON、PageResult、Pagination、QueryFilter、Repository、Executor、mongo_ops。排序使用 super::sort_doc。address_ciphertext 和 address_query_hmac 必须继续排除在列表投影之外；本模块无需引用 contact 或 financial，避免子模块间循环依赖。
  - **backend/database/src/repository/party/financial.rs**：合并两个财务从属事实查询簇。税务部分放置 PartyTaxProfileRow、PartyTaxProfileFilter、对应 QueryFilter/Pagination impl、Repository<PartyTaxProfile>::search_party_tax_profiles、party_tax_profile_projection；银行部分放置 PartyBankAccountRow、PartyBankAccountFilter、对应 QueryFilter/Pagination impl、Repository<PartyBankAccount> 的 search_party_bank_accounts、find_by_bank_account_no、find_by_account_hmac，以及 party_bank_account_projection。
    - 依赖/注意：依赖 PartyId、PartyTaxProfile、PartyBankAccount、EffectiveRecordStatus、NOT_DELETED_TIMESTAMP_BSON、PageResult、Pagination、QueryFilter、Repository、Executor、mongo_ops，并通过 super::sort_doc 共享排序校验。两个投影 helper 均保持文件私有。银行账户列表必须继续排除 account_number_ciphertext 和 account_number_query_hmac，仅返回 account_number_last4；find_by_account_hmac 仍只使用 keyed HMAC 精确查询。若未来该文件接近 800 行，可再拆为 tax_profile.rs 与 bank_account.rs，但当前约 300 行，无需提前增加第七个文件。

## 实施约束

- 拆分应保持现有公开 API 和 re-export 路径，除非实施前另行确认契约变更。
- Service 继续负责事务边界；Repository 只负责数据访问；纯业务规则优先下沉到 Entity 或 Value Object。
- 私有 helper 应跟随唯一调用方迁移；仅在同一领域多个子模块复用时使用 `pub(super)`。
- 拆分完成后执行 `cargo fmt --all -- --check`、`cargo check --workspace`、`cargo clippy --workspace --all-targets --all-features` 和 `cargo test --workspace`。
