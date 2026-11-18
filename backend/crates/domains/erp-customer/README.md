# erp-customer：客户账户与分配

## 职责

本 crate 管理「哪些往来主体是我们的客户」，以及客户账户的启停、默认付款条件和人员归属。客户账户以 `party_id` 引用往来主体，不复制主体的法定名称、联系人、地址、税务资料和银行账户。同一个主体可以同时是客户和供应商，两种账户各自维护。

### 负责的数据与能力

- **客户账户**（`customer_accounts`）：`CustomerAccount` 保存客户编号、主体引用、启停状态和默认付款条件。更新不能改写客户编号或主体引用。停用不同于软删除：停用阻断上传新合同 PDF、创建销售单等新业务动作，不切断历史单据引用。
- **客户分配**（`customer_assignments`）：`CustomerAssignment` 记录客户、人员、角色、有效期和调整原因。生效区间为 `[valid_from, valid_to)`，无结束日表示长期有效。同一客户可有多名 `OWNER`，但同一客户、人员和角色的有效期不得重叠。
- **客户资料命令回执**（`customer_profile_commands`）：`CustomerProfileCommand` 保存建档或修订的成功结果、幂等键、发起人和请求指纹，并提供重放匹配规则；不保存原始请求正文。主体资料写入由跨域流程负责。

### 使用场景

- 按权限查询客户账户列表、账户详情、人员分配记录，或调整账户状态、默认付款条件与负责人关系。
- 修改客户资料保存请求的结构校验、指纹或成功回执时，从本域合同入手；需要同时维护主体资料时，进入主数据流程。

## 目录结构

| 路径 | 职责 |
| --- | --- |
| [src/lib.rs](src/lib.rs) | 公开模块与重导出：服务、访问器、DTO、实体、Port、仓储及错误合同。 |
| [src/entity/](src/entity/mod.rs) | 客户账户、归属、分配命令、资料命令和纯结构校验；不执行 I/O。 |
| [src/dto/](src/dto/mod.rs) | 账户、归属、资料请求和响应；`party_snapshot` 是对外展示合同，不是主体实体。 |
| [src/service/customer/](src/service/customer/mod.rs) | `CustomerService` 和 `CustomerAssignmentService` 编排单域用例；`access.rs` 接入统一授权，`query.rs` 和 `scope.rs` 处理账户读取，`profile/` 适配资料校验和指纹。 |
| [src/authorization/](src/authorization/mod.rs) | `CustomerFacts` 提供客户对象事实，`CustomerMapping` 将授权事实映射到客户查询模型，`CustomerResolvedScope` 携带本次查询凭证。 |
| [src/ports/](src/ports/mod.rs) | 客户侧使用的主体事实 `PartyFactPort` 和审计写入 `CustomerAuditPort`。账号资格和姓名通过 identity 的 `AccountDirectoryService` 获取，不另设账号 Port。 |
| [src/repository/](src/repository/mod.rs) | `CustomerExt` 提供类型化 MongoDB 集合访问器；仓储负责查找、投影、有效期过滤、排序和分页，`scope.rs` 执行带授权条件的账户查询。 |
| [src/indexes/](src/indexes/mod.rs) | 定义本域索引，通过公开的 `indexes::ensure` 依次注册账户、分配、资料命令三个集合。 |

## 架构设计

### 依赖与协作边界

