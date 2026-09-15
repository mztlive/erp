# S1 负责人基础查询交付与验收合同

状态：本地检查通过；业务验收未执行

实施日期：2026-09-13

修订日期：2026-09-14

上位合同：[组织架构、数据范围与负责人查询执行合同](organization-data-scope-contract.md) v1.2，第 11 章及第 9.2—9.5 节阶段衔接要求。

## 1. 生效范围

1. 本批交付客户、合同、销售单、采购单四类主对象的当前负责人查询、详情展示和完整结果 CSV 导出。
2. 人员筛选仅收窄既有读取结果，不授予动作权限，不写入任务，不开放销售责任改派。
3. S1 不作为正式业务上线依据。组织树、成员与管理关系、DataScope v2、业务组织归属、首次生效人员与组织快照必须在 S2 完成。
4. 不提供旧姓名筛选、旧销售负责人推导或旧业务数据回填。开发环境必须使用符合新字段要求的测试数据；不得通过读取时补创建人绕过责任字段要求。
5. 本文“本地检查通过”只覆盖 S1 负责人基础查询批次。S2、S3 的公共解析、消费方 Port、组合层 adapter、资源接入登记及 A33—A36 按上位合同执行；不得以 S1 记录证明后续 DataScope 接入符合架构要求。后续资源新增的组织参数、范围元信息及导出重验，以对应 S2、S3 合同为准，不得将本阶段未支持字段永久判为非法。

## 2. 责任事实与显示

| 对象 | 当前负责人权威来源 | 查询和展示要求 |
| --- | --- | --- |
| M01 客户 | 当日有效的 `customer_assignment`，角色为 `OWNER`，有效期遵循原自然日半开区间 | 先与当前用户客户范围求交，再按人员 ID 过滤；不以协作或创建人代替主责 |
| M02 合同 | 合同所关联客户的当前主负责人 | 列表、详情和 CSV 使用当前跟进负责人；客户换任不改历史销售单 |
| M04 销售单 | 稳定主表 `sales_owner_user_id` | ERP 建单时取认证建单人；列表、详情、打印和 CSV 使用同一字段；普通编辑、提交不修改该字段 |
| M05 采购单 | 稳定主表 `owner_user_id` | 沿用采购责任形成与改派规则；筛选、列表、详情、CSV 使用当前采购负责人 |

1. `created_by` 保留创建人审计及原有工作视图用途，不作为指定负责人筛选参数。
2. 人员候选随本资源列表响应返回，不调用系统账号管理接口获取负责人全集。候选基于完整可见对象范围，不从当前页推导。
3. 候选以稳定 ID 为值，以姓名、登录账号和停用标记区分同名人员；停用但仍负责可见业务的账号必须保留。
4. 失效或不在候选中的 URL 条件必须显示为可清除的已选条件；不得静默丢弃条件并查全量。
5. 本批不变更工作台、审批、履约、客户验收、采购任务级联和任务改派资格。派生单据继续使用既有来源关联；派生单据独立管理查询按后续阶段接入。

## 3. API 与页面执行规则

| 接口 | 新参数 | 负责人响应 |
| --- | --- | --- |
| `GET /admin/customers` | `owner_user_ids` | 行 `owner_user_id` / `owner_user_name`，页 `owner_options` |
| `GET /admin/customers/all-authorized` | `owner_user_ids` | 同上；保持既有全量客户读取权限检查 |
| `GET /admin/contracts` | `owner_user_ids` | 行 `owner_user_id` / `owner_user_name`，页 `owner_options` |
| `GET /admin/contracts/{id}` | 无新增查询参数 | 增加 `owner_user_id` / `owner_user_name` |
| `GET /admin/sales-orders` | `owner_user_ids` | 既有 `owner_user_id` / `owner_user_name` 改由显式销售负责人产生；页增加 `owner_options` |
| `GET /admin/purchase-orders` | `owner_user_ids` | 既有采购负责人字段；页增加 `owner_options` |

1. `owner_user_ids` 为逗号分隔的稳定 ID，服务端去首尾空白、去重并限制单次最多 100 项。省略表示不增加人员条件；显式空值、空片段、显示姓名或非法身份格式必须拒绝。
2. 同一参数内按 OR，与客户、状态、关键词和既有授权条件按 AND。不得在分页后做人员过滤。
3. 四类列表的未知查询参数必须被拒绝；旧 `owner` 姓名参数与 S1 尚未接入的组织、处理人参数不得被静默忽略。HTTP Query 解码错误返回 400；领域校验继续使用原错误响应合同。
4. 人员条件以 `ownerUserIds` 保存在页面 URL，并进入 TanStack Query key。选择草稿仅在点击查询后生效；应用、刷新、分页、清除使用同一已应用条件。
5. 销售单创建人条件与负责人条件分开显示、分别清除。人员选择不得改变“待我处理”“我创建的”的原有任务含义。
6. 四类列表响应增加 `ownership_basis`，明确当前负责人事实来源。S2 必须补齐统一 `scope_summary`、`as_of` 和真实权限／组织版本；S1 不生成占位权限版本。
7. 增量 OpenAPI 见 [S1 查询接口定义](organization-owner-query-openapi.yaml)。该文件定义本批新增字段和校验边界，既有列表业务字段仍按原 DTO 执行。

