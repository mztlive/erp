# S4 扩展管理实施及验收合同

后续候选改造：普通筛选候选的独立接口、候选自身 DataScope、历史及命令候选例外按[列表筛选候选独立查询执行合同](query-selector-decoupling-contract.md)执行。本文随列表返回候选等记录保留 S4 交付基线，不构成后续实现要求；新专项完成度单独登记。

状态：已完成（本地检查通过；真实验收转上线准入跟踪，不阻塞完成；未登记已验收，非正式上线）

实施日期：2026-09-17

修订日期：2026-09-17（结算复核任务身份改写结算单内部组织；阶段退出重验）

上位合同：[组织架构、数据范围与负责人查询执行合同](organization-data-scope-contract.md) v1.3 第 3、5—12 章。公共解析接入必须执行第 9.2—9.5 节及 A33—A36。门禁执行 [第 9 章门禁执行合同](organization-data-scope-gate.md)。

完成度口径：S4 完成只核销 M13—M18 功能与架构本地检查（静态门禁、库单元、隔离／模拟验证）。真实库、真实账号、生产业务验收转上位合同第 10 章上线准入跟踪，不阻塞阶段「本地检查通过／已完成」。不得把 S1—S3 核销或任一子批次单独视为 S4 退出。

同步基线：S3 以 `docs/organization-data-scope-s3.md` 第 3、6、10 章为准（阶段退出已核销）。S4 未开始前的代码缺口以本文件第 2—4 章锁定的目标为准，不得用创建人、最近修改人、系统账号或供应商外部联系人冒充内部负责人。

增量接口：[S4 查询与交接增量 OpenAPI](organization-data-scope-s4-openapi.yaml)。

## 1. 交付边界

1. S4 总范围是 M13—M18。不重做 S1—S3 已接入资源；不开放薪酬、考勤、职级或通用策略引擎；不把部门 ID 写入结算主体、仓库或 WorkItem 既有责任组织字段。
2. M13—M16 **必须先**补责任创建、显式交接与审计，**再**开放人员筛选与 DataScope 列表隔离。禁止先做筛选、后补责任。
3. 查询只收窄授权结果，不授予动作权，不改派任务。新增部门管理可见权不得自动授予 `work_item:manage`。
4. 库存余额不建立虚构所有人。品牌、单位、分类、结算主体等公共字典不增加业务负责人。
5. M15 内部跟进负责人在 `supplier_fulfillment_order`；异常处理人是该订单当前开放 W26 任务的 `owner_user_id`。M17 是 W29 `integration_error_task`／`reconciliation_difference` 自身的当前／历史处理人。派发失败写入的无主错误任务不得顶替 M15 跟进人。
6. M16 结算单是整单责任模型。确认后应付／成本继续走 M09 已落地的部分授权，不得因「看得见结算单」泄露应付其他分配。复核 WorkItem `owner_organization_id` 取结算单 `business_org_unit_id`，禁止公司根，不得当作部门 ID。
7. M18 仓库维 DataScope 与审批 `responsible_org_id = warehouse_id` 已由 S2 接入，本阶段只叠加人员查询，禁止改写仓库维度语义。
8. 正式业务准入继续执行上位合同第 10 章。本阶段不得登记已验收。

## 2. 锁定约定

各 Subagent 必须使用下列标识，禁止另起同义字段或查询参数。

### 2.1 责任事实

| 功能组 | 对象 | 权威字段 | 创建规则 | 交接 |
| --- | --- | --- | --- | --- |
| M13 | `SupplierAccount` | `maintainer_user_id`（整体维护人）、`business_org_unit_id` | 新建必填；缺主属组织阻断；禁止 `created_by` 兜底 | `POST /admin/suppliers/{id}/handover` |
| M13 | `SupplierCapability` | 既有 `owner_user_id`（供给能力负责人） | 创建必须显式指定合格内部人员，禁止静默写成操作人 | `POST /admin/suppliers/{id}/capabilities/{capability_id}/handover` |
| M14 | `Product` | `maintainer_user_id`、`business_org_unit_id` | 同上 | `POST /admin/products/{id}/handover` |
| M14 | `SupplierOffering` | `maintainer_user_id`、`business_org_unit_id` | 同上 | `POST /admin/supplier-offerings/{id}/handover` |
| M14 | 商品／供给列表的采购负责人 | **不落主档**；由 `ProcurementResponsibilityRuleSet::resolve` 按 SKU／分类／类型／默认调度人解析 | 缺规则 fail-closed；列表筛选在授权集合内批量解析，超限整体拒绝 | 不因本查询改写采购单或任务 |
| M15 | `SupplierFulfillmentOrder` | `follow_up_user_id`（内部跟进）、`business_org_unit_id` | 创建取固定供应关系／履约责任规则解析的内部人员；禁止系统账号、派发 actor、供应商联系人 | `POST /admin/supplier-fulfillment-orders/{id}/handover` |
| M15 | 异常处理人 | 当前开放 W26 `WorkItem.owner_user_id` | 替换 `actor_id`／`owner_organization_id = "company"` 硬编码；须解析唯一合格人员 | 是否随单据交接转交须在命令中显式声明，默认不自动改派审批任务 |
| M16 | `SupplierSettlementStatement` | `prepared_by`＝对账负责人（语义升级，保持字段名）、`business_org_unit_id`；`difference_handler_user_id`＝差异处理人（新建，缺省等于对账负责人但可独立改派）；`reviewed_by`＝实际复核人 | 创建写入对账负责人及其主属组织；经办≠复核保持 | `POST /admin/supplier-settlement-statements/{id}/handover`（对账负责人）；差异处理人改派为独立命令，不得把 `resolved_by` 强制等于经办人 |
| M17 | `IntegrationErrorTask` | 既有 `owner_user_id`＝当前处理人；历史处理人取已办 `completed_by`／处理记录，不删除留痕 | 创建必须有合格内部处理人，禁止 `None` 后靠列表筛选假装有主 | 复用既有改派入口；补稳定 ID 列表筛选 |
| M17 | `ReconciliationDifference` | 当前处理人必须落到实体（`owner_user_id`）及列表投影，不得只存在于 WorkItem | 创建写入处理人 | 改派同步任务与实体 |
| M18 | `StockMovement` | `recorded_by`＝实际经办人 | 已有 | 无交接；仅查询 |
| M18 | `StockAdjustment` | `prepared_by`＝实际经办；申请人＝审批快照 `submitted_by`（列表必须投影，不得用 `created_by` 顶替）；当前审批人＝开放审批实例 `current_assignee` | 已有 | 审批禁止运行时改派 |

