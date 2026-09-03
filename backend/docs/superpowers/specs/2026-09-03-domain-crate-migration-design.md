# ERP 后端领域 Crate 迁移设计契约

## 1. 文档信息

| 项目 | 内容 |
| --- | --- |
| 状态 | 已批准 |
| 生效日期 | 2026-09-03 |
| 适用范围 | `backend` Rust workspace |
| 迁移模式 | 上线前硬切迁移 |
| 业务行为 | 必须保持不变 |
| 兼容层 | 禁止 |
| 执行计划目录 | `backend/docs/superpowers/plans/domain-crate-migration/` |

本文档是 ERP 后端从水平三层业务 crate 迁移到领域 crate 的架构约束源。后续阶段计划、代码实现、测试、评审和验收不得与本文档冲突。

本文档中的“必须”“禁止”“允许”具有约束力：

- “必须”表示阶段验收的必要条件。
- “禁止”表示不得通过例外、临时转发或白名单绕过。
- “允许”仅表示可采用，不代表必须采用。

## 2. 迁移目标

当前集中在 `entities`、`database`、`services` 中的业务代码必须按限界上下文迁入独立 crate。每个领域 crate 必须拥有本领域所需的实体、值对象、应用服务、Repository、Port、DTO 和错误合同。

迁移完成后必须达到以下结果：

1. 修改单一领域代码时，不再重新编译一个包含全部业务服务的单体 `services` crate。
2. 领域事实、业务规则和持久化能力具有唯一归属。
3. 跨领域写入由显式流程组合层持有事务并编排。
4. 跨领域查询由显式读模型组合层组装。
5. `entities`、`database`、`services` 三个旧业务 crate 从 workspace 和文件系统中删除。
6. HTTP、DTO、错误、MongoDB、权限、幂等、事务和业务结果与迁移前保持一致。

## 3. 不在本次迁移范围内的事项

本次迁移禁止夹带以下变更：

- 新增、删除或改变业务功能。
- 调整业务规则、审批规则或状态机语义。
- 修改 HTTP 路径、HTTP 方法、请求或响应语义。
- 修改业务错误码、HTTP 状态码或错误分类。
- 修改 MongoDB collection 名、字段名、字段类型、BSON 表达或索引语义。
- 修改金额、数量、税率、时间或舍入规则。
- 修改幂等键、命令指纹、命令回执或重放语义。
- 修改 RBAC 权限标识、权限生成规则或数据范围语义。
- 引入事件总线、消息队列、微服务拆分或分布式事务。
- 为领域 crate 再机械拆分 `entity`、`service`、`repository` 子 crate。
- 保留旧三层兼容 façade、转发模块、双实现或双写。

发现上述需求时，当前阶段必须停止并转为“阻塞”。相关变更必须建立独立设计和验收合同。

## 4. 目标架构

### 4.1 依赖层级

目标依赖方向固定如下：

```text
apps/web-api       apps/cli
      |               |
      +-------+-------+
              |
              +-------> erp-processes / erp-read-models
              |                       |
              +-----------------------+
                                      v
                           bounded-context crates
                                      |
                                      v
                 erp-core / application-core / persistence-core

erp-workflow ------> bpm
id-generator ------> persistence-core
foundations -------> entity-core / entity-macros（按需）
```

现有 `config`、`storage`、`permission-macros`、`test-support` 按其既有职责保留。`bpm` 必须继续保持纯流程领域和状态引擎，不得依赖 ERP 业务、MongoDB、Axum、配置、ID 生成器或外部 I/O。

### 4.2 基础 crate

| Crate | 唯一职责 | 禁止内容 |
| --- | --- | --- |
| `erp-core` | 金额、数量、比率、业务时间、稳定共享 ID、最小通用校验原语 | 具体业务流程、Repository、HTTP、MongoDB |
| `application-core` | 分页、查询合同、应用错误分类、调用上下文和用例支撑 | 领域规则、数据库实现、HTTP Handler |
| `persistence-core` | MongoDB 连接、`Executor`、`NoTransaction`、`Transactional`、通用 Repository 和 Mongo 操作 | 领域事实、业务授权、跨域编排 |