## 4. CSV 导出

1. 四类对象从第一页开始，按每页 100 条重复调用原列表接口，保留全部已应用条件和稳定排序。
2. 导出必须收集完整结果后才生成下载；每页继续执行原列表鉴权。不得输出当前页或前 100 条并登记完成。
3. 查询失败、权限拒绝、总数变化、重复记录或非预期空尾页必须终止导出，不下载部分结果。失败后允许重新查询并重试。
4. 合同、销售导出不再创建无法生成本次文件的占位后台任务，不返回虚构权限版本或排队成功。
5. 该导出为浏览器即时下载，不建设历史下载入口。S2 必须完成范围版本一致性及最后一页之后的撤权重验；S1 不声称跨请求结果具有事务快照或墙钟级即时撤权保证。

## 5. 数据模型、索引与初始化

1. 销售稳定主表增加必需字符串 `sales_owner_user_id`。ERP 新建时与主表一并写入，不向普通更新 DTO 暴露改派字段，反序列化缺字段必须失败。
2. 销售查询增加 `idx_sales_order_owner_created`：`sales_owner_user_id + deleted_at + created_at + id`。
3. 采购查询增加 `idx_purchase_order_owner_created`：`owner_user_id + deleted_at + created_at + id`。
4. 客户、采购分页排序追加 `id`；销售、合同保留原有稳定排序。客户复用归属有效期和客户身份索引，合同复用关联客户查询路径。
5. 新索引纳入原拥有领域的幂等索引登记入口，不新增授权种子，不更改现有角色默认范围。
6. S2 必须补齐商城同步／导入责任映射、组织字段必填、首次生效快照及显式授权初始化。禁止在本批之后直接开放缺少这些事实的正式业务。

## 6. 文件归属

| 层 | 实现入口 |
| --- | --- |
| 通用查询合同 | `backend/crates/application-core/src/query_ids.rs` |
| 候选账号事实 | `erp-identity/src/repository/account_core.rs`；客户／合同消费 Port 与 `erp-processes/src/adapters/{customer,contract}.rs` |
| 客户查询 | `erp-customer/src/dto/customer.rs`、`service/customer/mod.rs`、`repository/customer.rs` |
| 合同查询 | `erp-contract/src/dto/contract.rs`、`service/contract/{mod,query}.rs`、`repository/list_search.rs` |
| 销售责任与查询 | `erp-sales/src/entity/sales_order/entity/order.rs`、销售 DTO／Repository／Service、`erp-read-models/src/sales_center/order/{query,status}.rs` |
| 采购查询 | `erp-procurement/src/dto/purchase_order/query.rs`、`repository/purchase_order`、`erp-read-models/src/purchase_center/query.rs` |
| HTTP | `backend/apps/web-api/src/core/handler/{customer,contract,sales_order,purchase_order}` |
| 前端 | `erp-client/features/{customers,contracts,sales-orders,purchase-orders}`；`features/entity-selectors/components/responsible-user-filter.tsx`；`lib/list-export.ts` |

## 7. 验收要求与记录

以下为 2026-09-13 S1 批次历史记录，交付参考提交为 `c9fd41e8`；保留原测试计数及验证边界。后续代码变更必须单独登记执行基线和结果，不得将本表视为当前 HEAD 或主合同 v1.2 新增架构检查已经通过。

