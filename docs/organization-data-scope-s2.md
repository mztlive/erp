# S2 组织与范围实施及验收合同

状态：执行中；未达到 S2 退出条件

实施日期：2026-09-13

修订日期：2026-09-15

上位合同：[组织架构、数据范围与负责人查询执行合同](organization-data-scope-contract.md) v1.2，第 4—6、8—12 章；公共解析接入必须执行第 9.2—9.5 节及 A33—A36。

## 1. 交付准入

1. S2 必须同时完成组织配置、统一范围解析、S1 四类资源接入、业务组织采集、首次销售生效归属、关联任务资格检查及前后端验证。
2. 当前代码包含组织与范围后端基础、组织接口、S1 四类资源 v2 接入、销售和采购业务组织采集及销售生效快照。任务消费者尚未完成统一接入，阶段验证仍有缺口，不得登记 S2 已完成或已验收。
3. 本批代码不得作为正式业务上线依据。禁止将单个领域编译或单元测试通过等同于部门数据隔离、任务资格或导出撤权保证已经交付。
4. 首次业务开放继续执行上位合同第 10 章；不回填旧业务数据，不提供旧范围版本兼容读取。
5. 功能接入与架构符合性必须分别登记。客户已接入的查询、版本与写入校验不得登记为尚未实施；其跨域直连整改已完成，公共单对象判定和条件等价性仍须分别验证。尚未迁移的工作流等消费者列为待接入项，不得归因为本批接入错误。

## 2. 已实施的后端规则

| 能力 | 实现入口 | 当前边界 |
| --- | --- | --- |
| 组织树与时段关系 | `erp-identity/src/entity/organization.rs` | 校验环、缺失父节点、明确下级展开、UTC 半开区间；内部组织与法人、仓库身份分开 |
| 组织变更 | `erp-identity/src/entity/organization_change.rs` | 创建、移动、更名、停用、成员调岗与结束、管理授权与撤销；调岗保留原关系 |
| 组织持久化 | `erp-identity/src/repository/organization.rs` | 组织、成员、管理关系、版本及变更回执由身份域拥有；全局组织版本防止并发拓扑和时段写偏差 |
| 预览、提交与审计 | `erp-identity/src/service/organization.rs` | 共用状态校验；提交独立重验权限、版本与未结业务；回执记录操作人、原因和前后值；不改派任务 |
| DataScope v2 模型 | `erp-identity/src/entity/access_control/{data_scope,scope_binding}.rs` | 必需版本、资源、动作与目标维度；动态模式只适用于内部组织；禁止目标通配符和显示名身份 |
| 范围集合解析 | `erp-identity/src/entity/access_control/resolved_scope.rs` | 同角色授权、逐维求交、角色并集与个人上限分别保留；合法历史参与仍受个人上限限制 |
| 应用解析入口 | `erp-identity/src/service/access_control/resolve.rs` | 复用现有账号、角色、RBAC policy 版本；缺动作拒绝、缺范围保持空集；当前尚未替换全部业务消费者 |
| 范围配置写入 | `erp-identity/src/service/access_control/mod.rs` | 同角色证明组织配置及范围配置动作；写入和审计在同一事务推进 policy 版本 |
| 显式初始化 | `erp-identity/src/service/iam/predefined_data_scopes.rs` | 按角色、资源、动作列出规则；保留已有配置及软删除留痕；取消缺范围补 Company 的初始化逻辑；清单与消费者准入仍须按 S2-13 同步，不得将列入清单视为已接入 |
| 组织与范围索引 | `erp-identity/src/indexes.rs` | 新增组织集合身份与有效期索引；范围按稳定 ID 唯一，取消旧主体与范围类型唯一约束 |
| 业务组织采集 | `erp-processes/src/business_ownership.rs` | 有效后台责任人必须具备有效主属组织；ERP 建单在写入事务重验预先采集的组织 |
| 销售首次生效归属 | `erp-sales/src/entity/sales_order/attribution.rs`；`erp-processes/src/order_to_cash/{formalize,formalization_posting}.rs` | 冻结负责销售、单据业务组织、名称及祖先路径；生效写入事务重验，失败回滚；普通编辑不修改快照 |

## 3. 组织接口