组织字段一律使用内部组织 ID，禁止写入 `settlement_party_id` 或仓库 ID。`include_descendants` 缺省 `false`。

### 2.2 查询参数

未注册字段必须 400，不得静默忽略。人员／组织各最多 100 个 ID。同字段 OR、不同字段 AND。第二页起必填 `scope_version`；冲突返回 `409 DATA_SCOPE_CHANGED`。

| 参数 | 语义（S4 用法） |
| --- | --- |
| `owner_user_ids` | 本资源当前业务负责人：供应商维护人、商品／供给维护人、API 订单跟进人、结算对账负责人 |
| `capability_owner_user_ids` | 仅供应商列表：供给能力负责人（与整体维护人分列） |
| `procurement_owner_user_ids` | 仅商品／供给列表：规则解析出的采购负责人 |
| `handler_user_ids` | 当前开放任务处理人：API 订单异常处理人、结算当前复核人、集成当前处理人、库存调整当前审批人 |
| `operator_user_ids` | 已发生动作的实际经办人：结算差异处理人、集成历史处理人、库存流水／调整经办人 |
| `applicant_user_ids` | 仅库存调整：申请人（`submitted_by`） |
| `org_unit_ids` / `include_descendants` | 当前业务组织；库存资源不注册该内部组织参数，继续用仓库筛选 |
| `scope_version` | 跨页／导出绑定 |

废弃姓名筛选参数必须 400。不以 `"me"` 作为人员 ID。

### 2.3 Port／adapter／资源动作

消费域定义窄 Port 与 `FailClosed*`；生产 adapter 在 `erp-processes`，**同一语句**调用 `DataScopeService::new(...).resolve(...)` 或 `resolve_permissions(...)`，透传调用方 `executor`，禁止 `NoTransaction`、`unwrap_or*`、补 Company。业务域不得依赖 `erp-identity`。

| 资源 | 本阶段准入动作 | 维度 | 历史参与 | Port | Adapter 文件 |
| --- | --- | --- | --- | --- | --- |
| `supplier` | `list/detail/create/update/delete` | InternalOrg 必需 | 不允许 | `erp-supplier::ports::SupplierDataScopePort` | `erp-processes/src/adapters/supplier_data_scope.rs` |
| `product` | `list/detail/create/update` | InternalOrg 必需 | 不允许 | `erp-catalog::ports::CatalogDataScopePort` | `erp-processes/src/adapters/catalog_data_scope.rs` |
| `supplier_offering` | `list/create/update` | InternalOrg 必需 | 不允许 | `erp-supply::ports::OfferingDataScopePort` | `erp-processes/src/adapters/offering_data_scope.rs` |
| `supplier_fulfillment_order` | `list/detail` 及既有写动作 | InternalOrg 必需 | 读允许参与则按登记，写不允许 | `erp-supply::ports::FulfillmentOrderDataScopePort` | `erp-processes/src/adapters/fulfillment_order_data_scope.rs` |
| `supplier_settlement_statement` | 在既有 `confirm` 上扩展 `list/detail/create/update/submit` | InternalOrg 必需 | 不允许 | `erp-supply::ports::SettlementDataScopePort` | `erp-processes/src/adapters/settlement_data_scope.rs` |
| `integration_error_task` | `list/detail` 及既有处理动作 | InternalOrg 必需 | 历史处理人只作筛选，不授写 | `erp-integration::ports::IntegrationDataScopePort` | `erp-processes/src/adapters/integration_data_scope.rs` |
| `reconciliation_difference` | `list/detail` 及既有处理动作 | InternalOrg 必需 | 同上 | 同上 Port，按资源分支 | 同上 adapter |
| `stock_adjustment` / `stock_movement` / `stock_balance` / `stock_reservation` | **沿用 S2 已接线动作与仓库维** | Warehouse 必需 | 不允许 | 扩展既有 `erp-inventory::ports::authorization::AuthorizationPort` | 扩展 `erp-processes/src/adapters/inventory.rs` |

`WIRED_CONSUMERS` 与 `RESOURCE_ACTIONS` 必须在该资源列表／详情／命令真正按动作解析后才追加。禁止只种子不接线。库存资源已在 S2 登记，S4 不得重复登记或改维度。结算 `confirm` 已接线，扩展动作时更新同一行，不得另起并行资源名。

对象映射口径：

- 供应商：维护人及其 `business_org_unit_id`；能力负责人筛选只收窄，不单独扩大可见供应商。
- 商品／供给：维护人＋业务组织；采购负责人筛选是额外 AND，不改变维护人授权。
- API 订单：跟进人＋业务组织；`handler_user_ids` 再与当前开放 W26 任务求交。
- 结算：对账负责人＋业务组织；差异／复核筛选只收窄。
- 集成：当前处理人所属内部组织；无正向范围不得回退公司。
- 库存：仓库范围与人员条件分别校验后求交；余额不按人员授权。

### 2.4 交接命令形态

对齐销售单 `POST /admin/sales-orders/{id}/handover`：

