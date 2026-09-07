# 阶段 15 商城范围核验执行合同（正式输入范围结论）

## 1. 状态与输入约束

本合同适用于阶段 15 的范围核验。正式输入固定为阶段 14 已验收提交 `f5269ee9ab2277c87cb435cd1bf89adfc7483d64`，实际执行树为 `/private/tmp/erp-domain-crate-15-commerce-scope`，对应真正 metadata 为 `/private/tmp/erp-commerce15-metadata.json`。本合同与 JSON 记录实际范围结论；公共门禁由根集成负责人执行并单独登记，不以本审计脚本退出码替代门禁通过。更换输入源码、工作树或 metadata 后必须复验。

核验结论：两个已采样源码树均无 `CardInstance`、`MallOrder`、`MallAfterSales`、`MallBackfill`、`ProductPublication` 的既有生产聚合、专用 Repository/Service、活动集合调用或 HTTP 路由实现。执行方式固定为“无既有实现；不创建 crate”。不得为了补齐历史设计数量生成空 `erp-commerce`，不得将已有销售、财务、供给或集成合同改归商城。

| 输入 | 源码树 | HEAD（采集开始与结束一致） | Rust 文件数 | 工作区状态行数 | 单次采集稳定 |
| --- | --- | --- | ---: | ---: | --- |
| before00 | `/private/tmp/erp-domain-crate-00-measure` | `400ab4f7855255b284fe8a8e1caffe27acc96083` | 1101 | 0 | True |
| accepted15 | `/private/tmp/erp-domain-crate-15-commerce-scope` | `f5269ee9ab2277c87cb435cd1bf89adfc7483d64` | 1771 | 4 | True |

00 输入为原始测量树；两树工作区状态以表内实际采集结果为准。accepted15 行数、哈希与分类只适用于本次采样；不得将单次采集期间未变动等同于全部门禁通过。源文件列表包含未提交及被 gitignore 忽略的新增 Rust 文件（明确排除的 target 除外）；不将 `tests/` 排除出原始搜索。

依据为源码内 `backend/docs/superpowers/plans/domain-crate-migration/15-commerce-scope.md`，其中要求先完成 14 验收、保存精确搜索证据、保持现有业务合同并执行公共门禁。两树 `source-map.tsv` 的 `phase=15` 均为 0 行，`repository-types.tsv` 的 `owner_phase=15` 均为 0 行；此结论来自当前文件内容，不修改或重生成清单。

## 2. 五类对象逐项核销

| 对象 | 00 与 accepted15 词法命中 | 生产类型 / ID newtype / 专用仓储或服务 | 活动集合 / 路由 / Cargo 目标 | 执行动作 |
| --- | --- | --- | --- | --- |
| `CardInstance` | 两树均 0 | 均 0 | 均 0 | 不生成卡实例聚合或卡库存流程 |
| `MallOrder` | 两树各 32 个 cfg(test) token、2 个注释 token；主要为 mall_order_fact 引用样例 | 均 0 | 均 0 | 保留集成证据解析测试；不生成商城订单 |
| `MallAfterSales` | 两树均 0 | 均 0 | 均 0 | 不生成商城售后 |
| `MallBackfill` | 两树均 0 | 均 0 | 均 0 | 不生成回填流程或数据任务 |
| `ProductPublication` | 两树各 1 个错误兼容字符串、1 个内联断言字符串 | 均 0 | 均 0 | 保留错误映射；不生成发布聚合、索引或路由 |

精确五个 CamelCase 名称的 raw rg 在两树均为 exit 1、stdout 为空。大小写不敏感并允许 snake/kebab 写法的搜索在两树各输出 36 行；这些行必须按词法分类解释，不能误称为生产实现。五类对应的 ID newtype 实际也均不存在；计划中“历史 ID 不等于实现”是一条判断规则，不能将其写成“已发现五种历史 ID”的事实。

`DocumentType` 在当前 `crates/erp-workflow/src/entity/document_registry/business_document.rs` 定义 20 个既有单据变体，含 `VoucherSalesOrder`，不含五类商城对象。`VoucherSalesOrder` 是既有卡券销售单注册值，保留 workflow 单据注册归属。