| 验收项 | 要求 | 当前记录 |
| --- | --- | --- |
| A07 同名与参数 | 两个同名账号按 ID 分开；旧姓名、空值、超限和未注册参数拒绝 | HTTP Query 与 ID 值对象单元检查通过；四页模拟候选和筛选通过 |
| A10 一致性 | 修改编辑人不改变负责销售；列表、详情、打印和 CSV 同源 | 实体、BSON 合同与前端投影单元检查通过；真实业务打印未执行 |
| A15 跨页与导出 | 跨页总数和人员条件一致；201 条结果完整导出；中途失败不下载部分文件 | 前端单元检查通过 |
| A21 查询与任务 | 查询链路不调用任务写入，责任改派与审批资格保持 | 差异核对通过：查询接线无任务写入；未增加销售改派入口 |
| 前端回归 | 四个功能目录单元测试、类型检查、Lint | 157 项单元测试、TypeScript 与 Lint 通过 |
| 后端回归 | workspace lib 单元、编译、Clippy、格式、BPM 与领域边界 | workspace：3775 通过、64 忽略；最终客户域复验 54 通过；编译、Clippy、格式、BPM、领域边界和权限生成物漂移检查通过 |
| 浏览器 | 同名候选、多选、应用、清除、刷新、分页与窄屏 | 四类同名／停用候选筛选、四类导出通过；销售刷新、清除、102 条跨页导出通过；销售与采购 390px 页面无横向溢出 |
| MongoDB 与业务验收 | 数据库执行计划、真实归属交接、权限与并发、实际导出核对 | 未执行；按仓库约束不运行真实数据库集成测试 |

1. 本地检查通过后，开发执行者仅可登记“本地检查通过”，不得登记“已验收”。
2. 浏览器模拟数据仅证明页面请求、呈现和交互，不作为数据库权限或生产验收证据。
3. 首发清单必须继续关闭尚未满足上位合同第 10 章准入条件的资源。

## 8. 本地检查执行命令

后端命令必须从 `backend/` 执行：

```sh
cargo fmt --all --check
cargo check --workspace
cargo clippy --workspace --all-targets --all-features -- -D warnings
env -u ERP_TEST_MONGO_URI cargo test --workspace --lib
env -u ERP_TEST_MONGO_URI cargo test -p erp-customer --lib
./scripts/check-bpm-boundaries.sh
./scripts/check-domain-boundaries.sh
./scripts/check-permissions-drift.sh
```

前端命令必须从 `erp-client/` 执行：

```sh
npx tsc --noEmit
npm run lint
npx vitest run features/contracts features/customers features/purchase-orders features/sales-orders features/entity-selectors/components/responsible-user-filter.test.tsx tests/list-export.test.ts tests/owner-query-state.test.ts
```

浏览器检查使用独立本地前端与模拟 API：105 条对象中，两个同名负责人分别关联 102 条和 3 条；四类列表须按所选 ID 请求并显示 3 条对应对象。销售选择 102 条的一方后，刷新保留条件，第 6 页仅显示最后 2 条，导出必须重新读取全部 102 条。390px 检查覆盖销售、采购的已选人员和筛选菜单。模拟环境不得登记真实数据库、组织范围或业务验收通过。

## 9. HEAD 本地重验补登记（2026-09-15，基线 8b3fc1e4）

本节为 2026-09-15 在 S1 专用 worktree 对 HEAD 提交 `8b3fc1e4`（`fix(org-scope): S1 销售采购补齐 ownership_basis 与服务端 as_of`）的本地重验补登记，只记录本次执行的命令与结果，不改写 §7 历史表及其计数。

1. 执行环境：worktree 路径 `/Users/huangjiajiang/Development/erp-s1-remaining`，分支 `org-scope/s1-remaining`，基线 `8b3fc1e4`。未合并到 main，未改父仓库。
2. 本次 6 文件修复内容（均为加法透传，无行为破坏）：
   - `erp-client/features/sales-orders/api/sales-orders-list.ts`：响应 wire 类型新增可选 `as_of` / `ownership_basis`，回传 `asOf` / `ownershipBasis`，`queriedAt` 改为服务端 `as_of` 优先、缺失时回退本地 `formatIsoNow()`。
   - `erp-client/features/sales-orders/api/contracts.ts`：`SalesOrderListView` 新增可选 `asOf` / `ownershipBasis`。
   - `erp-client/features/purchase-orders/api/purchase-order-queries-api.ts`：响应 wire 类型新增可选 `as_of` / `ownership_basis`，回传 `asOf` / `ownershipBasis`，`freshness.updatedAt` 改为服务端 `as_of` 优先、缺失时回退本地时间。
   - `erp-client/features/purchase-orders/api/purchase-orders-contract.ts`：`PurchaseOrderListResult` 新增可选 `asOf` / `ownershipBasis`。
   - `erp-client/features/sales-orders/api/sales-orders-list.test.ts`、`erp-client/features/purchase-orders/api/purchase-order-queries-api.test.ts`：mock 补 `as_of` / `ownership_basis` 并断言透传。
   - 对抗评审结论：wire 新增字段均为可选，服务端缺失时回退旧行为；请求参数、URL、Query key、文案、自动化 id 均未改动；筛选语义 unchanged；测试 mock 与断言有效。无 blocker，无需修复。