- 请求：目标人员 ID、可选目标组织（缺省保留原组织，不得随接收人部门隐式变化）、原因、期望版本、幂等键。
- 目标必须账号有效、具备该资源维护／跟进资格、通过岗位分离。
- 同事务更新责任字段、审计、推进版本；开放审批任务不改派。
- `GET …/handover-candidates` 只返回合格有效人员。
- 成功 mutation 必须标注 `affectsDataScope`。

### 2.5 前端

复用 `ResponsibleUserFilter`、`ListWorkspaceFilterBar`、`features/data-scope/cache.ts`。有效条件进入 URL 与 TanStack Query key。三分空态：无范围／筛空／请求失败。跨页带 `scope_version`。导出与列表同一授权条件；主数据若仍走前端拼 CSV，必须先按当前授权拉全量并在撤权后丢弃，不得输出旧宽范围。禁止新视觉体系。界面文案走 `lib/ui-text.ts`；自动化 id 走 `lib/automation-id.ts`。

## 3. 阶段项

| 编号 | 范围 | 必须完成 | 文件归属（独占） |
| --- | --- | --- | --- |
| S4-00 | 本合同、OpenAPI 骨架、主合同状态 | 锁定字段／批次／波次 | `docs/organization-data-scope-s4.md`、`docs/organization-data-scope-s4-openapi.yaml`、主合同状态行 |
| S4-01 | M13 供应商 | 维护人＋组织、能力负责人显式指定与交接、Port／adapter、列表／详情／导出／命令、双人员筛选 | `erp-supplier/**`；`erp-processes/src/supplier_profile/**`；`adapters/supplier_data_scope.rs`；`web-api` supplier handler/routes；`erp-client/features/master-data` 供应商列表／详情／筛选／导出相关文件 |
| S4-02 | M14 商品 | 商品维护人＋组织、交接、Catalog Port／adapter、列表／详情／导出、维护人筛选、采购负责人独立筛选（批量 resolve） | `erp-catalog/**`；`adapters/catalog_data_scope.rs`；`erp-read-models/src/catalog_center/**`；catalog HTTP；`erp-client/features/master-data` 商品列表／详情／筛选相关文件。只读复用采购责任规则，不改规则模型 |
| S4-03 | M14 供给 | 供给维护人＋组织、交接、Offering Port／adapter、供给列表筛选（含采购负责人） | `erp-supply` 下 `entity/dto/service/repository/indexes` 的 `supplier_offering*`；`adapters/offering_data_scope.rs`；offering HTTP；`erp-client/features/supplier-offerings/**` |
| S4-04 | M15 API 订单 | 跟进人＋组织、交接、W26 异常处理人合规解析、Fulfillment Port／adapter、列表／详情／导出、`owner`/`handler` 筛选；取消／退款筛选漏传一并修 | `erp-supply` 下 `supplier_fulfillment*`；`erp-processes/src/supply_execution/**`；`adapters/fulfillment_order_data_scope.rs`；fulfillment HTTP；`erp-client/features/supplier-orders/**` |
| S4-05 | M17 集成异常与差异 | 差异实体当前处理人、稳定 ID 列表、历史处理人筛选、Integration Port／adapter、去掉 `"me"` 与客户端假分页、W29 `owner_organization_id` 不再写死 company | `erp-integration/**`；`erp-processes/src/integration_resolution/**`；`adapters/integration_data_scope.rs`；integration_ops HTTP；`erp-client/features/integration-errors/**` |
| S4-06 | M18 库存经办 | 流水／调整人员筛选与列表投影、申请人／当前审批人、导出重验、`scope_summary`；不改仓库维、不给余额加所有人 | `erp-inventory/**`；`erp-processes/src/adapters/inventory.rs`；`inventory_adjustment/**`；inventory HTTP；`erp-client/features/inventory/**` |
| S4-07 | M16 供应商结算 | 业务组织、差异处理人独立、交接、扩展 list/detail/create/update/submit DataScope、三人员筛选、去掉 `hasDataScope` 占位与组织根 `company` | `erp-supply` 下 `supplier_settlement*`；`erp-processes/src/supply_settlement/**`；`adapters/settlement_data_scope.rs`；settlement HTTP；`erp-client/features/supplier-settlements/**` |
| S4-08 | 架构闭合 | 已核对 `WIRED_CONSUMERS`／`RESOURCE_ACTIONS`／`adapters/mod.rs`／AppState 与实现一致（库存仓库维未改）；已填第 6 章九列与 A33—A36 本地证据；主合同第 3／11／12 章已同步。结算复核任务身份已改写结算单内部组织，阶段退出见第 8 章 | 共享接线文件与本合同第 6、8 章 |

波次：

1. **Wave 0**：S4-00（本文件）。
2. **Wave 1 可并行**：S4-01、S4-02、S4-05、S4-06。
3. **Wave 2**（`erp-supply` 热区，默认串行；若并行必须 worktree 且不得改对方子树）：S4-03 → S4-04 → S4-07。
4. **Wave 3**：S4-08。未完成 Wave 1／2 的适用入口不得登记本地检查通过。

共享文件协议：Wave 1／2 实现者**尽量只新增** adapter 文件与领域文件。若必须改 `consumers.rs`、`predefined_data_scopes.rs`、`erp-processes/src/adapters/mod.rs`、web-api `AppState`，只在文件末尾追加自己的资源块，并在 PR 说明中标明。S4-08 负责去重与调用链审查。禁止修改 S1—S3 已核销九列的历史结论。

## 4. 各功能组验收要点

### 4.1 M13

- 供应商列表／详情展示整体维护人；能力列表／详情展示能力负责人。
- 两套筛选独立，不得互相顶替。
- 能力创建请求必须能指定 `owner_user_id`；资料保存不得再把操作人写成能力负责人。
- 无维护人的存量开发数据：命令在首次维护／交接前阻断正式变更，或提供显式补录命令；禁止用 `created_by` 回填后开放筛选。

