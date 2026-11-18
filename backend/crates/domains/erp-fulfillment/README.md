# erp-fulfillment：收货、交付与客户验收

## 职责

采购和销售订单记录约定，履约领域记录实际发生的数量、结果和业务凭证。这里维护采购入库单及明细、实物发货单及明细（仓发与供应商直发）、电子交付、服务履约、客户验收单及明细，以及验收对各项履约事实的数量分配。发货的收货地址安排、装箱和运单规则也归本域。

本库判断单据能否过账、确认或冲正，以及哪些成功交付还有数量可供验收。它保存本域事实和附件引用，不维护订单修订、库存账、财务账或供应商接口侧的订单与回执；客户验收登记和反向验收的独立命令回执则归本域。

## 目录结构

公开入口见 [src/lib.rs](src/lib.rs)。以采购入库为例，从请求进入后，Service 用实体规则准备单据；读写本域事实时再经 Repository 访问对应集合。

| 目录 | 负责内容与定位入口 |
| --- | --- |
| [`dto/`](src/dto/mod.rs) | 请求、视图和分页契约；例如[入库 DTO](src/dto/purchase_receipt.rs)区分收货明细输入和过账请求。HTTP 优先复用用例提供的 DTO。 |
| [`entity/`](src/entity/mod.rs) | 无 I/O 的实体、值对象和规则；例如[入库实体](src/entity/fulfillment/purchase_receipt.rs)校验数量和状态，[外域事实](src/entity/facts.rs)只声明本域所需的采购、销售、库存预占及凭证字段。 |
| [`service/`](src/service/mod.rs) | `FulfillmentService` 准备草稿、组织本域校验与持久化，例如[入库草稿](src/service/purchase_receipt.rs)和[入库过账步骤](src/service/purchase_receipt_posting.rs)；`FulfillmentReadService` 提供[带授权的本域管理读取](src/service/read.rs)。 |
| [`repository/`](src/repository/mod.rs) | 本域 MongoDB 集合访问器、筛选、分页和写入；例如[表头与明细写入](src/repository/fulfillment/writes.rs)复用调用方的 `persistence_core::Executor`，不自行开启事务。 |
| [`ports/`](src/ports/mod.rs) | 声明本域所需的外部能力；[`ServiceLocationCryptoPort`](src/ports/service_crypto.rs)接收规范化后的现场地点，具体加密由组合层接入。 |
| [`indexes/`](src/indexes/mod.rs) | `indexes::ensure` 是组合根的公开注册入口，依次安装履约业务集合、命令回执和收货地址安排的索引。 |

## 架构设计

### 采购入库：收货事实不是库存账

1. [本域入库步骤](src/service/purchase_receipt_posting.rs)核对草稿状态、期望版本和已冻结的仓库，并读取明细。[入库实体](src/entity/fulfillment/purchase_receipt.rs)要求实收、合格和拒收数量均非负，合格与拒收之和不得超过实收；过账时以此前累计合格量加本行合格与拒收量检查当前采购行上限。
2. [入库过账 Process](../../orchestration/erp-fulfillment-execution/src/fulfillment_execution/purchase_receipt_posting.rs)开启根事务，在事务内核对办理资格并读取[采购领域](../erp-procurement/README.md)的生效版本，把跨版本累计合格收货归并到当前版本行。实际收货不因尚未付款而受阻；只有合格数量交给[库存履约服务](../erp-inventory/src/service/fulfillment.rs)登记入库流水、更新库存余额，并在有采购销售分配时建立销售预占及预占流水。拒收留在收货事实中，不入库存账。
3. 同一 `Executor` 下，本域标记入库已过账，Process 推进任务和采购履约进度，并按销售单与仓库新建或补充仓发草稿。仓发明细只引用库存预占及其数量，预占仍由 [erp-inventory](../erp-inventory/README.md) 管理。后续[发货过账](../../orchestration/erp-fulfillment-execution/src/fulfillment_execution/delivery_posting.rs)中，仓发调用库存服务处理扣减；供应商直发核验采购来源资格，不扣本仓库存。

### 客户验收：履约数量、签收凭证和命令回执各有归属