| 接口 | 权限 | 执行要求 |
| --- | --- | --- |
| `GET /admin/org-units` | `org_unit:list` | 返回当前组织配置边界内的节点与关系；禁止返回其他组织事实 |
| `POST /admin/org-units/preview` | `org_unit:manage` | 携带 `expected_version`、`idempotency_key`、`reason` 与固定类型 `change`；只预览，不写入 |
| `POST /admin/org-units/change` | `org_unit:manage` | 使用相同命令提交；在事务内重验当前授权、版本、成员及管理关系资格；异载荷复用幂等键拒绝 |

1. `change.operation` 固定为 `create_unit`、`move_unit`、`rename_unit`、`disable_unit`、`transfer_member`、`end_membership`、`grant_management`、`revoke_management`。
2. 创建根节点和管理无主属组织人员必须具备对应公司配置边界。移动节点须覆盖原子树与新父节点；成员调岗须覆盖原组织及目标组织。
3. 停用组织必须先结束成员和管理关系，处理有效下级，并清理需交接的未结销售、采购业务。历史闭合业务和历史归属不得被删除。
4. 组织配置权限不授予业务执行权，不修改开放审批、履约、验收或采购任务。
5. 组织管理及范围配置页面已接入，执行 S2-08；组织接口执行 [S2 组织配置 OpenAPI](organization-data-scope-s2-openapi.yaml)。浏览器验收仍须核销，不得只凭接口和页面代码登记验收完成。

## 4. 接入项及适用边界