术语与统一分工见[总索引](../../README.md#代码中常见名称的含义)。本域维护客户事实和仓储；常规、构建和测试依赖都不指向其他业务域（`erp-identity` 的公开接口除外）、编排层或读模型层。权限常量来自 `erp-permissions`，策略判断和凭证编译由 [erp-identity](../erp-identity/README.md#业务授权-api) 的共享 `AuthorizationService` 执行。应用组合根装配客户 Facts、Mapping 和 `CustomerAccess`，并将 `PartyFactPort`、`CustomerAuditPort` 分别接到 `erp-party` 的仓储读取和 `erp-audit` 的写入。

管理端 `/admin` 统一做 JWT 认证。客户路由的 `permission_route` 只登记权限目录，不做运行时授权；Handler 复用 DTO，把请求和操作人交给 Service、Process 或 ReadModel，不直接查询集合。`CustomerAccess` 用 `require` 检查候选、现有客户或归属变更，用 `query` 获取可下推到仓储的查询凭证。常规客户范围事实基于上海自然日有效的 `OWNER` 及其主属组织；历史 `OWNER` 参与事实只在允许的读取规则中补充覆盖，不授权写入，`COLLABORATOR` 也不扩展客户授权范围。

### 客户账户与分配读写链路

以下路径均相对于 `/admin`。

| HTTP 入口 | 业务入口与实际链路 |
| --- | --- |
| `GET /customers`、`GET /customers/all-authorized` | 都进入 `CustomerService::customer_list`，使用 `customer:list` 编译查询。普通入口拒绝 `all_authorized`；专用入口固定该口径，只取消本人归属这一额外筛选，不扩大授权范围。专用路由标注 `customer_scope:detail`，当前实现不另行执行该资格检查。 |
| `GET /customers/{id}` | `CustomerService::customer_detail` 在同一快照中按 `customer:detail` 对目标 ID 应用完整授权条件，读取账户、主体身份及当前负责人和姓名，交付前复验凭证。不可读客户不返回账户数据。`load_customer_detail` 只负责取数和组装，调用方必须已证明读取资格。 |
| `POST /customers` | `CustomerService::create_customer` 为已有主体建账户：在写事务里验证创建候选范围、创建人主属组织及登录资格，并通过主体 Port 验证主体存在。首条 `OWNER` 固定为认证操作人，不采信请求的 `owner_user_id`。账户、首条归属和成功审计同事务提交。 |
| `PUT /customers/{id}` | `CustomerService::update_customer` 在写事务内用 `customer:update` 检查目标范围，核对请求版本，按乐观锁更新默认付款条件与启停状态并写审计。完整主体资料不走此入口。 |
| `DELETE /customers/{id}` | 实际进入 `erp-master-data::delete_customer`；它持有带审计的事务，按 `customer:delete` 重验目标范围，调用本域仓储软删除账户，不删除主体。 |
| `GET /customers/{id}/assignments` | `CustomerAssignmentService::customer_assignment_list` 在快照中执行 `customer_assignment:list` 操作资格检查；当前入口不额外检查 `customer:detail` 客户对象范围。仓储按路径客户 ID、可选人员与角色过滤未软删除的归属记录，按白名单排序并分页计数，不限当前有效期。该列表的 `user_name` 目前回填账号 ID。 |
| `POST /customers/{id}/assignments` | `CustomerAssignmentService::apply_assignment` 将 DTO 转成 `Assign` 或 `End` 命令。在写事务内检查 `customer_assignment:create` 操作资格及 `customer:update` 目标范围。`Assign` 检查目标账号可登录；新增 `OWNER` 时还检查候选负责人归属范围，拒绝重叠，不自动结束旧关系。`End` 核对归属所属客户及请求版本，提前结束 `OWNER` 时要求其他负责人连续覆盖被缩短的区间，并按区间重验剩余负责人集合的目标范围。两种动作均更新客户根记录以串行化同一客户的并发变更，归属写入与审计同事务提交。 |

账户列表在同一快照叠加归属、负责人、组织、状态、主体和关键词筛选；关键词匹配客户编号或 `PartyFactPort` 返回的主体名称命中。`mine`、`collaborating`、`assigned` 当前都按本人有效 `OWNER` 关系收窄。授权条件和业务条件共同用于分页、计数及客户版本查询；当前页批量补齐主体身份、归属和人员姓名，不返回负责销售候选。列表把授权版本与匹配客户的 ID、版本合成 `scope_version`，交付前复验凭证并在同一快照重算版本；第二页起缺少版本或版本不一致返回 `DATA_SCOPE_CHANGED`，不以公司范围兜底。

### 仓储、索引与事务

Repository、Port 和事务内业务步骤复用调用方传入的 `persistence_core::Executor`，不自行开事务。拥有用例的 Service 或 Process 管理原子写入，并在事务内完成决定写入的授权和数据校验。普通仓储查找不从登录人推导权限；`repository::scope` 校验凭证的集合、对象粒度及读取阶段，将 identity 编译的条件交给 MongoDB 聚合执行，而不是先取出全量客户再过滤。

`customer_accounts` 对 `party_id` 和 `customer_no` 使用全局唯一索引，软删除后仍保留身份占用。分配索引兜底拒绝重复的 `(customer_id, user_id, assignment_role, valid_from)`，完整的区间重叠检查仍由实体在事务中根据现有归属判定。资料命令以 `idempotency_key` 唯一索引防止重复回执。索引和仓储共用 `CustomerExt` 的集合名，账户更新、软删除和结束归属的持久化使用版本比较写入。

### 协作示例

- 给客户增加负责销售时，直接调用本域的分配服务；保存联系人则由 [erp-master-data](../../orchestration/erp-master-data/README.md) 的 `CustomerProfileProcess` 协调客户账户与 [erp-party](../erp-party/README.md) 的主体和资料仓储。`erp-party` 拥有主体身份、修订、从属资料及敏感数据编解码规则。
- `POST /customer-profiles` 建档时新建主体，或在确认后复用尚无客户账户的既有企业；复用保留其已有资料，首位负责人仍为认证创建人。`PUT /customer-profiles/{id}` 同时核对客户和主体版本，新增主体修订并保存显式提交的资料变更；负责人改动必须走分配入口。Process 在同一事务检查客户范围与资料字段权限，并保存变更、成功回执和审计。
- 资料命令重试先重验权限，再按原幂等键核对操作、修订目标、发起人及请求指纹，匹配时返回原结果。指纹基于 DTO 的 JSON 字节计算 SHA-256；内存使用版本化指纹，持久化保留兼容旧记录的裸摘要。提交结果不明时流程只查证原回执，不重做写入。`GET /customer-profile-commands/{idempotency_key}` 也只回读成功结果：校验回读功能资格，非原发起人还须通过关联客户的详情范围。
- `GET /customer-profiles/{id}` 走 [erp-read-models](../../read-models/erp-read-models/README.md) 的 `CustomerProfileReadService::detail`，在同一授权快照组装账户、主体修订、归属历史和人员姓名，按字段权限裁剪从属资料及敏感字段令牌，交付前复验客户读凭证。`POST /customer-sensitive-fields/reveal` 则由主数据流程校验短时令牌、客户范围、字段资格及归属，解密单字段并写审计。
- 客户中心的合同与销售摘要走 `CustomerCenterReadService::related`，应收汇总走 `receivable`；各自叠加客户详情和来源业务授权，只读聚合，不把合同、销售或应收事实写回客户域。

## 代码入口

| 定位 | 源码 |
| --- | --- |
| HTTP 路由和分流 | [routes/customer.rs](../../../apps/erp-server/src/http/routes/customer.rs)、[handler/customer/mod.rs](../../../apps/erp-server/src/http/handler/customer/mod.rs) |
| 应用装配、主体事实和审计接入 | [wiring/application.rs](../../../apps/erp-server/src/wiring/application.rs)、[wiring/customer.rs](../../../apps/erp-server/src/wiring/customer.rs)、[wiring/authorization/master.rs](../../../apps/erp-server/src/wiring/authorization/master.rs) |
| 账户查询、范围复核、分配与仓储 | [query.rs](src/service/customer/query.rs)、[service/customer/scope.rs](src/service/customer/scope.rs)、[assignment.rs](src/service/customer/assignment.rs)、[repository/scope.rs](src/repository/scope.rs)、[repository/customer.rs](src/repository/customer.rs) |
| 账户软删除与完整资料命令 | [erp-master-data/src/customer.rs](../../orchestration/erp-master-data/src/customer.rs)、[customer_profile/mod.rs](../../orchestration/erp-master-data/src/customer_profile/mod.rs)、[idempotency.rs](../../orchestration/erp-master-data/src/customer_profile/idempotency.rs) |
| 完整资料详情与客户中心汇总 | [customer_center/profile/mod.rs](../../read-models/erp-read-models/src/customer_center/profile/mod.rs)、[customer_center/service.rs](../../read-models/erp-read-models/src/customer_center/service.rs) |

## 修改执行要求

1. 将无 I/O 的校验、状态迁移和不变式留在实体或值对象；Service 编排单域用例，跨域写入由 Process 管理事务。上层已持有事务时，调用显式接收原 `Executor` 的业务步骤，不要嵌套开启事务。
2. 调整集合查询时同步评估索引，保持授权、筛选、总数与分页口径一致；应用复用公开索引入口并保持现有逐集合注册顺序。
3. HTTP 优先复用本域 DTO；扩展公开合同时同步检查 Process、ReadModel 和应用调用方。
4. 业务改动补充本域库单元测试，覆盖成功、失败、边界及相关幂等或版本冲突路径。

## 验证要求

按 [backend/AGENTS.md](../../../AGENTS.md#验证) 在 `backend/` 执行。仅改文档时只运行 `git diff --check`。代码变更执行[统一质量门禁](../../README.md#质量门禁)，本域测试限库单元测试，不执行集成测试或真实外部服务测试：

```bash
env -u ERP_TEST_MONGO_URI cargo test -p erp-customer --lib --locked
```