### 4.2 M14

- 商品与供给维护人分对象存储；SKU／品牌／分类不加维护人。
- `procurement_owner_user_ids` 使用规则解析，不物化到商品字段。
- 供给成本字段继续按 `supplier_offering_cost:detail` 脱敏，范围接入不得扩大成本可见性。

### 4.3 M15

- 列表去掉纯客户端 `actionable` 假分页，改为服务端条件。
- 跟进人与异常处理人分列展示。
- 与 M17 共享组件不等于 M15 验收完成。

### 4.4 M16

- 三角色独立筛选。
- `confirm` 继续走既有工作流资格；list/detail 不得只靠 RBAC 看全库。
- 整单金额对无整单资格账号按上位合同第 7 章限制；本资源无份额行时按整单可见或不可见处理，不得编造份额。

### 4.5 M17

- `owner_user_id` 查询改为稳定 ID 列表，禁止正则模糊和 `"me"`。
- 差异列表必须投影当前处理人。
- 历史处理人筛选命中处理记录／已办，不删除参与留痕。

### 4.6 M18

- 余额列表不注册人员负责人参数。
- 调整列表投影经办、申请人、当前审批人。
- 人员条件与仓库授权求交；缺仓库维角色范围不贡献对象。

## 5. 实现步骤（每组固定）

1. 责任事实与交接／审计（M13—M16；M17 补差异当前处理人；M18 不新建责任人）。
2. 本域 Port、事实类型、`FailClosed*`、对象 `allows`。
3. `erp-processes` adapter：同语句公共解析、维度拒绝、executor 透传。
4. Repository `*ReadScope` 条件编译；A34 内存等价测试放 adapter 或领域测试模块。
5. 列表／详情／候选／导出／写命令各按自身动作解析；写命令原事务重验。
6. 前端筛选、URL／QueryKey、空态、`affectsDataScope`。
7. 索引评估：负责人、业务组织、稳定排序；不机械建全排列。组合根只调领域 `indexes::ensure`。
8. 本批旧读取器清零：禁止再解释 `scope_type`／`scope_targets`。
9. 在交付记录补九列与门禁第 4 节登记（通过／失败／未执行／不适用）。

质量门禁（后端在 `backend/`，只跑库单元）：

```bash
cargo fmt --all -- --check
cargo check --workspace --locked
cargo clippy --workspace --all-targets --all-features --locked -- -D warnings
env -u ERP_TEST_MONGO_URI cargo test --workspace --lib --locked
./scripts/check-bpm-boundaries.sh
./scripts/check-domain-boundaries.sh --cutover
./scripts/check-org-data-scope.sh
```

前端在 `erp-client/`：`npm run lint`、相关 vitest。禁止新增或执行 `tests/` 集成测试、真实 Mongo／S3。生产方法 ≤50 有效行，生产文件 ≤800 行。Handler 权限宏与生成物同步；禁止手改 `permissions.generated.ts`。

## 6. 九列登记（S4-08，基线 `a16ce443`）

状态列只允许：尚未接入／部分接入／已接入但不符合合同／本地检查通过。真实验收另列跟踪，不得登记已验收。

接线核对（S4-08，未改维度、未重做功能）：

1. `erp-identity` `WIRED_CONSUMERS` 与 `RESOURCE_ACTIONS` 对 M13—M18 资源动作集合一致：`supplier` list/detail/create/update/delete；`product` list/detail/create/update；`supplier_offering` list/create/update（无 detail）；`supplier_fulfillment_order` list/detail/investigate/complete/submit/cancel/refund/reject/handover；`supplier_settlement_statement` list/detail/create/update/submit/confirm；`integration_error_task` list/detail/create；`reconciliation_difference` list/detail/create/decide。库存四资源动作与维度仍为 Warehouse，未重复登记、未改维。
2. `allows_history` 仅客户／合同／销售／采购的 list/detail；S4 资源全部 `false`。履约读动作按本表登记为不允许历史参与。
3. 生产 adapter 均在 `erp-processes/src/adapters/`，`mod.rs` 已导出；AppState／Handler 装配见下表 HTTP 列。库存继续 `DataScopeService::resolve_permissions`。
4. HTTP 另有未纳入本阶段 DataScope 准入的附属资源（如 `supplier_sensitive:reveal`、`sellable_sku`、`supplier_offering_availability`、`supplier_offering:resolve_supply_exception`、`supplier_refund_fact:post`、`supplier_settlement_statement:void` 走 `update` 重验、`inbox_message`、`integration_task:process/complete`、`stock_adjustment:post`）。不得把这些动作写成已接入 v2 消费者。