`id-generator` 必须改为依赖 `persistence-core`，不得继续依赖旧 `database`。

只有满足以下全部条件的类型才允许进入 `erp-core`：

1. 至少被两个限界上下文以相同语义使用。
2. 不属于任一具体业务上下文。
3. 不会因为单一领域需求频繁变化。
4. 不依赖数据库、HTTP、运行时或外部系统。

### 4.3 领域 crate 清单

| Crate | 领域范围 |
| --- | --- |
| `erp-identity` | Account、Auth、Role、RBAC、IAM、D06 Access Control、Casbin 业务适配 |
| `erp-audit` | 审计日志、审计写入、通用命令执行审计事实 |
| `erp-workflow` | D02 Document Registry、D03 WorkItem、Approval 核心与 Approval Integration 领域部分 |
| `erp-support` | D01 Source Registry、D04 Bulk Job、D05 File Asset |
| `erp-party` | D07 Party |
| `erp-customer` | D08 Customer，不包含客户中心跨域读模型 |
| `erp-supplier` | D09 Supplier |
| `erp-catalog` | D10 Catalog |
| `erp-warehouse` | D11 Warehouse |
| `erp-contract` | D12 Contract |
| `erp-sales` | D13 Sales Order、D14 Sales Review |
| `erp-procurement` | D15 Purchase Order、Procurement Responsibility |
| `erp-fulfillment` | D16 Fulfillment |
| `erp-inventory` | D17 Inventory |
| `erp-finance` | D18 Receivable、D19 Payable、D20 Cost |
| `erp-returns` | D21 Returns |
| `erp-import` | D22 Import |
| `erp-integration` | D23 Mall Sync、D27 Projection、D34 Integration Operations |
| `erp-commerce` | D28 Card Instance、D29 Mall Order、D30 Mall After Sales、D31 Mall Backfill |
| `erp-supply` | D24 Supplier Offering、D25 Supplier API、D26 Publication、D32 Supplier Fulfillment、D33 Supplier Settlement |

### 4.4 组合 crate

`erp-processes` 必须持有跨领域命令、跨领域事务、审批动作分发和需要多个领域共同完成的业务流程。

`erp-read-models` 必须持有跨领域查询、工作台、客户中心、业务摘要和统计视图。

组合 crate 不得成为新的通用收纳层：

- `erp-processes` 中每个模块必须对应一个可命名的业务流程。
- `erp-read-models` 中每个模块必须对应一个明确的消费方视图。
- 单领域规则不得放入组合 crate。
- MongoDB 原始查询只能存在于明确的读模型 Repository 或领域 Repository 中，不得散落在流程编排代码中。

## 5. 领域 crate 内部合同

每个领域 crate 使用以下逻辑职责：

```text
src/
├── lib.rs
├── entity/
├── service/
├── repository/
├── ports/
├── dto/
└── error.rs
```

只有存在实际内容时才创建相应文件或目录。禁止创建只有 re-export 或转发调用的空模块。

### 5.1 `entity`

`entity` 必须承载实体、值对象、不变式、规范化、确定性计算和状态转换。

`entity` 禁止依赖：

- MongoDB、BSON 或数据库执行器。
- Axum、HTTP DTO 或 Handler。
- Repository 和 Service。
- 其他业务领域的 Repository、Service 或完整聚合。

### 5.2 `service`

`service` 必须承载单领域用例、事务意图、Repository 调用、依赖外部事实的判断和本领域多步骤一致性。

`service` 禁止：

- 直接拼装 BSON 或访问 MongoDB collection。
- 直接调用其他领域的 Repository 或 Service。
- 承载可独立测试的实体不变式。
- 在已有调用方事务时自行开始或提交第二个事务。