| 编号 | 必须交付 | 当前状态 |
| --- | --- | --- |
| S2-01 | M01 客户：统一范围、主责及协作、组织筛选、列表与详情、候选及导出 | 功能已接入；跨域直连已改为本域 Port + 组合层 adapter。列表、详情、全量授权、导出、写命令与归属变更消费 v2；主责与协作分开；组织筛选按当前主负责人所属组织；跨页与 CSV 携带 scope_version。S2-12 的 A34／A36 及业务验收未核销，不得登记本项整体完成 |
| S2-02 | M02 合同：当前客户归属及合法单据参与、组织筛选、附件、候选与导出 | 功能已接入；Port／adapter 已按第 9.4 节接线。列表、详情、附件、候选、导出与写命令消费 v2；当前跟进负责人取客户当前主负责人；合法单据参与仅补充读取；组织筛选按当前主负责人所属组织；跨页与 CSV 携带 scope_version。S2-12 的 A34／A36 及业务验收未核销，不得登记本项整体完成 |
| S2-03 | M04 销售：业务组织范围、列表与详情、打印、候选、导出和写命令范围 | 业务组织与快照已采集；销售列表、详情、候选、跨页与 CSV 版本复核及主单创建／保存／提交／撤回／作废事务范围已接入；组织筛选按单据 `business_org_unit_id` 启用 `org_unit_ids`／`include_descendants`；关联客户／合同读取走 v2，create／save／submit 原写入事务内分别 `ContractAccess.require_with` 与 `CustomerAccess.require_with`，handler 事前检查不是唯一凭证；变更单沿原单业务归属且独立重验，前端变更列表回传 `scope_version`，`no_scope` 按空集、`DATA_SCOPE_CHANGED` 刷新，详情失败不得用列表行顶替；附件与独立打印／合同 PDF 消费者重验。任务命令仍归 S2-10。A34／A36 及业务验收未核销，不得登记本项整体完成 |
| S2-04 | M05 采购：业务组织范围、列表与详情、候选、导出和写命令范围 | 业务组织已采集；采购列表、详情、候选、跨页与 CSV 版本复核及主单创建／保存／提交／撤回／作废事务范围已接入；采购变更单及退货入口已沿来源采购单当前负责人和 `business_org_unit_id` 接入同一解析器；采购变更列表使用创建时间及稳定 ID 排序。变更 start／submit／cancel 在状态、版本、进行中校验前按来源采购单动作 `require_object`／`command_access.current`，不可见对象一律 NotFound。退货授权空集与越界原单保持 `$expr:false`。前端变更列表回传 `scope_version`，`no_scope` 按空集、`DATA_SCOPE_CHANGED` 刷新，详情失败不得用列表行顶替。`PurchaseDataScopePort` 归 `erp-procurement`，生产 adapter `erp-processes/src/adapters/purchase_data_scope.rs` 调用 `DataScopeService`。A34／A36 及业务验收未核销，不得登记本项整体完成 |
| S2-05 | 全部既有 DataScope 读取器与工作流事实适配器按资源、动作和身份维度解释 v2 | 未完成：工作台／工作流等仍为旧读取器，列为尚未接入；其目标接入必须经公共解析，禁止将无资源动作的旧覆盖计算作为 v2 授权结果 |
| S2-06 | 统一 `scope_summary`、`as_of`、权限、组织及业务归属版本；跨页变化错误 | 四类资源的列表摘要、授权时点、策略／组织／范围版本及跨页变化检查已接入。客户和合同按当前客户归属指纹，销售和采购按单据责任及版本校验；真实范围变化、账号及浏览器验收未核销 |
| S2-07 | 跨页查询与完整导出的版本一致性、最后一页之后及下载前的撤权重验 | 四类资源 CSV 已接入范围版本与完整结果收集后的再次校验；客户、合同导出及采购、销售范围元信息的前端模拟测试已通过。真实 HTTP 撤权和下载验收未核销，禁止登记 A16／A36 已验收 |
| S2-08 | 组织页面、影响预览、范围配置页面、URL 与 Query 缓存同步及窄屏交互 | 功能已接入并完成评审整改：主体/范围类型可见筛选与芯片、范围列表信封版本与 `empty_reason`、成员与管理授权按 `as_of` 过滤、权限未决加载与 403/无范围空态分离、预览命令与提交载荷一致、仅已接线资源动作可配置、组织筛选客户端裁剪故 Query key 不含筛选。浏览器与业务验收未核销，不得登记本项整体完成 |
| S2-09 | 已建设的订单导入与商城入口：明确责任映射、缺映射处理及首次生效来源证明 | 条件适用。商城尚未建设，当前登记不适用，不阻断 S2 退出；本阶段不要求新增商城或订单导入入口。已建设的订单导入入口须按实际建单链路纳入校验；未建设入口登记不适用。后续建设时必须同步交付本项规则并在正式业务开放前验收，不得将 ERP 建单规则视为这些入口已经接入 |
| S2-10 | A21—A25 关联任务管理边界、改派接收候选、执行资格失效及采购级联 | 部分接入：S2 订单及变更、采购入库、发货、电子／服务履约任务增加独立订单详情范围检查；工作台列表／统计／详情在分页前过滤，改派操作人与接收人、采购全部开放履约任务在原执行器重验；订单审批决定与恢复资格、详情及历史增加当前订单读取重验。管理视图增加 `EXECUTION_BLOCKED` 与负责人失效摘要，保留原责任和非审批受控管理动作。任务管理范围及审批原始范围读取仍未迁移；A21—A25 未整体核销 |
| S2-11 | 初始化重跑、真实范围解析、数据库并发、索引执行计划和真实账号验收 | 未执行 |
| S2-12 | 客户消费方 Port、组合层 adapter、窄组织事实、公共单对象判定及条件等价验证 | 客户、合同、采购均经本域 `DataScopePort::allows` 调用 adapter 和身份域 `ResolvedScope::allows`；销售命名用例直接复用公共判定。创建及主要单对象访问校验已接线；仓储创建政策不再进入生产构建。四类资源的条件编译须与公共单对象判定对拍；A34 的真实数据库等价性及 A36 的入口级真实范围变化未核销 |
| S2-13 | 资源动作接入登记、适用维度、配置与初始化准入、旧消费者阻断及架构门禁 | 配置、解析与初始化已共用 `consumers::registration`／`validate_binding`；初始化在 I/O 前校验主体、资源、全部动作和维度，事务内重查撤销留痕并推进策略版本。消费者登记已独立于种子清单；工作流／工作台旧读取器清理及阻断仍未完成，A35 不得整体核销 |

1. S2-01—S2-13 的全部适用要求完成前，本阶段状态必须保持“执行中”。S2-09 中未建设的入口按不适用登记，不作为 S2 未完成项，也不得登记为已实施或已验收。
2. S2 不包含 S3 的销售责任交接与验收责任来源切换；不得因本批增加业务组织字段而开放销售改派命令。
3. 不得将未接入资源登记为首发资源。已注册或已初始化 v2 规则不构成业务入口已经正确消费这些规则的证据。

### 4.1 公共解析接入登记

本表按 `b2cf10fb` 及本次工作区增量登记已知接入位置和缺口；属于服务端静态登记，不是可配置或上线放行清单。每批实施必须按上位合同第 9.5 节补齐逐资源动作、必需维度、历史参与动作及全部适用入口，不得只登记资源名称。