| 资源动作 | 拥有领域 | 公共解析入口 | Port／adapter | 适用与必需维度 | 历史参与允许动作 | 对象事实来源 | 条件编译入口 | HTTP／CLI 入口 | 状态 |
| --- | --- | --- | --- | --- | --- | --- | --- | --- | --- |
| `supplier:list/detail/create/update/delete` | erp-supplier | `DataScopeService::resolve`（`MongoSupplierDataScope::resolve` 同语句） | `erp-supplier::ports::SupplierDataScopePort`／`FailClosedSupplierDataScopePort`／`erp-processes::adapters::supplier_data_scope::MongoSupplierDataScope`；对象判定 `SupplierAccess::allows` → adapter `evaluate_object` → `ResolvedScope::allows` | 适用 InternalOrg；必需 InternalOrg（仓库／结算主体出现即拒绝） | 不允许 | 整体维护人 `maintainer_user_id`＋`business_org_unit_id`；能力负责人 `owner_user_id` 只作 `capability_owner_user_ids` 收窄，不扩大可见供应商 | `erp-supplier::repository::scope::SupplierReadScope::document`；`supplier_scope`；等价 `supplier_data_scope::equivalence_tests::public_object_decision_matches_compiled_conditions` | `GET /admin/suppliers`、`GET /admin/suppliers/{id}`（`SupplierAccess::require` detail）、资料 create/update、`DELETE /admin/suppliers/{id}`、`POST /admin/suppliers/{id}/handover`、`GET …/handover-candidates`、`POST …/capabilities/{id}/handover`；装配 `AppState::supplier_service` → `scoped_supplier_service_with_sensitive` | 本地检查通过（S4-01；真实验收转跟踪） |
| `product:list/detail/create/update` | erp-catalog | 同上（`MongoCatalogDataScope`） | `erp-catalog::ports::CatalogDataScopePort`／`FailClosedCatalogDataScopePort`／`MongoCatalogDataScope`；`CatalogAccess::allows` | 适用 InternalOrg；必需 InternalOrg | 不允许 | 商品 `maintainer_user_id`＋`business_org_unit_id`；采购负责人由规则 `ProcurementResponsibilityRuleSet::resolve` 额外 AND，不落主档 | `erp-catalog::repository::catalog::scope::CatalogReadScope::document`；`catalog_scope`；等价 `catalog_data_scope::tests::a34_public_allows_matches_catalog_read_scope_document` | `GET /admin/products`、`GET /admin/products/{id}`（`CatalogCenterReadService::product_detail` → `require_product(detail)`）、create/update、`POST /admin/products/{id}/handover`、`GET …/handover-candidates`；附属 `GET …/revisions`、`…/skus`、`…/sku-revisions` 经 `CatalogService::product_detail_*` → `require_product(detail)`；装配 `catalog_center`＋`scoped_catalog_service` | 本地检查通过（S4-02／A36；附属子资源已按 detail 重验；真实验收转跟踪） |
| `supplier_offering:list/create/update` | erp-supply | 同上（`MongoOfferingDataScope`） | `erp-supply::ports::OfferingDataScopePort`／`FailClosedOfferingDataScopePort`／`MongoOfferingDataScope`；`OfferingAccess::allows` | 适用 InternalOrg；必需 InternalOrg | 不允许 | 供给 `maintainer_user_id`＋`business_org_unit_id`；采购负责人筛选同商品，只收窄 | `erp-supply::repository::supplier_offering::scope::OfferingReadScope::document`；`offering_scope`；等价 `offering_data_scope::tests::a34_public_allows_matches_offering_read_scope_document` | `GET/POST /admin/supplier-offerings`、`POST …/{id}/revisions`（update）、`POST …/handover`、`GET …/handover-candidates`；列表 `SupplierOfferingReadService`＋`offering_access`；命令 `scoped_offering_process`。无独立 `detail` 动作（`WIRED` 拒绝） | 本地检查通过（S4-03；真实验收转跟踪） |
| `supplier_fulfillment_order:list/detail/investigate/complete/submit/cancel/refund/reject/handover` | erp-supply | 同上（`MongoFulfillmentOrderDataScope`） | `erp-supply::ports::FulfillmentOrderDataScopePort`／`FailClosedFulfillmentOrderDataScopePort`／`MongoFulfillmentOrderDataScope`；开放 W26 处理人 `FulfillmentExceptionHandlerPort`／`MongoFulfillmentExceptionHandlers`；`FulfillmentOrderAccess::allows` | 适用 InternalOrg；必需 InternalOrg | 不允许（`consumers::allows_history=false`；读动作亦不补充历史参与） | 跟进人 `follow_up_user_id`＋`business_org_unit_id`；`handler_user_ids` 与当前开放 W26 `WorkItem.owner_user_id` 求交。W26 `owner_organization_id` 取订单业务组织，禁止 `"company"`；**不把该字段改成部门 ID 语义之外的新模型** | `erp-supply::repository::supplier_fulfillment_scope::FulfillmentOrderReadScope::document`；`fulfillment_order_scope`；等价 `fulfillment_order_data_scope::equivalence_tests::public_object_decision_matches_compiled_conditions` | `GET /admin/supplier-fulfillment-orders`、`GET …/{id}`（`require_scoped_order(detail)`）、investigate/complete/submit/cancel/refund/reject、`POST …/handover`、`GET …/handover-candidates`；装配 `scoped_fulfillment_service`＋`supplier_fulfillment_process` 注入 Port | 本地检查通过（S4-04；真实验收转跟踪） |
| `supplier_settlement_statement:list/detail/create/update/submit/confirm` | erp-supply | `DataScopeService::resolve`；confirm 另走既有工作流资格，对象仍按本资源动作解析 | `erp-supply::ports::SettlementDataScopePort`／`FailClosedSettlementDataScopePort`／`MongoSettlementDataScope`；`SettlementAccess::allows`（对账负责人 `prepared_by`） | 适用 InternalOrg；必需 InternalOrg | 不允许 | 对账负责人 `prepared_by`＋`business_org_unit_id`（禁止结算主体／`"company"` 根）；差异处理人 `difference_handler_user_id`、复核人 `reviewed_by`／开放复核任务只收窄。整单责任，不因可见结算单泄露 M09 应付其他分配 | `erp-supply::repository::supplier_settlement::scope::SettlementReadScope::document`；`settlement_scope`；等价 `settlement_data_scope::equivalence_tests::public_object_decision_matches_compiled_conditions` | `GET/POST /admin/supplier-settlement-statements`、`GET …/{id}`（`require_detail`）、create/update/submit/confirm、`POST …/handover`、差异改派；装配 `scoped_settlement_process`。void 走 `update` 重验，未单独登记 void 消费者 | 本地检查通过（S4-07；复核 WorkItem `owner_organization_id` 取结算单 `business_org_unit_id`，禁止 `"company"`；真实验收转跟踪） |
| `integration_error_task:list/detail/create` | erp-integration | 同上（`MongoIntegrationDataScope::resolve(resource, action)`） | `erp-integration::ports::IntegrationDataScopePort`／`FailClosedIntegrationDataScopePort`／`MongoIntegrationDataScope`；`IntegrationAccess::allows_handler`／`require_handler` | 适用 InternalOrg；必需 InternalOrg | 不允许（历史处理人只作 `operator_user_ids` 筛选，不授写、不进 `allows_history`） | 当前处理人 `owner_user_id` 及其内部组织 `owner_org_unit_id`；禁止 `"me"`、禁止组织根 `"company"` 回退。W29 任务身份取实体组织，改派同步实体与任务 | `erp-integration::repository::scope::IntegrationReadScope::document`；`integration_scope`；等价 `integration_data_scope::equivalence_tests::public_object_decision_matches_compiled_conditions` | `GET/POST /admin/integration/error-tasks`；`GET …/{id}`（`Extension<AuditActor>`，`IntegrationCenterReadService::error_task_detail` → `require_handler(detail)`，越权 NotFound）；列表／创建经 `scoped_integration_ops_service`；详情装配 `AppState::integration_center` → `MongoIntegrationDataScope::shared` | 本地检查通过（S4-05／A36；真实验收转跟踪） |
| `reconciliation_difference:list/detail/create/decide` | erp-integration | 同上 | 同上 Port，按资源分支 | 适用 InternalOrg；必需 InternalOrg | 不允许（历史处理人仅筛选） | 差异实体当前 `owner_user_id`＋内部组织；改派同步任务与实体 | 同上 `IntegrationReadScope::document` | `GET/POST /admin/integration/differences`、`GET …/{id}`（actor + `require_handler(detail)`）、`POST …/{id}/decisions`；列表／创建经 `scoped_integration_ops_service`；详情同 `integration_center` | 本地检查通过（S4-05／A36；真实验收转跟踪） |
| `stock_adjustment:list/detail/create/update/submit`；`stock_balance:list/detail`；`stock_movement:list`；`stock_reservation:list` | erp-inventory | 既有 `DataScopeService::resolve_permissions`（`MongoInventoryAuthorization::authorize` 按动作独立解析） | 既有 `erp-inventory::ports::authorization::AuthorizationPort`／`FailClosedAuthorizationPort`／`MongoInventoryAuthorization`；人员筛选 `AdjustmentPeopleFactsPort` | **Warehouse 必需（S2，本阶段未改维）**；内部组织参数不注册 | 不允许 | 仓库范围；流水／调整经办 `recorded_by`／`prepared_by`、申请人审批快照 `submitted_by`、当前审批人开放任务 `current_assignee` 为额外 AND。余额不按人员授权、不建虚构所有人 | 仓库：`WarehouseScope` → `repository_warehouse_ids`；人员：`StockMovementFilter.recorded_by_ids`、`StockAdjustmentFilter.prepared_by_ids`＋`id_in`（`intersect_object_ids`）。S2 仓库等价沿用；人员投影 `inventory::adjustment_query::people_projection_tests`、DTO `movement_and_adjustment_consume_registered_people_filters` | `GET /admin/stock-balances`、`…/{id}`、`GET /admin/stock-movements`、`GET /admin/stock-reservations`、`GET/POST/PUT /admin/stock-adjustments`；装配 `AppState::inventory_service` → `adapters::inventory_service` | 仓库维沿用 S2 本地检查通过；人员查询已接入（S4-06；真实验收转跟踪） |