### 5.3 `repository`

`repository` 必须承载本领域数据读写、存储映射、索引定义和查询实现。执行数据访问的 Repository 方法必须按现有合同接收 `&mut dyn Executor`，不得自行管理业务事务。

Repository 禁止：

- 决定业务权限和可操作性。
- 编排跨领域用例。
- 把 MongoDB 原始错误直接暴露为 HTTP 错误。
- 访问其他领域的 collection。

### 5.4 `ports`

Port 必须由能力消费方定义，并且只暴露当前用例所需的最小事实或命令。

Port 禁止返回：

- 其他领域完整实体或聚合。
- 其他领域的 Repository 类型。
- 其他领域内部 Service DTO。
- MongoDB Document、Bson 或 collection 句柄。

### 5.5 `dto`

DTO 必须服务公开应用用例或协议适配。字段完全同构且没有安全或协议差异时，应继续复用现有语义合同。

DTO 不得成为第二套持久化模型。HTTP 形态确实不同的薄包装必须提供显式 `From`、`TryFrom` 或映射函数。

### 5.6 可见性

- 默认使用私有可见性。
- 领域内部共享优先使用 `pub(super)` 或 `pub(crate)`。
- 只有稳定跨 crate 合同允许 `pub`。
- `lib.rs` 只导出调用方真正需要的用例、命令、结果、稳定事实、ID 和错误类型。
- 禁止通过通配 re-export 暴露领域内部模块。

## 6. 目标依赖规则

### 6.1 业务领域之间

普通业务领域 crate 默认禁止直接依赖其他普通业务领域 crate。跨域依赖必须转换为以下形式之一：

1. 消费方定义的查询 Port。
2. 消费方所需的稳定事实 DTO。
3. `erp-processes` 中的跨领域命令编排。
4. `erp-read-models` 中的跨领域查询组装。

确实属于共享内核的值类型必须满足第 4.2 节的全部条件后才能进入 `erp-core`，不得仅为消除编译错误而上移。

### 6.2 允许依赖

- 领域 crate 可以依赖基础 crate、必要的过程宏和实际使用的基础设施库。
- `erp-workflow` 可以依赖 `bpm`，但 `bpm` 不得反向依赖 `erp-workflow`。
- `erp-processes` 可以依赖多个领域 crate。
- `erp-read-models` 可以依赖多个领域公开查询合同和读模型 Repository。
- `web-api`、`cli` 可以依赖单领域公开 API、`erp-processes` 和 `erp-read-models`。
- `web-api` 和 `cli` 之间禁止互相依赖。

### 6.3 迁移期依赖

迁移期间必须满足：

- 已迁移的新领域 crate 禁止依赖旧 `entities`、`database`、`services`。
- 尚未迁移的旧 crate 允许依赖已经迁移的新领域 crate。
- `web-api`、`cli`、`erp-processes`、`erp-read-models` 允许暂时同时依赖新旧代码。
- 一个模块迁入新领域 crate 后，旧实现必须在同一阶段删除。
- 禁止保留旧路径转发、类型别名兼容、feature 双路由或重复事实来源。

## 7. 调用与数据流合同

### 7.1 单领域命令

单领域命令必须由领域 Service 执行。作为独立用例执行、没有调用方事务且只涉及单集合原子操作时，必须使用 `NoTransaction`；涉及本领域多集合原子性时，由该领域 Service 持有事务。单领域命令被跨领域 Process 纳入既有事务时，必须复用 Process 传入的 `Executor`。

```text
Handler/CLI
    -> Domain Service
        -> Domain Repository
            -> MongoDB
```

### 7.2 跨领域读取

跨领域读取必须由消费方定义窄 Port，或由 `erp-read-models` 组装。

```text
Consumer
    -> Consumer-owned Port
        -> Adapter / Read Model
            -> Provider facts
```

返回值必须是稳定事实，不得返回外部领域完整聚合。事务中的读取必须复用调用方传入的 `Executor`。