- **履约数量。** [验收资格](src/service/acceptance_eligibility.rs)按跨版本稳定的销售行组织有效发货、电子交付和服务履约事实，依据正向和反向分配计算净验收量与剩余可验收量。[本域验收过账步骤](src/service/customer_acceptance_posting.rs)要求每行分配合计等于通过数量，且单项履约事实的净验收量不得超过成功履约量。冲正追加反向验收单和引用原分配的 `REVERSE` 记录，并标记原单已冲正，不覆盖原验收明细。
- **业务凭证。** 履约单据记录附件 ID，而不是文件字节或对象存储键。[本域签收凭证规则](src/entity/fulfillment/acceptance_evidence.rs)判断文件类型、大小和治理状态；[验收 Process](../../orchestration/erp-fulfillment-execution/src/fulfillment_execution/customer_acceptance/evidence.rs)在原事务从 [erp-support](../erp-support/README.md) 读取真实文件资产、校验元数据和转授资格，不读取对象存储。现场服务使用独立的[图片凭证规则](src/entity/fulfillment/service_evidence.rs)；[服务确认](src/service/service_fulfillment_confirm.rs)将实际地点规范化后交由加密 Port 处理，持久化密文和查询指纹。外部文件上传不放在数据库事务内。
- **命令回执。** 本域写入验收及分配后，[验收完成步骤](../../orchestration/erp-fulfillment-execution/src/fulfillment_execution/customer_acceptance/completion.rs)沿用同一事务刷新销售履约进度、处理责任任务并写入成功审计事件；登记和反向验收还须在该事务写入命令结果引用，一并提交。成功事件与 `fulfillment_command_receipts` 分开保存；[回执服务](src/service/command_receipt.rs)用调用方的 `Executor` 按原命令身份和载荷查证，不读取审计正文，[回执索引](src/indexes/command_receipt.rs)保证命令 ID 唯一，不设置 TTL。[重放](../../orchestration/erp-fulfillment-execution/src/fulfillment_execution/customer_acceptance/replay.rs)仍需授权：登记恢复原验收单当前视图并核对销售关系，反向验收恢复原反向单 ID；命中回执不再写入。普通过账即使复用完成步骤，也不写回执或获得新增幂等语义。

### 依赖与协作边界

术语与统一分工见[总索引](../../README.md#代码中常见名称的含义)。[Cargo.toml](Cargo.toml) 声明了对 `erp-identity` 公开身份与授权能力的依赖（库测试也使用其测试支持）；其余常规、构建和测试依赖不得指向其他业务领域或组合层。采购版本行、库存预占等外域事实由调用方投影为最小输入；现场地点加密等外部能力由组合层或应用接入本域 Port。

跨域写入由 [erp-fulfillment-execution](../../orchestration/erp-fulfillment-execution/README.md) 的 `fulfillment_execution` 编排。Process 负责授权、顺序和事务，履约、库存、销售、采购、财务等事实仍由各自领域写入。例如，电子交付或服务确认触发的[实际成本](../../orchestration/erp-fulfillment-execution/src/fulfillment_execution/fulfillment_actual_cost.rs)由财务服务沿用确认事务写入；供应商接口侧订单与回执归 [erp-supply](../erp-supply/README.md)。组合多个领域的履约查询见[履约中心 ReadModel](../../read-models/erp-read-models/src/fulfillment_center/mod.rs)。

## 修改执行要求

1. 无 I/O 的校验、状态迁移和不变式放在实体或值对象中；Service 组织本域用例，生产跨域事务由 Process 控制，Repository 不替调用方决定提交或回滚。
2. 新增或调整集合查询时同步评估索引；组合根复用本域公开索引入口并保持既有逐集合注册顺序。
3. 扩展公开合同须检查 Process、ReadModel 和应用调用方；业务改动补充本域库单元测试，覆盖成功、失败、边界及相关幂等或版本冲突路径。

## 验证要求

仅改文档时只运行 `git diff --check`。修改业务代码时，在 `backend/` 运行本库单元测试，并按[统一质量门禁](../../README.md#质量门禁)执行适用检查：

```bash
env -u ERP_TEST_MONGO_URI cargo test -p erp-fulfillment --lib --locked
```

测试范围限定为库单元测试，不执行集成测试或真实外部服务测试。