所有生产 `.collection(...)` / `.create_collection(...)` 与 `.route(...)` / `.nest(...)` 等调用参数均保存于 JSON。00 记录 486 个集合调用和 376 个路由调用；accepted15 记录 633 个集合调用和 376 个路由调用。它们是语法调用次数，不是集合数、端点数或线上活动量。五类对象的匹配调用均为 0。动态表达式不执行；宏、字符串常量与未知生产 token 仍须人工复核。真实 MongoDB 集合存在性未验证。

## 3. 既有真实合同与归属

下列路径均相对正式 accepted15 工作树的 `backend/`，已按本次输入复核。00 的同名旧路径及所有 token 位置记录在 JSON。任何下列代码不得因商城范围为空而删除、改写值或迁入空商城 crate。

| 真实符号或事实 | 当前源码 | 保持的归属和语义 |
| --- | --- | --- |
| `SalesOrder / SalesOrderData` | `crates/erp-sales/src/entity/sales_order/entity/order.rs` | 销售；`business_type: BusinessType`、`origin_system: OriginSystem`、`source_identity_id: Option<String>`、`source_status_code: Option<String>` 均是原销售事实。保持卡券变更缺少原正式版本冻结目标或应收到期日的拒绝。 |
| `BusinessType / OriginSystem` | `crates/erp-sales/src/entity/sales_order/types.rs` | 销售；`Voucher`、`GoodsService` 及 `OriginSystem::Mall/Erp` 原稳定值不变，`Mall` 对应 `MALL`。 |
| `SalesOrderVoucherLineRevision` | `crates/erp-sales/src/entity/sales_order` | 销售卡券行及修订归销售；不得据卡券名称认定存在 CardInstance。 |
| `销售来源只读展示` | `crates/erp-read-models/src/sales_center/order/status.rs` | 读模型保留“这单由商城开单，商业数据同步中，本系统只能查看；改内容请在商城处理。”，不扩大可写入口。 |
| `ReceivableFundsReview / persist_card_funds_receipt_plan` | `crates/erp-finance/src/entity/receivable/receivable_funds_review.rs；crates/erp-finance/src/service/receivable/card_funds_register.rs` | 应收资金审核与本域登记事实归 finance。 |
| `register_card_funds_receipt / register_card_funds_invoice / complete_card_funds_review` | `crates/erp-processes/src/finance_posting/receivable/{card_funds_register,card_funds_review}.rs` | 既有卡券资金登记跨域流程仍归 finance_posting；不新增卡实例或商城开卡行为。 |
| `SourceSystemType::Mall / ExternalObjectType::MallUser` | `crates/erp-support/src/entity/source_registry/mod.rs` | support 来源目录；稳定 wire 值 `MALL`、`mall_user`，MallUser 仅来源侧。保留“历史外部来源标识”语义。 |
| `MessageType::CardBalanceRestored / MallActionRequest` | `crates/erp-integration/src/entity/integration_ops/inbox_message.rs` | integration 实际消息协议；稳定 wire 值 `CARD_BALANCE_RESTORED`、`MALL_ACTION_REQUEST`，保留现有标签、信封、幂等与处理分支。 |
| `CardBalanceRestored 证据分支` | `crates/erp-processes/src/integration_resolution/evidence_adapter.rs` | 保留真实集成证据适配与判断；消息类型不能当作 CardInstance 聚合。 |
| `difference_owner_role 的 mall_missing` | `crates/erp-integration/src/entity/integration_ops/w29_work_items.rs` | 实际差异代码注册，仍映射 W29_OPERATIONS_ROLE；不能当成商城订单加载器。 |
| `mall_order_fact / mall-snapshot` | `crates/erp-integration/src/entity/integration_ops/{evidence_reference,decision_policy,reconciliation_difference,inbox_message}.rs 等` | 当前匹配内容为原注释或内联 fixtures/assertions；保留通用证据引用解析行为和测试。 |
| `CostScope::MallConsumption / CostBasis` | `crates/erp-finance/src/entity/cost/cost_entry.rs` | finance 历史成本范围与取值基础；`mall_consumption` wire 值和“商城消费成本必填取值基础”原规则不变。 |
| `validate_create_cost_entry` | `crates/erp-finance/src/service/cost.rs` | 依原顺序先 `req.validate()`，再拒绝新增 `MallConsumption`：“商城消费成本范围已停用”，先于外域存在性查询；历史读取不被本范围核验删除。 |
| `uk_product_publication_revisions_publication_revision` | `services/src/errors.rs` | 唯一生产命中是原重复键兼容映射：“该发布修订序号已被占用，请刷新后重试”。内联测试保留；当前没有相应活动索引定义。是否最终迁 HTTP 错误适配按根集成合同处理，禁止凭计划目标路径谎报已迁。 |
| `mapped_reason_label("supplier_stopped")` | `crates/erp-read-models/src/workbench/presentation.rs` | 真实供应工作台展示“供应已停止，商城在售发布已暂停”；保留展示，不补建发布写入。 |
| `SupplierOffering / SupplierOfferingRevision / SupplierFulfillmentOrder / SupplierSettlementStatement` | `entities/src/{supplier_offering,supplier_fulfillment,supplier_settlement}/**` | 当前尚由旧供应源码持有，按阶段 16 迁 supply、相应 Process 与读模型；本阶段不迁、不复制，不改归商城。 |
| `SupplierFulfillmentService::ensure_placeable` | `services/src/supplier_fulfillment/place.rs` | 原文档提到 D29 商城订单，但实际依次读取连接、能力、供给修订/供给，校验供应商及连接归属；未查询 MallOrder。不得因注释补做存在性查询或改变原首错。 |