### 7.3 跨领域原子命令

跨领域原子命令必须由 `erp-processes` 持有事务并依次调用各领域的事务内命令接口。

```text
Handler
    -> Process
        -> Transactional::with_transaction
            -> Domain A apply_in_transaction(..., executor)
            -> Domain B apply_in_transaction(..., executor)
            -> Workflow apply_in_transaction(..., executor)
            -> Audit apply_in_transaction(..., executor)
```

事务内领域接口必须满足：

- 不开始、提交或终止事务。
- 不执行外部 HTTP、S3、消息或其他不可控 I/O。
- 不访问其他领域 Repository。
- 在当前事务快照中重新验证本领域不变式。
- 返回稳定、可分类的领域错误。

### 7.4 外部系统调用

外部调用固定采用以下顺序：

```text
事务 A：持久化意图、inbox/outbox、审计或任务
    -> 提交
事务外：调用外部系统
    -> 获得成功或失败结果
事务 B：持久化结果、错误任务、状态和审计
    -> 提交
```

禁止在 MongoDB 事务或 session 持有期间执行外部网络、S3 或第三方调用。

## 8. 核心流程归属

### 8.1 `order_to_cash`

`erp-processes::order_to_cash` 必须编排销售正式化、应收创建、财务工作项、审计和命令回执。全部数据库写入必须复用同一 `Executor`。

迁移前的调用顺序、失败点、回滚结果和幂等重放结果必须通过特征测试固化并保持不变。

### 8.2 财务入账与销售进度

财务领域必须负责财务规则和财务事实。销售进度、Workflow 和 Audit 更新必须由 Process 在同一事务中调用，不得由 `erp-finance` 直接调用 `erp-sales`。

### 8.3 `procure_to_pay`

`erp-processes::procure_to_pay` 必须编排采购正式化、应付、成本、Workflow、Audit 和命令回执。

### 8.4 `fulfillment_execution`

`erp-processes::fulfillment_execution` 必须编排供货分配、库存变化、采购关联、履约状态、销售进度和 WorkItem 推进。

### 8.5 `reverse_flow`

`erp-processes::reverse_flow` 必须编排退货、销售冲销、财务冲销、采购回退、库存恢复、履约回退和 WorkItem。原正向操作识别、反向幂等和禁止重复反转的业务语义必须保持不变。

### 8.6 Approval

`erp-workflow` 必须拥有审批核心状态与规则。业务动作分发必须迁入 `erp-processes::approval_dispatch`，并通过 `ApprovalDomainActionPort` 或等价窄合同调用具体业务动作。

审批动作必须接收当前事务的 `Executor`，不得在分发器或业务适配器中另开事务。

### 8.7 WorkItem

WorkItem 实体、Repository、状态转换和单域写入属于 `erp-workflow`。跨领域 brief、工作台、客户中心和统计属于 `erp-read-models`。

同一事务中必须先完成业务命令，再推进 WorkItem。业务命令失败时，WorkItem 不得成功推进。

## 9. 错误、幂等和数据合同

### 9.1 错误

- 各领域拥有本领域稳定的 `DomainError` 或等价错误类型。
- Process 必须保留错误类别，不得把领域冲突转换为未分类内部错误。
- HTTP 入口必须保持现有状态码、错误码和 `ApiResponse` 语义。
- 数据库和外部系统原始错误不得直接返回调用方。
- 禁止依赖错误字符串包含关系判断类别。

### 9.2 幂等

- 幂等键、规范化规则、指纹算法、唯一索引、回执内容和重放结果必须保持不变。
- 单领域命令的回执由领域 Service 持有。
- 跨领域命令的根回执由 `erp-processes` 持有。
- 唯一键竞争后不得复用已经失败的 MongoDB session。

### 9.3 MongoDB

- collection、字段、序列化形式和索引必须保持一致。
- Repository 迁移只能改变代码归属，不得改变数据归属或数据形态。
- 索引初始化可以迁移到领域 crate，但生成的索引集合必须与基线一致。
- 本次迁移不得要求数据回填脚本。