| 资源／消费方 | 已有公共解析与对象映射 | Port／adapter 状态 | 必须关闭的缺口 |
| --- | --- | --- | --- |
| 身份域组织与范围 | `erp-identity/src/service/access_control/resolve.rs`；`entity/access_control/resolved_scope.rs` | 身份域内部调用无需跨域 Port | 资源维度与准入登记；公共结果转换合同；单对象判定与条件编译的基准 |
| 客户 | `erp-customer/src/service/customer/{access,scope}.rs`；`ports/data_scope.rs`；`repository/scope.rs` | `CustomerDataScopePort` 已定义；生产 adapter `erp-processes/src/adapters/customer_data_scope.rs` 调用 `DataScopeService` | S2-12 剩余 A34／A36；同事务和时点的窄组织事实已由 Port 暴露；列表／详情／候选／导出／命令共同验证未核销 |
| 销售及关联成本 | `erp-read-models/src/sales_center/access.rs` 调用公共入口，销售 Repository 编译对象条件；命令由 `erp-processes/src/order_to_cash/authorization.rs` 复用；变更单由 `erp-processes/src/sales_change` 沿原单重验 | 命名组合用例可调用身份域；不得据此要求业务域反向依赖组合层 | 按 S3 第 6 章核对公共判定、条件等价、事务与动作；任务命令仍归 S2-10 |
| 合同／采购 | 合同：`erp-contract/src/service/contract/{access,scope,query}.rs`；`ports/data_scope.rs`；`repository/scope.rs`。采购：`erp-procurement/src/service/purchase_order/access.rs`；`ports/data_scope.rs`；`repository/purchase_order/scope.rs`；读模型与变更／退货入口映射当前采购负责人和 `business_org_unit_id` | 合同 `ContractDataScopePort` 已定义；生产 adapter `erp-processes/src/adapters/contract_data_scope.rs` 调用 `DataScopeService`。采购 `PurchaseDataScopePort` 已定义；生产 adapter `erp-processes/src/adapters/purchase_data_scope.rs` 调用 `DataScopeService` | S2-13；A34／A36 及业务验收未核销；接入前拒绝将初始化目录作为授权生效依据 |
| 工作台／工作流 | 旧读取器见 `erp-read-models/src/workbench/access.rs`、`erp-processes/src/adapters/workflow/authorization.rs` | 既有 WorkflowAuthorizationPort 仍传原始范围事实，尚未完成 v2 接入 | S2-05 及 S3-04；转换不得丢资源动作、启用状态或目标维度；保留任务执行与管理边界 |

### 4.2 实施与退出顺序

1. S2-12 的消费方 Port 与 adapter 已按第 9.4 节接线，须用领域边界门禁复验 A33；并核对 S2-13 的接入登记。后续领域必须复用该样例，不得再复制跨域直连接法。
2. 客户整改已保留主责／协作口径、组织条件、版本与 CSV 行为；A34／A36 通过前不得核销 S2-01、S2-12 的剩余缺口。
3. 合同、采购及工作流按当前实施批次接入，补齐资源动作登记和配置／初始化准入；每批移除自身旧授权解释。工作台仍按 S3-04 登记尚未接入，不因修改本文件改变其实施状态。
4. 退出证据必须分别列功能结果与 A33—A36 架构结果；命令、代码基线、入口覆盖和未执行验证必须可核对。范围基础通过不替代消费者验收。


### 4.3 S1 资源动作与单对象判定准入

1. 下表资源只支持 `internal_org`；该维度为必需维度。结算主体、仓库维度必须明确拒绝，不得丢弃后执行。Company 仍受个人上限与业务强制条件限制。
2. 历史参与只允许下表的 `list`、`detail`，写动作不得使用参与事实补授权。每个命令必须在原事务内按自身动作解析。
3. `org_unit:list/manage` 只用于组织配置边界，不开放历史参与。`cost_entry:list/detail` 与 `cost_allocation:list` 沿销售责任解释，仍执行 S3 独立资源检查，不因本表开放新能力。