### 3.1 名称含 publication 的真实窄查询

这四个接口必须按实际返回类型与集合归属保留；它们证明存在供应链/catalog 查询能力，不证明 ProductPublication 聚合存在。当前整个 Rust 树对这四个方法名的搜索仅命中定义；不得为注释中的未来消费者新增调用。

| 当前方法 | 原参数与返回 | 实际生产查询 | 归属 |
| --- | --- | --- | --- |
| `database/src/repository/supplier_offering/query.rs::find_publication_supplier_offering` | `&SupplierOfferingId, &mut dyn Executor -> Result<Option<SupplierOffering>>` | 原 `find_by_id`，本域 `supplier_offerings` | supply，阶段 16 |
| 同文件 `find_publication_offering_revision` | `&SupplierOfferingRevisionId, &mut dyn Executor -> Result<Option<SupplierOfferingRevision>>` | 原 `find_by_id`，本域 `supplier_offering_revisions` | supply，阶段 16 |
| 同文件 `list_publication_offering_revisions` | `&SupplierOfferingId, &mut dyn Executor -> Result<Vec<SupplierOfferingRevision>>` | 原 `find_many`，`supplier_offering_id` 精确过滤 | supply，阶段 16 |
| `crates/erp-catalog/src/repository/catalog/sku.rs::find_publication_sku_revision` | `&SkuRevisionId, &mut dyn Executor -> Result<Option<SkuRevision>>` | 原 `find_by_id`，本域 `sku_revisions` | catalog |

## 4. Cargo 目标核验合同

00 实际 metadata：`/private/tmp/erp-commerce15-baseline-metadata.json`，SHA256 `33d041e3c9aebb5a9b2fe5001878d9ff2236fd118088efd6addcf098d91615bc`。根集成负责人在 00 的 backend 执行 `cargo metadata --format-version 1 --all-features --locked`，退出码 0；本审计脚本只消费该 JSON，不执行 Cargo。文件含实际 `packages/workspace_members/targets`，workspace_root 与 00 工作树匹配。旧 customer-check measurement 摘要没有 targets，不作为目标证明；13/14 metadata 均未替代 accepted15 的真正 metadata。

00 共 13 个 workspace package、44 个实际目标：15 个生产目标与 29 个历史集成测试目标。原始 00 metadata 中存在历史 test target 是基线事实；迁移执行中不得运行它们。不能将后续 autotests=false 倒写成原始 00 没有 test targets。