### 9.4 HTTP 和权限

- Handler 必须继续复用现有应用 DTO，或使用具有明确协议差异的薄适配 DTO。
- 路由、权限宏、权限名称和生成物必须保持一致。
- 管理路由必须继续经过 JWT、RBAC 和现有数据范围控制。

## 10. 阶段执行合同

迁移采用依赖锥顺序硬切，共 18 个阶段。每个阶段对应一份执行文档和一个验收边界。

| 阶段 | 名称 | 主要结果 | 风险 |
| --- | --- | --- | --- |
| 00 | 基线与执行治理 | 行为、数据、依赖和编译基线；领域边界检查 | 低 |
| 01 | 公共基础 crate | `erp-core`、`application-core`、`persistence-core` | 高 |
| 02 | 身份与审计 | `erp-identity`、`erp-audit` | 中 |
| 03 | 工作流与组合层 | `erp-workflow`、`erp-processes`、`erp-read-models` | 高 |
| 04 | 通用支撑 | `erp-support` | 中 |
| 05 | 主体、客户、供应商 | `erp-party`、`erp-customer`、`erp-supplier` | 中 |
| 06 | 商品、仓库、合同 | `erp-catalog`、`erp-warehouse`、`erp-contract` | 中 |
| 07 | 导入任务 | `erp-import` | 中 |
| 08 | 库存 | `erp-inventory` | 高 |
| 09 | 财务 | `erp-finance` | 极高 |
| 10 | 销售 | `erp-sales`，完成销售与财务解环 | 极高 |
| 11 | 采购 | `erp-procurement`，完成采购与财务解环 | 极高 |
| 12 | 履约 | `erp-fulfillment` | 极高 |
| 13 | 退货与逆向流程 | `erp-returns` | 极高 |
| 14 | 外部集成与投影 | `erp-integration` | 高 |
| 15 | 商城业务 | `erp-commerce` | 高 |
| 16 | 供应链协同 | `erp-supply` | 极高 |
| 17 | 最终切换 | 删除旧三层 crate，完成全量与编译收益验收 | 高 |

执行规则：

- 阶段内部允许暂时无法编译。
- 阶段结束时必须恢复整个 workspace 可编译并通过规定门禁。
- 一个阶段未验收时禁止开始下一阶段。
- 已迁移源代码必须在同一阶段从旧 crate 删除。
- 每个阶段必须形成明确完成提交。
- 不得跨阶段混合无关模块迁移。

## 11. 阶段文档合同

### 11.1 文件集合

执行计划必须写入：

```text
backend/docs/superpowers/plans/domain-crate-migration/
```

该目录必须包含 `README.md` 和 `00` 至 `17` 共 18 份阶段文档。

`README.md` 只能包含目标、不可变合同、阶段顺序、状态、公共门禁和阶段链接。禁止写入方案讨论和迁移复盘。

### 11.2 固定章节

每份阶段文档必须包含：

1. 阶段元数据。
2. 阶段目标。
3. 前置条件。
4. 不可变业务合同。
5. 范围内与范围外事项。
6. 源路径和符号到目标路径和符号的映射。
7. 目标依赖合同。
8. 按顺序执行的任务清单。
9. 阶段内编译中断规则。
10. 测试和质量门禁。
11. MongoDB 副本集验收。
12. 暂停条件。
13. 回退步骤。
14. 完成判据。
15. 结构化验收证据表。

正式阶段文档必须精确到文件、主要类型、Trait、公开方法、调用方和删除目标。禁止只写目录级迁移指令。

### 11.3 状态

阶段状态只允许：

```text
未开始
执行中
本地门禁通过
等待副本集验收
已验收
阻塞
```

只有前一阶段为“已验收”，后一阶段才允许进入“执行中”。

### 11.4 任务顺序

每个迁移单元必须按以下顺序执行：