3. 后端定向命令与结果（从 `backend/` 执行）：`cargo fmt --check` exit 0；定向 6 crate `cargo check` exit 0；lib 单测 625 通过、0 失败，分 crate 计数为 application-core 11、contract 60、customer 70、procurement 251、sales 233；BPM 边界、领域边界、权限漂移、`git diff --check` 均 exit 0。
4. 前端定向结果（从 `erp-client/` 执行）：`npx tsc --noEmit` exit 0；7 文件 vitest 19/19 通过（`features/contracts/api/list.test.ts`、`features/customers/api/directory.test.ts`、`features/purchase-orders/api/purchase-order-queries-api.test.ts`、`features/sales-orders/api/sales-orders-list.test.ts`、`tests/list-export.test.ts`、`tests/owner-query-state.test.ts`、`features/entity-selectors/components/responsible-user-filter.test.tsx`）；本次 6 文件 `oxlint --deny-warnings` 与 `oxfmt --check` 通过；全量 `npm run lint` exit 1，存量 6 处失败均在本次范围外、未改：
   - `features/sales-orders/components/sales-orders-list-filter-panel.tsx`：jsx-a11y label 警告。
   - `features/customers/pages/customer-center-directory-toolbar.tsx`：jsx-a11y label 警告。
   - `features/contracts/components/contracts-table-panel.tsx`：jsx-a11y label 警告。
   - `features/sales-orders/lib/sales-orders-list-filters.ts`：no-extra-boolean-cast 错误。
   - `features/organization/lib/impact.ts`：no-unused-vars（`afterUnits`）错误。
   - `features/organization/components/organization-layout.test.tsx`：no-unused-vars（`container`）错误。
5. 仍未执行项：真实 MongoDB 集成测试、业务验收、浏览器真实账号验证、全仓 workspace 门禁均未执行。S1 状态仍为“本地检查通过”，未验收；不声称 S2 或 A33—A36 通过。

## 10. HEAD 全量本地重验（2026-09-16，基线 ceb0fdb0）

本节为 2026-09-16 在工作区路径 `/Users/huangjiajiang/Development/erp`、分支 `main`、基线 `ceb0fdb0`（`docs(org-scope): S1 补登记 HEAD 本地重验`）上执行的 S1 §8 全量本地重验。不改写 §7、§9 历史表。状态仍为“本地检查通过”，不得登记“已验收”。

1. 本批为打通 §8 全仓门禁与模拟浏览器检查而做的修复（不含 S1 查询语义变更）：
   - 前端 lint：`sales-orders-list-filter-panel.tsx`、`customer-center-directory-toolbar.tsx`、`contracts-table-panel.tsx` 为包含下级 Checkbox 补 `htmlFor`；`sales-orders-list-filters.ts` 去掉多余 `Boolean()`；`organization/lib/impact.ts`、`organization-layout.test.tsx` 删除未使用变量。
   - 后端 Clippy：nightly `clippy::collapsible_if` 在 `--all-targets` 下阻断全仓；按 Clippy 建议将嵌套 `if let` 收成 let-chain（含 `web-api/build.rs` 与多领域 crate）。另有 `login_rate_keys` / 采购草稿测试 helper 的直接返回整理。
   - 浏览器：新增 `e2e/tests/s1-owner-query-mock.spec.ts`。登录真实本地 web-api 后拦截客户/合同/销售单/采购单列表，105 条中同名负责人分别 102 与 3 条。
2. 后端命令与结果（从 `backend/` 执行）：`cargo fmt --all -- --check` exit 0；`cargo clippy --workspace --all-targets --all-features --locked -- -D warnings` exit 0；`env -u ERP_TEST_MONGO_URI cargo test --workspace --lib --locked` 3891 通过、64 忽略、0 失败；`env -u ERP_TEST_MONGO_URI cargo test -p erp-customer --lib --locked` 70 通过；BPM 边界、`check-domain-boundaries.sh --cutover`、权限漂移、`git diff --check` 均 exit 0。
3. 前端命令与结果（从 `erp-client/` 执行）：`npx tsc --noEmit` exit 0；全量 `npm run lint` exit 0；§8 vitest 53 文件 183 通过。
4. 浏览器：`cd e2e && npx playwright test tests/s1-owner-query-mock.spec.ts` 1/1 通过。覆盖四类列表按 `owner_user_ids` 显示 3 条、销售 102 条刷新保留条件、第 6 页 2 条、`page_size=100` 重读 102 条导出、销售/采购 390px 已选人员与筛选菜单无横向溢出。模拟环境不得登记真实数据库、组织范围或业务验收通过。
5. 仍未执行：真实 MongoDB 集成测试（仓库禁止）、真实业务打印、真实账号权限/并发/执行计划核对、业务验收。不声称 S2 或 A33—A36 通过。