### 6.1 A33—A36 本地证据

功能与架构分别登记。内存／隔离证据可计入阶段本地检查；真实库 `find` 对拍、真实账号、浏览器写入、代表性执行计划、撤权下载一律转第 10 章上线准入跟踪，不登记已验收。

| 验收项 | 调用链路径 | 等价／准入测试模块 | 当前登记 |
| --- | --- | --- | --- |
| A33 | 消费域 Port（上表）→ 组合层 adapter `DataScopeService::new(...).resolve(...)` 或库存 `resolve_permissions` 同语句调用 → 身份域 `consumers::registration`；业务域 Cargo 无 `erp-identity`。装配：`scoped_supplier_service_with_sensitive`、`scoped_catalog_service`／`catalog_center`、`offering_access`／`scoped_offering_process`、`scoped_fulfillment_service`＋`MongoFulfillmentExceptionHandlers`、`scoped_settlement_process`、`scoped_integration_ops_service`、`AppState::integration_center` → `MongoIntegrationDataScope`、`inventory_service` | 领域边界夹具拒绝业务域直连身份域（`check-domain-boundaries.sh --cutover` 自检 18 项、工作区 0 错误） | 调用链源码符合合同结构；工作区 ODS-DOMAIN 已通过 |
| A34 | 公共判定 `evaluate_object`／`ResolvedScope::allows` 为基准；仓储 `*ReadScope::document` 编译固定字段（维护人／跟进人／对账负责人／处理人＋业务组织）。库存仓库维沿 S2；人员条件不进入身份维度，与仓库过滤求交 | `erp-processes`：`supplier_data_scope::equivalence_tests::public_object_decision_matches_compiled_conditions`；`catalog_data_scope::tests::a34_public_allows_matches_catalog_read_scope_document`；`offering_data_scope::tests::a34_public_allows_matches_offering_read_scope_document`；`fulfillment_order_data_scope::equivalence_tests::public_object_decision_matches_compiled_conditions`；`settlement_data_scope::equivalence_tests::public_object_decision_matches_compiled_conditions`；`integration_data_scope::equivalence_tests::public_object_decision_matches_compiled_conditions`。库存人员：`erp_inventory::service::inventory::adjustment_query::people_projection_tests`、`dto::inventory::movement_and_adjustment_consume_registered_people_filters`、`adapters::inventory::tests::inventory_scope_keeps_dimension_and_user_limit_without_fallback`。不得登记为真实库执行等价 | 内存集合等价作为本地证据。真实库对拍转跟踪 |
| A35 | 初始化 `predefined_data_scopes::RESOURCE_ACTIONS` 经 `validate_binding` → `WIRED_CONSUMERS`；未接线动作失败关闭。FailClosed Port 未装配即拒绝。库存维度仍走 Warehouse，S4 未把 InternalOrg 写入库存登记 | `erp-identity::service::access_control::consumers`：`wired_s3_actions_admit_reads_unwired_actions_fail_closed`（含 supplier）、`wired_integration_handlers_reject_history_writes`、`wired_supplier_offering_admits_list_and_writes_without_history`；`predefined_data_scopes::every_manifest_entry_uses_version_two_and_explicit_resource_actions` 对全部清单条目 `validate_binding`。FailClosed 单测在各 Port 模块 | 清单与消费者一致；未接线动作拒绝。不得用初始化条目代替入口准入 |
| A36 | 列表按自身动作 `resolve`；供应商／商品主详情与附属修订／SKU／履约详情与命令／结算详情与命令／集成错误任务与差异详情 `require_*` 原执行器重验；跨页 `scope_version`；交接 mutation `affectsDataScope`。导出无独立后端入口的主数据走前端按当前授权拉全量（撤权后须丢弃，真实验收转跟踪）。供给无独立 detail 动作 | 各域 `access`／handover 模块与 adapter 维度拒绝测试；库存 `ensure_scope_version`。A36 增量：`erp_integration::service::access::tests`（`fail_closed_require_handler_rejects_unwired`、`require_handler_hides_out_of_scope_as_not_found`、`require_handler_allows_current_handler_and_rejects_public_false`、`require_handler_detail_does_not_grant_historical_participation`）；`erp_catalog::service::catalog::access::tests::detail_mapping_hides_out_of_scope_maintainer` | 适用入口本地检查通过。真实验收转跟踪，不得登记已验收 |

