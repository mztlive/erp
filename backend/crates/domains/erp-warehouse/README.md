# erp-warehouse：仓库资料与 SKU 策略

## 职责

维护仓库的稳定身份、可追溯的资料修订、入库和仓发经办配置，以及按仓库和 SKU 设置的最低可用量预警阈值。本库提供配置和仓库身份，不记库存数量，也不登记实际收发货。

### 负责的数据与能力

| MongoDB 集合 | 保存内容 |
| --- | --- |
| `warehouses` | 稳定仓库 ID、创建后不可修改的唯一仓库代码、启停状态、当前修订指针，以及分别负责入库和仓发的账号 ID。 |
| `warehouse_revisions` | 按仓库递增的只追加修订，记录名称、行政区划末级代码、地址、联系人、生效区间和变更原因。新修订更新当前指针，不改写历史修订或其他单据已冻结的业务快照。 |
| `warehouse_sku_policies` | 按仓库 ID 和 SKU ID 保存非负的定点预警阈值、启停状态和生效区间。策略不是库存事实，不写现存量、预占量或商城库存缓存。 |

### 使用场景

- 新建仓库、修改资料或启停状态、查询修订历史；按仓库代码或当前修订名称查找仓库，或在独立目录回显已选仓库。
- 单独更换入库与仓发经办人；创建、查询、更新或软删除仓库 SKU 预警策略。

### 协作示例

采购收货时，[erp-fulfillment](../erp-fulfillment/README.md) 记录实际收货单及验收数量，[erp-inventory](../erp-inventory/README.md) 维护余额、流水、预占与调整；跨域过账由 [erp-fulfillment-execution](../../orchestration/erp-fulfillment-execution/README.md) 编排。若只是换入库经办人或调整预警阈值，则改本库配置，不调整库存或改写履约单据。履约编排在[生成责任任务](../../orchestration/erp-fulfillment-execution/src/fulfillment_execution/task.rs)时按入库或仓发职责取经办人；仓库停用或未配置对应经办人时拒绝建任务，不回退给创建人或责任池。更换经办人只影响后续创建的任务，不改派已开放任务。

## 目录结构

### 代码入口

| 入口 | 用途 |
| --- | --- |
| [Cargo.toml](Cargo.toml)、[src/lib.rs](src/lib.rs) | 依赖声明与公开入口，导出实体、DTO、仓储、端口和 `WarehouseService`。 |
| [src/entity/mod.rs](src/entity/mod.rs) | 实体和值对象入口；[仓库身份](src/entity/warehouse/warehouse_entity.rs)、[资料修订](src/entity/warehouse/warehouse_revision.rs)、[SKU 策略](src/entity/warehouse/warehouse_sku_policy.rs)各自约束状态、字段和有效期。 |
| [src/dto/warehouse.rs](src/dto/warehouse.rs) | 写入请求、列表参数与视图，定义分页和排序字段白名单。 |
| [src/service/warehouse/mod.rs](src/service/warehouse/mod.rs) | `WarehouseService` 的本域事务入口；[commands.rs](src/service/warehouse/commands.rs) 组织写入，[reads.rs](src/service/warehouse/reads.rs) 校验读取资格。 |
| [src/service/directory.rs](src/service/directory.rs)、[src/authorization.rs](src/authorization.rs) | 独立目录与仓库业务列表入口；提供给仓库和库存共用的实际仓库身份授权、Facts 和 Mapping。 |
| [src/repository/mod.rs](src/repository/mod.rs) | MongoDB 集合访问器与仓储入口；[warehouse.rs](src/repository/warehouse.rs) 封装修订及策略查询，[directory.rs](src/repository/directory.rs) 执行目录和仓库列表的完整授权聚合。 |
| [src/ports/mod.rs](src/ports/mod.rs)、[src/indexes/mod.rs](src/indexes/mod.rs) | 经办人身份、附件指纹与审计合同；公开索引注册入口 `indexes::ensure`。 |
| [src/error.rs](src/error.rs) | 本域错误分类和下层错误转换。 |

## 架构设计

### 仓库与策略写入