| 00 package | 实际生产 target（kind） | 历史 test targets 数 |
| --- | --- | ---: |
| `bpm` | `bpm` (lib) | 0 |
| `cli` | `cli` (bin) | 0 |
| `config` | `config` (lib) | 0 |
| `database` | `database` (lib) | 13 |
| `entities` | `entities` (lib) | 0 |
| `entity-core` | `entity_core` (lib) | 0 |
| `entity-macros` | `entity_macros` (proc-macro) | 0 |
| `id-generator` | `id_generator` (lib) | 2 |
| `permission-macros` | `permission_macros` (proc-macro) | 0 |
| `services` | `services` (lib) | 13 |
| `storage` | `storage` (lib) | 0 |
| `test-support` | `test_support` (lib) | 1 |
| `web-api` | `web_api` (lib)；`web-api` (bin)；`build-script-build` (custom-build) | 0 |

accepted15 实际 metadata：`/private/tmp/erp-commerce15-metadata.json`，SHA256 `7ffbf33029023fe12d9c82c4749db3e6e3eb96711d571b607af4335b7a4de6c6`，workspace_root 与被审计的 accepted15 backend 匹配。该实际图包含 36 个 workspace package、38 个生产目标（33 lib、2 bin、2 proc-macro、1 custom-build），没有历史 integration test target；与同次 TOML 静态清单一致。完整名称、路径、kind、autotests 与显式 test 配置均在 JSON。该 metadata 由根集成负责人真实采集，本 worker 不执行 Cargo；源码绑定为第 1 节实际提交。

| accepted15 实际 target kind | 名称 |
| --- | --- |
| `bin` | `cli`, `web-api` |
| `custom-build` | `build-script-build` |
| `lib` | `application_core`, `bpm`, `config`, `database`, `entities`, `entity_core`, `erp_audit`, `erp_catalog`, `erp_contract`, `erp_core`, `erp_customer`, `erp_finance`, `erp_fulfillment`, `erp_identity`, `erp_import`, `erp_integration`, `erp_inventory`, `erp_party`, `erp_processes`, `erp_procurement`, `erp_read_models`, `erp_returns`, `erp_sales`, `erp_supplier`, `erp_support`, `erp_warehouse`, `erp_workflow`, `id_generator`, `persistence_core`, `services`, `storage`, `test_support`, `web_api` |
| `proc-macro` | `entity_macros`, `permission_macros` |

accepted15 当前 package 清单：`cli`, `web-api`, `bpm`, `erp-core`, `application-core`, `persistence-core`, `erp-identity`, `erp-audit`, `erp-workflow`, `erp-processes`, `erp-read-models`, `erp-support`, `erp-party`, `erp-customer`, `erp-supplier`, `erp-catalog`, `erp-warehouse`, `erp-contract`, `erp-import`, `erp-inventory`, `erp-finance`, `erp-sales`, `erp-procurement`, `erp-fulfillment`, `erp-returns`, `erp-integration`, `entity-macros`, `entity-core`, `permission-macros`, `storage`, `id-generator`, `test-support`, `database`, `entities`, `services`, `config`。

其中已有 18 个业务领域 crate；阶段 16 的 `erp-supply` 尚未建立。阶段 15 要求的最终 19 指有既有业务实现的领域 crate 总数，不能与当前全部 workspace package 或生产 target 数混用。最终依赖图仍须由根集成负责人按 normal/build/dev 三类边验收。

## 5. 词法与搜索执行合同

审计器固定扫描 `backend/**/*.rs`，包含 hidden/no-ignore 文件及历史 tests 档案，排除 target 与 .git。逐 token 识别行注释、嵌套块注释、普通/raw/byte/C 字符串、字符与 lifetime、平衡括号；只将精确正向 `#[cfg(test)]` 作用的 item/block 及其外置 mod 递归判为测试。`include_str!` 不被当作生产模块包含。未识别 cfg 组合继续按生产候选处理。