### 6.2 门禁第 4 节（通过／失败／未执行／不适用）

| 条款 | 本阶段结论 | 说明 |
| --- | --- | --- |
| 9.1.2—3 同快照列表／总数 | 未执行（真实库） | 单元覆盖筛选 AND 与空范围空集；同快照对拍转跟踪 |
| 9.1.4、9.3.5、A36 各入口按动作解析 | 通过（本地） | 集成详情与商品附属详情按 `detail` 重验，越权 NotFound；真实验收转跟踪 |
| 9.1.5—6 导出重验 | 未执行 | 主数据无独立导出 HTTP；前端 CSV／撤权下载转跟踪 |
| 9.1.7 缓存键 | 未执行（浏览器） | 复用 `features/data-scope/cache.ts`；真实账号转跟踪 |
| 9.1.8 写命令原事务重验 | 通过（本地） | 已接线写／交接走 `require_*`；集成处理动作走 `integration_task` 非本表资源 |
| 9.1.9—10 展开／索引计划 | 未执行 | 代表性 `explain` 转跟踪 |
| 9.1.11 并发交接 | 未执行 | 单元覆盖 CAS／幂等；真实并发转跟踪 |
| 9.2—9.3、A34 条件等价 | 通过（内存）／未执行（真实库） | 见 §6.1 |
| 9.3 错误与空集 | 通过（本地） | adapter 拒不支持维度、FailClosed、空范围空集 |
| 9.4 客户样例 | 不适用 | S2 客户域，本阶段不重做 |
| 9.5、A35 登记与初始化一致 | 通过（本地） | 清单与实现已对齐；供给无独立 detail 按不适用 |
| ODS-* 静态门禁 | 通过 | §6.3；行为条款仍按上表分列 |

### 6.3 本批命令与退出码

在 `backend/` 执行；只跑库单元，未跑 `--test`、未连真实 Mongo。

| 命令 | 退出码 | 结果 |
| --- | --- | --- |
| `cargo fmt --all -- --check` | 0 | 格式通过 |
| `./scripts/check-domain-boundaries.sh --cutover` | 0 | 夹具 18 项通过；工作区 0 错误。结算列表改为 `WorkItemRepository::list_open_by_type_owners`，adapter 测试不再使用 `doc!` |
| `./scripts/check-org-data-scope.sh` | 0 | `STATIC_CHECKS_PASSED`，阻断 0。行为条款仍须按 §6.2 分列 |
| `./scripts/check-permissions-drift.sh` | 0 | 重建 `web-api` 后 `erp-client/lib/permissions.generated.ts` 无漂移 |
| `env -u ERP_TEST_MONGO_URI cargo test -p erp-supplier -p erp-catalog -p erp-supply -p erp-integration -p erp-inventory --lib --locked` | 0 | catalog 168、integration 129、inventory 103、supplier 161（4 ignored）、supply 306；合计 867 passed、0 failed、4 ignored（未跑 `--test`／真实 Mongo） |
| `env -u ERP_TEST_MONGO_URI cargo test -p erp-processes --lib --locked data_scope` | 0 | 53 passed（含 S4 adapter 等价与维度拒绝，以及既有选品／资金／客户等 `*data_scope`） |
| `env -u ERP_TEST_MONGO_URI cargo test -p erp-processes --lib --locked adapters::inventory` | 0 | 2 passed：`inventory_scope_keeps_dimension_and_user_limit_without_fallback`、`applicant_filter_uses_latest_snapshot_not_created_by` |

A36 详情重验增量（工作区，基线 `a45e00e1` 之上未提交）：