| 资源 | 准入动作 | 公共单对象入口 | 权威对象事实与条件编译 |
| --- | --- | --- | --- |
| `customer` | `list/detail/create/update/delete` | `CustomerDataScopePort::allows` → `MongoCustomerDataScope` → `ResolvedScope::allows` | 当前主责、协作、合法历史归属；组织取主负责人当前主属组织。`customer::access::customer_scope` → `CustomerReadScope::document` |
| `contract` | `list/detail/create/update` | `ContractDataScopePort::allows` → `MongoContractDataScope` → `ResolvedScope::allows` | 当前客户主责及协作、合法合同参与；组织取主负责人当前主属组织。`contract::access::contract_scope` → `ContractReadScope::document` |
| `sales_order` | `list/detail/create/update/delete/submit/cancel_approval` | `SalesAccess::allows` → `ResolvedScope::allows` | 显式销售负责人、业务组织、客户协作及合法单据参与。`sales_center::access::sales_scope` → `SalesReadScope::document` |
| `purchase_order` | `list/detail/create/update/delete/submit/cancel_approval` | `PurchaseDataScopePort::allows` → `MongoPurchaseDataScope` → `ResolvedScope::allows` | 当前采购负责人、业务组织及合法单据参与；客户协作不产生采购范围。`purchase_order::access::purchase_scope` → `PurchaseReadScope::document` |

4. 列表、详情、候选、CSV、附件及命令的生产装配必须保持第 4.1 节路径；未装配 Port 的单对象判断必须返回错误，不得调用仓储创建政策兜底。
5. 客户创建必须具有有效主属组织；即使 Company 授权允许全范围，也不得生成缺失该必需责任事实的客户初始归属。
6. 工作流／工作台不在可配置清单中。其原始范围读取仍按 S2-05、S2-10 保留阻断项；不得凭本表登记 S2 退出。

## 5. 验证与记录规则

1. 后端必须执行 workspace 编译、格式、Clippy、workspace lib 单元回归、BPM 和领域边界检查。新增权限生成物须由 `web-api/build.rs` 生成。
2. 组织与范围单元测试必须覆盖同角色经理隔离、包含下级、跨团队管理、缺范围、个人上限、历史参与、多维范围、并发期望版本及有效期边界。
3. 销售单元测试必须证明缺归属快照时拒绝生效且实体不发生部分修改、首次生效时点与归属时点一致、重复冻结拒绝、编辑人变化不改写快照。
4. 仓库禁止执行真实数据库集成测试。本地检查记录不得登记真实 MongoDB 事务原子性、索引执行计划、生产授权或业务验收通过。
5. 尚未接入的业务路径不以单元测试或编译通过核销。全部交付及业务验收条件满足后，方可按上位合同推进状态。
6. 代码接入批次必须执行主合同第 12.1 节的领域边界检查与调用链审查，验证违规依赖夹具、adapter 调用公共入口、事务传递和失败关闭；不得为客户直连增加例外或放宽门禁。
7. A34 必须核对公共单对象范围判定与本域数据库条件编译在相同事实、动作、业务边界和筛选下的对象集合；A36 必须覆盖同资源各入口的动作独立解析。未验证项不得记为通过；代码批次必须登记本次实际执行结果。

## 6. 历史本地检查记录与当前复核边界

### 6.1 S2 基础批次历史记录

下表保留 2026-09-13 S2 基础批次记录，交付参考提交为 `4d9300e2`；其中工作区状态和“最后变更”均指该批次记录时点。该表不覆盖后续客户提交或主合同 v1.2 新增要求，不得作为当前 HEAD 的检查结果。

| 检查 | 记录 |
| --- | --- |
| `cargo check --workspace` | 通过 |
| `cargo clippy --workspace --all-targets --all-features -- -D warnings` | 通过；后续身份域、销售域变更的定向 Clippy 复验通过 |
| `env -u ERP_TEST_MONGO_URI cargo test --workspace --lib` | 3790 通过，64 忽略，0 失败 |
| 最后变更的定向单元复验 | 身份域 181 通过；销售域 224 通过，包含后补的 2 项快照拒绝与冻结检查 |
| 格式与差异检查 | `cargo fmt --all --check`、`git diff --check` 通过 |
| BPM 与领域边界 | 两项脚本通过 |
| 权限生成物 | 该批次记录时已生成 `org_unit:list`、`org_unit:manage` 对应路由权限；当时文件保留在工作区、未提交，未登记提交基线漂移检查通过；当前生成物与提交状态须另行核对 |
| HTTP、浏览器与前端交互 | 该批次未执行阶段验收；当时组织页面及四类资源前端接入未交付；后续客户接入按第 4 章登记 |
| MongoDB、并发、初始化重跑与业务验收 | 未执行 |