- **创建和修订仓库。** `WarehouseService` 在同一事务内完成操作授权、经办资格检查、仓库与首修订写入及成功审计。更新资料时读取仓库并核对期望版本，再追加修订、更新当前指针，仓库代码不变。仅更新经办人时修改稳定记录并写审计，不追加资料修订；两个账号均须可登录且具备对应履约执行资格。经办选项在 `warehouse:update` 资格校验后读取，只返回至少具备一项对应资格的可登录管理端账号。
- **修订有效期和敏感字段。** 修订区间包含开始日、不含结束日，未填结束日表示无限期。公开更新用例将新修订立即设为当前修订；现有写入校验单条区间并以索引拒绝同仓库相同开始日，不做不同开始日的完整跨修订重叠检查。地址与联系人通过 `AttachmentFingerprintPort` 生成带密钥的 HMAC-SHA256 指纹，不用裸摘要；修订列表不返回敏感值或指纹。当前服务仍以明文占位写入敏感值列，未接入生产级加密。
- **创建预警策略。** 新策略需要跨域核对 SKU，因此走 [erp-master-data](../../orchestration/erp-master-data/README.md) 的 [`create_warehouse_sku_policy`](../../orchestration/erp-master-data/src/warehouse.rs)，而非 `WarehouseService`。该命令在同一事务内检查 `warehouse_sku_policy:create`、仓库和 [erp-catalog](../erp-catalog/README.md) 的 SKU 是否存在，再读取同仓同 SKU 的全部未删除策略，复用本域实体检查启用区间重叠，写入策略和成功审计。`erp-master-data` 只拥有编排，实体、集合和索引仍归本库。
- **更新与删除策略。** 本域服务按期望版本更新阈值或启停状态，不改仓库、SKU 或生效日期；软删除使用读到的记录版本。策略生效区间同样左闭右开，相邻区间不冲突，只有同仓同 SKU、双方启用且区间重叠才被实体拒绝。[唯一索引](src/indexes/warehouse.rs)另行禁止同仓同 SKU 的相同开始日，不能替代事务内的完整启用区间检查。更新、软删除均与成功审计同事务提交。

### 读取与授权

- **资格检查和对象范围分开。** [权限定义](../../foundation/erp-permissions/src/warehouse.rs)归 `erp-permissions`，[授权判定](../erp-identity/README.md#业务授权-api)归共享 `AuthorizationService`。仓库创建、更新、经办人配置与候选读取、修订列表、策略读写使用 `AccessTarget::Operation` 校验动作资格。修订列表使用 `warehouse_revision:list`，按仓库或名称筛选；策略列表使用 `warehouse_sku_policy:list`，按仓库、SKU 或状态筛选。两者的 `warehouse_id` 都是业务筛选，不是仓库对象范围授权。
- **仓库列表和独立目录。** 两者使用 `warehouse:list`，由 `WarehouseAuthorization` 让 identity 编译完整对象条件，再在真实的未删除仓库上执行。仓储先执行完整授权，再执行分页或目录上限；业务列表分页和计数共用同一条件，关键词匹配代码或当前修订名称。目录只投影 ID、代码、名称和状态；已选 ID 只能收窄授权范围。仓库列表从第二页起须携带 `scope_version`，缺失或范围变化返回 `DATA_SCOPE_CHANGED`。
- **库存使用仓库身份授权。** [应用接线](../../../apps/erp-server/src/wiring/inventory.rs)为库存动作绑定对应的仓库权限。库存查询可用 `has_any` 判断是否存在获准的真实仓库；`ids_in` 仅校验当页候选，供按钮等功能收窄；对象命令在原业务事务以 `require` 检查实际仓库。目录与库存调用均须沿用原读取或写入 `Executor`，按授权入口在取数前后核对读取边界、在交付查询结果前复验凭证。不要预载账号全部授权仓库 ID，不得跨请求缓存返回的 ID 或将其视为后续命令授权。

### 依赖与协作边界

术语与统一分工见[总索引](../../README.md#代码中常见名称的含义)。本库可直接使用 `erp-identity` 的公开接口；其余外域能力通过 Port 或调用方传入的事实接入，常规、构建和测试依赖均不得直接指向其他业务领域或组合层。应用[装配经办身份、指纹与审计端口](../../../apps/erp-server/src/wiring/warehouse.rs)，并[登记仓库授权事实与映射](../../../apps/erp-server/src/wiring/authorization/master.rs)。本域停用仓库的写入尚未接入库存与有效预占检查，不得把启停状态更新视为已经验证无库存。

## 修改执行要求

1. 无 I/O 的校验、状态迁移和不变式放在实体或值对象；由 Service 组织本域用例，由 Process 组织跨域写入。Repository、Port 及业务步骤沿用调用方的 `persistence_core::Executor`，不自行开启事务。
2. 查询变更同步评估索引；组合根复用本域公开索引入口，保持仓库、修订、策略的逐集合注册顺序。
3. HTTP 请求和响应优先复用本域 DTO；扩展公开合同时同步检查 Process、ReadModel 和应用调用方。
4. 业务代码改动补充本域库单元测试，覆盖成功、失败、边界及适用的幂等或版本冲突路径。

## 验证要求

仅修改文档时，只对改动文件执行 `git diff --check`。业务代码验证按[后端开发规则](../../../AGENTS.md#验证)和[统一质量门禁](../../README.md#质量门禁)执行。本库单元测试命令在 `backend/` 运行：

```bash
env -u ERP_TEST_MONGO_URI cargo test -p erp-warehouse --lib --locked
```

不执行集成测试、忽略测试或依赖真实 MongoDB 和外部服务的测试。