| 命令 | 退出码 | 结果 |
| --- | --- | --- |
| `cargo fmt --all -- --check` | 0 | 格式通过 |
| `./scripts/check-domain-boundaries.sh --cutover` | 0 | 夹具 18 项；工作区 0 错误 |
| `./scripts/check-org-data-scope.sh` | 0 | `STATIC_CHECKS_PASSED`，扫描 2093 个活动源码文件，阻断 0 |
| `env -u ERP_TEST_MONGO_URI cargo test -p erp-catalog --lib --locked` | 0 | 169 passed（含 `detail_mapping_hides_out_of_scope_maintainer`） |
| `env -u ERP_TEST_MONGO_URI cargo test -p erp-integration --lib --locked` | 0 | 133 passed（含 `require_handler_*` 4 项） |
| `env -u ERP_TEST_MONGO_URI cargo test -p erp-read-models --lib --locked -- --list` | 0 | 读模型库编译通过 |
| `env -u ERP_TEST_MONGO_URI cargo test -p web-api --lib --locked -- --list` | 0 | HTTP 库编译通过 |
| `cargo clippy -p erp-integration --lib --locked -- -D warnings` | 0 | 通过 |
| `cargo clippy -p erp-catalog --lib --locked -- -D warnings` | 101 | 既有 `write_disabled_product` `too_many_arguments`（8/7），非本批引入，未改该文件 |

未跑 `--test`、未连真实 Mongo。未跑 workspace 全量 `cargo test`／`clippy --workspace`。

结算复核身份闭合与阶段退出重验（工作区，基线 `16d48158` 之上）：

| 命令 | 退出码 | 结果 |
| --- | --- | --- |
| `cargo fmt --all -- --check` | 0 | 格式通过 |
| `cargo check --workspace --locked` | 0 | 工作区检查通过 |
| `env -u ERP_TEST_MONGO_URI cargo test --workspace --lib --locked` | 0 | 33 个库目标 4148 passed、0 failed、64 ignored（未跑 `--test`／真实 Mongo） |
| `./scripts/check-bpm-boundaries.sh` | 0 | BPM 边界通过 |
| `./scripts/check-domain-boundaries.sh --cutover` | 0 | 夹具 18 项；工作区 0 错误 |
| `./scripts/check-org-data-scope.sh` | 0 | `STATIC_CHECKS_PASSED`，扫描 2093 个活动源码文件，阻断 0 |
| `cargo clippy -p erp-supply -p erp-catalog -p erp-integration -p erp-read-models --lib --locked -- -D warnings` | 0 | 本批触及的生产库通过；catalog 既有 `write_disabled_product` 参数已收束 |
| `cargo clippy --workspace --all-targets --all-features --locked -- -D warnings` | 101 | `erp-processes` 既有 `supplier_profile::{create,handover}` `too_many_arguments`（9–10/7），非本批引入，未改该子树 |
| `erp-client` oxlint | 0 | 供给列表 `useEffect` 依赖已收束 |
| `erp-client` `npm run lint` | 1 | `lint:fixed-decimal` 与 `lint:feature-cycles` 为 S3 票款既有失败（`customer-receivables` ↔ `invoice-requests`），非本批引入 |
| S4 相关 vitest（settlements／offerings／orders／integration-errors／inventory／master-data） | 0 | 52 files / 238 tests passed |

本批新增单测：`review_owner_organization_rejects_company_and_empty`、`review_task_identity_binds_finance_role_and_statement_org`、`review_work_item_uses_statement_org_and_rejects_company`。

### 6.4 已知偏差与约束

1. **WorkItem `owner_organization_id` 不改为部门 ID 模型。** 合同第 1.5 条与 S4 §1.1：禁止把部门 ID 写入 WorkItem 既有责任组织字段的新语义。W26 用订单 `business_org_unit_id`，W29 用实体 `owner_org_unit_id`，结算复核用结算单 `business_org_unit_id`；三者均拒绝 `"company"`。`SETTLEMENT_REVIEW_OWNER_ROLE = "role-finance"` 仍是稳定语义标签，不是部门主键。
2. 供给无独立 `detail` 动作（`WIRED` 拒绝），不是缺口。
3. 真实验收、浏览器真实账号、代表性索引计划、导出撤权下载：上线准入跟踪。
4. 全仓 `clippy --workspace --all-targets` 与前端 `npm run lint` 的既有失败不阻塞本阶段本地检查通过；不得据此登记已验收。

## 7. 验证分层

1. 静态：权限、DTO、参数消费、错误语义、领域边界、ODS-*。
2. 单元：缺责任阻断、交接 CAS／幂等、空范围空集、个人上限、筛选 AND、能力负责人与维护人分列、采购规则筛选不改主档、库存人员与仓库求交、`"me"` 拒绝、结算三角色、A34 内存等价。
3. 浏览器样本（不阻塞完成，执行则登记）：筛选 URL、清除、跨页版本、390px。
4. 真实库／真实账号／索引计划：上线准入跟踪。

A33—A36 本地证据见第 6.1 节。真实验收转跟踪。

## 8. 退出条件

本地检查通过须同时满足：

- M13—M18 第 4 章要点已实现，18 组覆盖矩阵在主合同第 3 章可闭合到扩展组。
- 无虚构负责人、无跨角色拼接、无重复金额、无仓库／部门维度混用。
- 第 6 章九列无「尚未接入」的本阶段资源动作（库存人员查询登记为已接入人员条件）。
- `check-org-data-scope.sh` 与领域边界通过；本批活动路径不再自行解释原始范围。

**当前时点：本地检查通过。** M13—M18 与 A33—A36 本地证据已闭合；结算复核任务身份取结算单内部组织并拒绝 `"company"`。余下仅真实验收转第 10 章跟踪。不得登记已验收。不得把 WorkItem `owner_organization_id` 改成部门 ID。不得以本阶段完成开放正式业务。

第 6 章已无「尚未接入」的本阶段资源动作；库存人员查询已登记为已接入人员条件。