```text
特征测试
-> 目标模块与公开合同
-> Entity/值对象
-> Repository
-> Service
-> Process/Read Model
-> Handler/CLI
-> 删除旧实现
-> 边界检查
-> 全量门禁
```

每项任务必须包含精确文件、符号、测试、调用方、删除项和验证命令。

## 12. 自动边界检查

阶段 00 必须新增 `scripts/check-domain-boundaries.sh`，至少检查：

- 新领域 crate 是否依赖旧三层 crate。
- 普通领域 crate 之间是否存在直接依赖。
- `entity` 是否引用 MongoDB、Axum、Repository 或 Service。
- `service` 是否引用 BSON、MongoDB collection 或原始数据库操作。
- 是否存在跨领域 Repository 或 Service 导入。
- 是否存在旧路径转发、重复类型或重复实现。
- 已迁移模块是否仍残留在旧三层 crate。
- 领域 `lib.rs` 是否通过宽泛 re-export 暴露内部实现。

现有边界检查必须按以下方式演进：

- `check-service-boundaries.sh` 在旧 `services` 存在期间继续执行。
- 阶段 03 更新 `check-bpm-boundaries.sh` 中对旧三层路径和映射文件的硬编码，但不得降低 BPM 纯度约束。
- 阶段 17 只有在新领域边界检查覆盖同等约束后，才允许删除 `check-service-boundaries.sh`。
- `check-permissions-drift.sh` 在全部阶段保留。

禁止通过增加忽略路径、扩大技术债基线或关闭失败规则使阶段通过。

## 13. 质量门禁

### 13.1 每阶段完整门禁

每个阶段结束时必须执行：

```bash
cargo fmt --all -- --check
cargo check --workspace
cargo clippy --workspace --all-targets --all-features -- -D warnings
cargo test --workspace
./scripts/check-bpm-boundaries.sh
./scripts/check-service-boundaries.sh
./scripts/check-domain-boundaries.sh
./scripts/check-permissions-drift.sh
```

旧 `services` 删除后，`check-service-boundaries.sh` 才允许从命令集中移除。

阶段内允许先执行目标 crate 和直接调用方的窄检查，但窄检查不得替代阶段结束门禁。未执行的命令不得记录为通过，已知失败不得通过文档豁免。

### 13.2 MongoDB 副本集门禁

涉及以下任一事项的阶段必须执行副本集测试：

- Repository 迁移。
- 事务调用方迁移。
- 跨集合写入。
- 幂等回执。
- WorkItem 或 Approval 推进。
- 唯一索引或并发控制。
- 财务、库存、履约、退货或结算。

执行合同：

```bash
export ERP_TEST_MONGO_URI='mongodb://127.0.0.1:27017/?replicaSet=rs0'
cargo test --workspace -- --include-ignored
```

没有可用副本集环境时，阶段最多进入“等待副本集验收”，不得标记为“已验收”。

### 13.3 行为验收

受影响流程必须覆盖：

- 成功结果等价。
- 关键步骤失败时整笔回滚。
- 幂等重放返回相同结果。
- 版本冲突和唯一键竞争保持相同分类。
- 同一原子流程复用同一 MongoDB session。
- 外部 I/O 不发生在事务中。
- HTTP、DTO、错误和权限黄金结果不变。

## 14. 阶段暂停条件

出现以下任一情况必须停止当前阶段并标记“阻塞”：

- 特征测试证明迁移前后行为不一致。
- 需要修改 HTTP、DTO、错误码或权限语义。
- 需要修改 MongoDB 字段、collection 或索引语义。
- 需要引入普通领域 crate 之间的直接依赖。
- 同一事务无法继续复用同一个 `Executor`。
- 外部 I/O 只能放在事务内才能工作。
- 幂等键、命令回执或失败重放结果发生变化。
- 必须保留双实现或兼容 façade 才能完成阶段。
- 当前工作区存在与阶段文件重叠且来源不明确的未提交修改。
- 原有测试不足以证明行为，需要先补充特征测试。