### 6.2 2026-09-14 接入静态登记（历史）

1. 客户交付参考提交为 `515a4ff1`、`1ba83e8b`；截至 `3988ae56` 仍为跨域直连。本批已改为 `CustomerDataScopePort` + 组合层 adapter，S2-12 的直连缺口关闭，A34／A36 仍未核销。
2. 本次同步只核对文档、客户范围元信息与导出调用链、依赖声明及资源准入入口，未重跑构建、测试、领域门禁或业务验收。既有“领域边界通过”记录不得用于证明当前直连符合合同；当前门禁执行结果须在整改批次重新登记。
3. 后续批次必须附代码基线、命令及结果，分别更新功能、架构和业务验收状态。历史测试计数不得累计为当前测试计数，S2 仍保持“执行中”。

### 6.3 当前增量验证要求（2026-09-15）

1. 基线为 `b2cf10fb`；本次增量未提交。历史批次结果不得替代本节检查。
2. A34 必须运行四类资源的公共判定／实际 BSON 条件编译对拍；输入须覆盖空角色范围、角色并集、个人上限、本人、协作、组织、历史参与及读取／创建／更新动作。受限 BSON 内存解释器只构成本地证据，不构成真实 MongoDB 查询等价性证据。
3. A35 必须运行初始化未接入动作、维度不支持、主体／资源不一致、空清单和未装配 Port 的失败关闭测试。初始化重跑、并发与管理员撤销留痕仍须按允许的业务验收环境验证。
4. S2-05、S2-10、S2-11 及真实业务验收保持未完成，不得因本批编译或单元通过核销。S2-09 按第 4 章登记适用性；未建设的商城入口不列入当前阻断项。

| 检查 | 本次结果及证据边界 |
| --- | --- |
| `cargo check --workspace` | 通过；末轮全 workspace 严格 Clippy 同时复核编译 |
| `cargo clippy --workspace --all-targets --all-features -- -D warnings` | 通过；已整理合同授权 Port 装配、查询参数、测试位置及未使用的处理器参数，未放宽 lint |
| `env -u ERP_TEST_MONGO_URI cargo test --workspace --lib` | 3895 通过、64 忽略、0 失败；末轮改动另执行受影响 crate 单元复验，计数分别登记 |
| 末轮受影响 crate 单元复验 | `env -u ERP_TEST_MONGO_URI cargo test -p erp-contract -p erp-procurement -p erp-sales -p erp-processes --lib`：1080 通过、6 忽略、0 失败；与 workspace 计数分别登记，不累计为新增测试数 |
| 通用条件测试工具 | `env -u ERP_TEST_MONGO_URI cargo test -p test-support --lib filter::`：2 通过，证明未知操作不会被静默忽略；仅供测试，不进入生产授权政策 |
| 四类资源公共判定／条件编译 | 4 项穷举测试通过，共 156672 组内存输入；通过生产条件编译与公共判定对拍，不作为真实数据库执行等价性验收 |
| 未装配 Port／初始化准入 | 客户、合同、采购单对象端口失败关闭；初始化主体、资源、动作、维度及空清单拒绝测试通过；身份域末轮 191 项单元通过 |
| 前端定向验证 | 10 个文件、33 项通过，覆盖范围版本、客户／合同导出撤权模拟、采购／销售元信息、组织配置与缓存；未执行真实账号浏览器验收 |
| BPM／领域边界 | `check-bpm-boundaries.sh` 通过；`check-org-data-scope.sh` 内嵌的领域边界检查及违规夹具通过 |
| 第 9 章静态总门禁 | `check-org-data-scope.sh` 返回 1：1999 个活动文件，仍有 5 项工作流原始范围读取阻断。不得登记 A33—A36 全部核销 |
| 格式、差异及权限生成物 | `cargo fmt --all --check`、`git diff --check` 通过；权限生成文件与 HEAD 无差异 |
| OpenAPI | `npx redocly lint ../docs/organization-data-scope-s2-openapi.yaml` 校验有效；保留缺少 license 与本地开发 server 两项元数据提示 |
| 真实数据库、并发、初始化重跑、索引计划及业务验收 | 未执行；继续按第 5 章及上位合同第 10 章执行，不以本表本地结果替代 |