分类白名单仅涵盖本次逐项核验的稳定枚举、差异代码、错误兼容字符串、既有文案与四个 publication 查询。`WorkItemAllowedAction` 是大小写折叠后跨词边界产生的 mall 子串，单独登记为不相关标识符；不得用广义“含 small/不以 Mall 起头”等规则过滤潜在真实对象。新增未知 production token、目标类型或匹配的集合/路由调用均导致 exit 2。

| 分类 | 00 token 数 | accepted15 token 数 |
| --- | ---: | ---: |
| `cfg_test_fixture_or_assertion` | 120 | 135 |
| `existing_catalog_publication_query` | 1 | 1 |
| `existing_domain_label_or_fail_closed_rule` | 9 | 9 |
| `existing_supply_publication_query` | 3 | 3 |
| `existing_supply_workbench_display_text` | 1 | 1 |
| `integration_registered_difference_code` | 1 | 1 |
| `legacy_error_index_compatibility` | 1 | 1 |
| `rust_comment` | 59 | 60 |
| `stable_enum_value_or_reference` | 21 | 21 |
| `stable_enum_wire_value` | 6 | 6 |
| `unrelated_identifier_substring` | 36 | 49 |

上述为 token 数；raw rg 输出为行数，两种计数不可相互替代。每个 token 保存 path/line/column/kind/source_line/classification/families。JSON 同时保留所有 ID 宏、相关声明、字符串常量、集合/路由参数及 Cargo 清单。

本次脚本退出码为 0；两树 lexical_errors、production_unclassified、commerce_implementation_candidates 均为空。lexer 内置正向控制包含真实 MallOrder 声明与 cfg(test) MallOrderFixture，另覆盖嵌套注释、raw/byte 字符串和字符/lifetime。此为审计器检查，不是 ERP 单元测试或数据库行为证明。

脚本不是 Rust 编译器，不展开宏、不执行 cfg 表达式、不求值动态集合字符串，不连接数据库或启动 HTTP。Rust 全文件、Cargo.toml、Cargo.lock、阶段计划在单次采集前后校验；文件变化或 HEAD 变化必须重新采样。源码哈希/静态零命中不能替代公共门禁或真实业务测试。

### 5.1 精确命令与结果

下列命令从每个对应 worktree 根目录运行；它们已由脚本实际执行。完整 argv/cwd/exit/stdout/stderr 存在 JSON 的 `trees[].commands`，不得将 exit 1 的无匹配解释为工具错误，也不得忽略 exit 2。

| 命令 | 00 exit / 输出行数 | accepted15 exit / 输出行数 |
| --- | --- | --- |
| `rg --files --hidden --no-ignore backend -g '*.rs' -g '!**/target/**' -g '!**/.git/**'` | 0 / 1101 | 0 / 1771 |
| `rg -n --hidden --no-ignore --no-heading --color never 'CardInstance\|MallOrder\|MallAfterSales\|MallBackfill\|ProductPublication' backend -g '*.rs' -g '!**/target/**'` | 1 / 0 | 1 / 0 |
| `rg -n --hidden --no-ignore --no-heading --color never -i 'card[_-]?instance\|mall[_-]?order\|mall[_-]?after[_-]?sales\|mall[_-]?backfill\|product[_-]?publication' backend -g '*.rs' -g '!**/target/**'` | 0 / 36 | 0 / 36 |
| `rg -n --hidden --no-ignore --no-heading --color never -i 'mall\|commerce\|card[_-]?instance\|publication\|card[_-]?balance[_-]?restored\|商城' backend -g '*.rs' -g '!**/target/**'` | 0 / 239 | 0 / 265 |
| `rg -n --hidden --no-ignore --no-heading --color never -i 'card[_-]?instance\|mall[_-]?order\|mall[_-]?after[_-]?sales\|mall[_-]?backfill\|product[_-]?publication' backend -g Cargo.toml -g '!**/target/**'` | 1 / 0 | 1 / 0 |
| `rg -n --hidden --no-ignore --no-heading --color never -i 'mall\|commerce\|card[_-]?instance\|publication\|card[_-]?balance[_-]?restored\|商城' backend/apps/web-api/src/core/routes backend/apps/web-api/src/app_state.rs backend/apps/web-api/src/lib.rs backend/apps/web-api/src/main.rs` | 1 / 0 | 1 / 0 |
| `rg --files --hidden --no-ignore backend -g '*.rs' -g '!**/target/**' -g '!**/.git/**'` | 0 / 1101 | 0 / 1771 |