阻塞阶段禁止通过扩展 allowlist、跳过测试、降低 lint 或修改基线继续推进。

## 15. 回退合同

本次迁移不提供运行时双版本回退。代码回退单位必须是完整阶段：

- 回退到上一阶段已验收提交。
- 已提交阶段使用 `git revert` 回退。
- 尚未合并阶段应废弃专用 worktree 或分支，并保留必要失败证据。
- 禁止只回退 Entity、Repository 或 Service 的一部分。
- 禁止为了回退保留新旧两套事实来源。
- 回退后必须重新执行上一阶段完整门禁。

本次迁移原则上不得产生数据迁移。任何数据转换需求都必须使当前阶段停止，并单独建立备份、迁移、验证和恢复合同。

## 16. 验收证据合同

每份阶段文档必须包含结构化证据，不得以叙述性复盘替代：

| 证据 | 必须记录的内容 |
| --- | --- |
| 变更范围 | 新增、移动、修改、删除文件清单 |
| 旧引用清零 | `rg` 命令与结果 |
| Cargo 依赖 | `cargo tree` 或 metadata 结果 |
| 边界检查 | 命令与退出状态 |
| 单元和集成测试 | 命令与通过结果 |
| Workspace 门禁 | 每条命令与退出状态 |
| 副本集测试 | 脱敏环境说明、命令与结果 |
| API 契约 | 路由、DTO、错误和权限结果 |
| 数据契约 | collection、字段和索引对比结果 |
| 阶段提交 | commit hash 和提交标题 |

## 17. 增量编译验收

阶段 00 必须建立固定测量脚本，阶段 17 必须使用相同脚本和环境复测。

测量必须覆盖：

1. 一个低耦合领域，例如 Customer。
2. 一个核心交易领域，例如 Sales。
3. 一个高耦合领域，例如 Finance。

每个场景必须执行一次预热和五次有效测量，以中位数作为结果。每个被比较的代码版本使用独立、预热后的 `CARGO_TARGET_DIR`。

必须记录：

- `cargo check -p web-api` 总耗时。
- 实际发生 rustc 编译的 crate 清单。
- 被判定为 Fresh 的 crate 清单。
- 修改前后的反向依赖闭包。
- Rust toolchain、profile、feature、硬件和后台负载说明。

最终必须同时满足：

- 修改一个领域时，不重新编译无依赖关系的其他领域 crate。
- Sales 修改不重新编译 Finance。
- Finance 修改不重新编译 Sales。
- 三个场景中至少两个的增量 `cargo check` 中位耗时降低不低于 30%。
- 任一场景不得回退超过 10%。
- 公共基础 crate 变更导致的大范围重编译不作为领域隔离失败。

结构隔离通过但耗时指标未通过时，阶段 17 不得完成。必须继续检查过程宏展开、泛型单态化、Build Script、Feature 传播和应用入口依赖。

## 18. 最终完成条件

只有同时满足以下条件，本次迁移才允许标记完成：

1. 20 个目标领域 crate、3 个基础 crate、2 个组合 crate 均按合同存在。
2. `entities`、`database`、`services` 已从 workspace 和文件系统删除。
3. Cargo metadata、源码、脚本、CI 和文档中不存在有效旧 crate 引用。
4. 普通业务领域 crate 之间不存在直接依赖。
5. 跨领域命令全部由 `erp-processes` 编排。
6. 跨领域查询全部由 `erp-read-models` 组装。
7. HTTP、DTO、错误、MongoDB、权限、幂等和事务合同保持不变。
8. 全部本地质量门禁通过。
9. 全部副本集集成测试通过。
10. 增量编译结构和量化指标通过。
11. 18 份阶段文档均包含完整验收证据和阶段提交。

未满足任何一项时，不得以“主体迁移完成”“剩余清理后续处理”或等价表述宣布完成。
