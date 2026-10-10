# erp-finance：应收、应付与成本

记录客户欠公司多少钱、公司欠供应商多少钱、实际收付了多少，以及发票、成本和分摊结果。

订单表达交易约定，财务记录表达应收应付和实际资金、成本事实。金额、核销和分摊规则在本领域维护，供销售、采购和履约流程共同使用。

## 使用场景

- 修改应收、应付、回款、付款、发票及其分配关系。
- 调整成本条目、成本分摊或本域财务查询。

## 协作示例

登记客户回款并分配到应收项目时，本 crate 维护回款和分配事实；涉及业务单据、审批和审计的完整过账由 [erp-processes](../erp-processes/README.md) 协调，经营盈亏等跨域报表由 [erp-read-models](../erp-read-models/README.md) 组装。

## 负责的数据与能力

- 应收、客户回款及销项发票事实。
- 应付、供应商付款及进项发票事实。
- 成本条目、分摊及相关金额规则和本域持久化。
- 财务命令的独立结构化回执、强类型结果引用和唯一命令身份约束。
- 普通采购付款申请、冻结主体与来源、逐采购批准额度、未付占用及关闭和冲正规则。

## 依赖与协作边界

术语与统一分工见[总索引](../README.md#代码中常见名称的含义)。

- 本领域的数据结构、业务规则、服务和数据库仓储在此维护。常规、构建和测试依赖均不得指向其他业务领域或组合层。
- 跨域过账由 erp-processes 编排；财务中心和混合统计由 erp-read-models 组装。
- 金额、单价、数量和税率复用 erp-core::money；资金纠错中的退款与冲正事实属于 erp-returns。
- 需要其他领域的能力时，由组合层或应用将本域接口接到实际提供方；公开入口见 [src/lib.rs](src/lib.rs)。

## 代码入口

| 入口 | 用途 |
| --- | --- |
| [src/service/receivable.rs](src/service/receivable.rs) | 应收与客户回款服务 |
| [src/service/payable.rs](src/service/payable.rs) | 应付与供应商付款服务 |
| [src/service/cost.rs](src/service/cost.rs) | 成本与分摊服务 |
| [src/service/command_receipt.rs](src/service/command_receipt.rs) | 财务命令结果恢复与同 Executor 回执写入 |
| [src/entity/payable/payment_request](src/entity/payable/payment_request) | 付款申请状态、冻结依据、逐采购支付与关闭额度 |
| [src/repository/payable/account/reservation.rs](src/repository/payable/account/reservation.rs) | 应付来源上的申请占用与实际付款共享并发保护 |
| [src/repository/payable/payment_request_snapshot.rs](src/repository/payable/payment_request_snapshot.rs) | 不可变提交材料及精确审批实例、版本读取 |
| [src/entity/mod.rs](src/entity/mod.rs) | 本域实体、值对象和确定性规则 |
| [src/repository/mod.rs](src/repository/mod.rs) | 本域 MongoDB 仓储与集合访问器 |
| [src/indexes/mod.rs](src/indexes/mod.rs) | 公开索引注册入口 |

## 修改执行要求

1. 将无 I/O 的校验、状态迁移和不变式放入本域实体或值对象；Service 组织本域用例。
2. Repository 使用调用方传入的 `persistence_core::Executor`；跨集合原子写入由本域用例或 Process 控制事务。
3. 新增或调整集合查询时同步评估索引；组合根复用本域公开索引入口，保持既有逐集合注册顺序。
4. HTTP 请求和响应优先复用本域 DTO；扩展公开合同须同步检查 Process、ReadModel 和应用调用方。
5. 业务改动补充本域库单元测试，覆盖成功、失败、边界及相关幂等或版本冲突路径。
6. 进项发票登记、供应商付款、客户回款登记与分配、销项发票登记和开票申请提交或撤销命令，必须将 finance_command_receipts 与正式财务事实及成功审计在同一事务提交。回放按原命令身份、定位及版本化指纹校验强类型结果后读取原对象当前视图；异载荷和损坏回执必须明确失败，不反查审计，不追加第二次成功事件。回执不得按日志归档规则删除或设置 TTL。
7. 采购付款申请提交、撤回和关闭命令复用本域独立回执。申请、来源应付占用、审批或执行任务和成功审计必须在同一事务提交。实际付款先扣减该申请对应占用，再核销应付；任何步骤失败须整体回滚。
8. 付款申请批准仅授予冻结逐采购额度。已批准内容不可修改；关闭只释放未付占用。冲正必须保留原付款与申请关系，已关闭申请不得恢复原授权。来源余额、正式采购版本、主体及收款账户复验由跨域 Process 在同一事务执行。
9. 每轮付款申请提交须在审批实例创建的同一事务插入完整提交快照。快照按申请与提交版本、审批实例分别唯一；历史审核读取只能按精确关系装载快照，缺失时拒绝补读当前申请。

## 验证要求

以下命令在 `backend/` 目录执行：

```bash
env -u ERP_TEST_MONGO_URI cargo test -p erp-finance --lib --locked
```

代码变更还须执行[统一质量门禁](../README.md#质量门禁)。测试范围限定为库单元测试，不执行集成测试或真实外部服务测试。