### 6.4 订单关联任务增量准入（2026-09-15）

1. 代码基线为 `b45b5f85`；本节工作区增量未提交。S2 保持“执行中”，不得将本节登记为全阶段完成。
2. `OrderTaskSource` 由唯一任务事实读取器从销售／采购主键及变更、履约实体外键构造。销售变更与采购变更必须分别沿原销售单、原采购单解释；不得复用展示根节点、参与根节点、创建人或 `owner_organization_id` 推断授权来源。
3. `WorkflowAuthorizationPort::require_order_task_read`／`readable_order_sources` 由 `erp-processes` 装配，分别复用 `SalesAccess`／`PurchaseAccess` 和公共单对象判定。批量读取按订单种类解析，单批最多 500 个去重来源；未装配、缺失来源、类型错配和无效范围配置必须失败关闭。
4. 工作台列表、统计、详情与命令读取结果必须先执行关联订单范围过滤。改派操作人和接收人必须在原事务执行器重验；采购责任键的全部开放履约任务逐项重验。事务内必须复用已装配事实 Port 的任务服务，禁止重建未装配服务；资格检查的配置及基础设施错误必须原样传播。任一任务不满足读取资格时，不得进入责任写入。
5. `order_approval_readable` 必须在订单审批决定与恢复的资格重验路径检查当前订单详情范围；越界按审批现有 `CannotReadSubject` 资格失败处理。审批详情与历史不得以启动人或冻结责任绕过当前订单范围。
6. 已授权管理视图必须保留失效任务的原负责人、业务组织和版本。负责人失去账号、完整执行权限或订单读取资格时，返回 `EXECUTION_BLOCKED` 和 `WORK_ITEM_OWNER_INELIGIBLE`；移除执行、通过、驳回动作，仅保留已授权查看及非审批受控管理动作。已有审批受阻原因必须保留；不得自动换人或完成任务。
7. 本节未替换工作流和工作台的原始任务管理范围计算，不核销 S2-05、S2-13 或 A35；审批绑定及管理范围的统一迁移、任务队列跨页版本锚点、S2-11 和真实账号验收仍须完成。订单详情补充校验不得作为上述缺口的替代证明。

| 检查 | 当前增量记录 |
| --- | --- |
| `cargo check --workspace` | 通过 |
| `cargo clippy --workspace --all-targets --all-features -- -D warnings` | 通过；末轮包含事务内 Port 复用修复 |
| `env -u ERP_TEST_MONGO_URI cargo test --workspace --lib` | 3903 通过、64 忽略、0 失败；后续事务装配和负责人执行资格调整另按下列受影响 crate 复验 |
| 末轮工作流及组合层回归 | `env -u ERP_TEST_MONGO_URI cargo test -p erp-workflow -p erp-processes --lib`：868 通过、9 忽略、0 失败；包含事务内 Port 复用修复，不与 workspace 计数累计 |
| 末轮工作台读模型回归 | `env -u ERP_TEST_MONGO_URI cargo test -p erp-read-models --lib`：210 通过、35 忽略、0 失败；覆盖原执行权限组合及停用角色排除，不与 workspace 计数累计 |
| 前端范围及任务定向测试 | 15 个文件、36 项通过；覆盖组织预览提交载荷及执行受阻任务的查看／处理边界 |
| 前端 TypeScript | `npx tsc --noEmit --pretty false` 通过；组织变更测试的提交 mock 已补齐请求参数类型 |
| BPM／领域边界 | `check-bpm-boundaries.sh` 通过；`check-org-data-scope.sh` 内嵌领域边界检查及违规夹具通过 |
| 第 9 章静态总门禁 | `check-org-data-scope.sh` 返回 1：2002 个活动源码文件，仍有 5 项工作流原始范围读取阻断；不得登记 S2-05、S2-13 或 A35 已核销 |
| 格式与差异检查 | `cargo fmt --all --check` 未通过：当前配置为 2024 风格，HEAD 大量已有文件仍为 2021 风格；未混入全库重排。27 个变更 Rust 文件按既有 2021 风格与 110 列检查通过，前端变更文件的 Oxfmt／Oxlint 及 `git diff --check` 通过。不得将定向检查登记为 workspace 格式门禁通过 |
| 真实数据库、HTTP／浏览器、并发、初始化重跑和业务验收 | 未执行；遵守第 5 章验证限制，不登记通过 |
