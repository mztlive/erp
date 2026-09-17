# S4 扩展管理实施及验收合同

状态：执行中；未登记已验收，非正式上线

实施日期：2026-09-17

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
6. M16 结算单是整单责任模型。确认后应付／成本继续走 M09 已落地的部分授权，不得因「看得见结算单」泄露应付其他分配。不得把 `SETTLEMENT_REVIEW_OWNER_ORGANIZATION_ID = "company"` 继续当作组织事实。
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
| S4-08 | 架构闭合 | 合并 `WIRED_CONSUMERS`／`RESOURCE_ACTIONS`／`adapters/mod.rs`／AppState；A33—A36 本地证据；权限生成物；九列登记；18 组覆盖矩阵；主合同 S4 状态 | 共享接线文件与本合同第 6、8 章 |

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

## 6. 九列登记（实施中填写）

状态列只允许：尚未接入／部分接入／已接入但不符合合同／本地检查通过。真实验收另列跟踪。

| 资源动作 | 拥有领域 | 公共解析入口 | Port／adapter | 适用与必需维度 | 历史参与允许动作 | 对象事实来源 | 条件编译入口 | HTTP／CLI 入口 | 状态 |
| --- | --- | --- | --- | --- | --- | --- | --- | --- | --- |
| `supplier:list/detail/create/update/delete` | erp-supplier | `DataScopeService::resolve` | `SupplierDataScopePort`／`MongoSupplierDataScope` | InternalOrg 必需 | 不允许 | 维护人＋业务组织；能力负责人仅筛选 | 待填 | 供应商 HTTP | 尚未接入 |
| `product:list/detail/create/update` | erp-catalog | 同上 | `CatalogDataScopePort`／`MongoCatalogDataScope` | InternalOrg 必需 | 不允许 | 商品维护人＋业务组织 | 待填 | 商品 HTTP | 尚未接入 |
| `supplier_offering:list/create/update` | erp-supply | 同上 | `OfferingDataScopePort`／`MongoOfferingDataScope` | InternalOrg 必需 | 不允许 | 供给维护人＋业务组织 | 待填 | 供给 HTTP | 尚未接入 |
| `supplier_fulfillment_order:list/detail/…` | erp-supply | 同上 | `FulfillmentOrderDataScopePort`／`MongoFulfillmentOrderDataScope` | InternalOrg 必需 | 待登记读动作 | 跟进人＋业务组织；异常处理人为开放 W26 任务 | 待填 | 履约订单 HTTP | 尚未接入 |
| `supplier_settlement_statement:list/detail/create/update/submit`＋既有 `confirm` | erp-supply | 同上；confirm 复用工作流 | `SettlementDataScopePort`／`MongoSettlementDataScope` | InternalOrg 必需 | 不允许 | 对账负责人＋业务组织 | 待填 | 结算 HTTP | 部分接入（仅 confirm） |
| `integration_error_task`／`reconciliation_difference` 适用动作 | erp-integration | 同上 | `IntegrationDataScopePort`／`MongoIntegrationDataScope` | InternalOrg 必需 | 历史处理人仅筛选 | 当前处理人组织 | 待填 | 集成 HTTP | 尚未接入 |
| `stock_*` 既有动作 | erp-inventory | 既有 `resolve_permissions` | 既有 `AuthorizationPort`／`MongoInventoryAuthorization` | Warehouse 必需 | 不允许 | 仓库；人员条件额外 AND | 既有仓库条件＋人员字段 | 库存 HTTP | 仓库维本地检查通过；人员查询尚未接入 |

## 7. 验证分层

1. 静态：权限、DTO、参数消费、错误语义、领域边界、ODS-*。
2. 单元：缺责任阻断、交接 CAS／幂等、空范围空集、个人上限、筛选 AND、能力负责人与维护人分列、采购规则筛选不改主档、库存人员与仓库求交、`"me"` 拒绝、结算三角色、A34 内存等价。
3. 浏览器样本（不阻塞完成，执行则登记）：筛选 URL、清除、跨页版本、390px。
4. 真实库／真实账号／索引计划：上线准入跟踪。

A33—A36 本地证据在 S4-08 汇总；各功能组须提供本域 Port 调用链与等价测试入口路径。

## 8. 退出条件

本地检查通过须同时满足：

- M13—M18 第 4 章要点已实现，18 组覆盖矩阵在主合同第 3 章可闭合到扩展组。
- 无虚构负责人、无跨角色拼接、无重复金额、无仓库／部门维度混用。
- 第 6 章九列无「尚未接入」的本阶段资源动作（库存人员查询登记为已接入人员条件）。
- `check-org-data-scope.sh` 与领域边界通过；本批活动路径不再自行解释原始范围。

不得登记已验收。不得以本阶段完成开放正式业务。