## 6. 正式阶段 15 复验步骤

1. 输入固定为 `f5269ee9ab2277c87cb435cd1bf89adfc7483d64`；在 `/private/tmp/erp-domain-crate-15-commerce-scope` 执行。本次未提交文档状态修改由根集成负责人所有，不影响业务源码归属；继续复验时必须重新核对 HEAD 与 diff。
2. 由根集成负责人在被审计 worktree 同一路径的最终输入 backend 采集实际 `cargo metadata --format-version 1 --all-features --locked`，记录命令、cwd、exit、环境与源码 commit；保存为 `/private/tmp/erp-commerce15-metadata.json` 或另一个明确文件。脚本拒绝 workspace_root 与被审计树不匹配的已提供 metadata。
3. 本次已按下列真实路径执行并得到 exit 0；继续复验应复用该命令，脚本只写指定 `/tmp` JSON。

```bash
python3 /private/tmp/audit-commerce15-scope.py \
  --tree before00=/private/tmp/erp-domain-crate-00-measure \
  --tree accepted15=/private/tmp/erp-domain-crate-15-commerce-scope \
  --metadata before00=/private/tmp/erp-commerce15-baseline-metadata.json \
  --metadata accepted15=/private/tmp/erp-commerce15-metadata.json \
  --out /private/tmp/commerce15-scope-final.json
```

4. 审查全部新出现或变化的生产 token、实际 Cargo targets 与三类依赖边；校核前后 source-map/repository-types phase 15 行及本合同第 3 节真实合同。若发现真实实现，先登记文件、符号、调用方、集合/索引及业务合同，更新阶段范围后迁移；不得新增功能补全想象中的商城。
5. 由根负责人执行阶段 15 与公共执行合同的质量门禁、既有卡券销售和资金登记内联测试，保存逐条命令/exit/日志；历史 tests 原样保留且不运行，真实数据库运行未验证。
6. 只有实际输入、完整范围证据、门禁、归属核销及阶段提交齐全后才更新阶段 15 为已验收。本范围审计不修改业务源码、注册、Cargo、索引、注释或历史 tests。

## 7. 证据完整性

| 产物 | SHA256 |
| --- | --- |
| `/private/tmp/audit-commerce15-scope.py` | `17feb86e1a4ca7f9bafba9b3144f35f7aa38c4992b722abc00a6d2a393cdb9cc` |
| `/private/tmp/commerce15-scope-final.json` | `9614f4583edb2dd347eb4c1cfa036819b092ba3252a7e7df3dbe7ef52fe14629` |
| `/private/tmp/erp-commerce15-metadata.json` | `7ffbf33029023fe12d9c82c4749db3e6e3eb96711d571b607af4335b7a4de6c6` |
| `before00` Rust 文件清单摘要（逐文件 SHA 在 JSON） | `a4fa0db28ddce503dca9a4c9031ed7e64613c5c802f52ae56d22561795f2a424` |
| `accepted15` Rust 文件清单摘要（逐文件 SHA 在 JSON） | `18a34eeeb74c3870a8d55a21a71626f5b947d48a828357793227b6a407000683` |
| 两树 `source-map.tsv` | `571e19695c50295508d7916cff5d3482047d6200ab0880a4a3891cc471f375d7` |
| 两树 `repository-types.tsv` | `1df4e0a648008d89b5ea5130412c1a4d629d89dfdb5d629f3de66b4637e98865` |

本合同由上述 JSON 与实际源码复核生成；合同自身 SHA 由交付通知单独记录，避免自引用哈希。不得修改 JSON 后继续使用本表旧 SHA；复验时输出新 JSON 并更新证据合同。
