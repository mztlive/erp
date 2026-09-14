# erp-supplier：供应商账户与资质

管理“哪些往来对象可以作为供应商”，以及供应商能力、资质、评级和商业结算档案。

供应商身份及合作资格与具体供货商品需要分别维护。本领域回答供应商有哪些能力、是否具备相应资质，以及采用什么商业条件。

## 使用场景

- 修改供应商账户、能力、资质、评级或资格判断。
- 修改商业档案、付款条件、资料修订及资料保存的确定性规则。

## 协作示例

登记一家供应商的资质和结算条件使用本 crate；联系人和银行资料由 [erp-party](../erp-party/README.md) 维护；该供应商具体供应哪些 SKU、各自价格和可供状态由 [erp-supply](../erp-supply/README.md) 维护。

## 负责的数据与能力

- 供应商账户、能力、资质及能力关联、评级和相应修订。
- 商业档案、结算条件、资料命令及供应商创建和资料修订的确定性规则。

## 依赖与协作边界

术语与统一分工见[总索引](../README.md#代码中常见名称的含义)。

- 本领域的数据结构、业务规则、服务和数据库仓储在此维护。常规、构建和测试依赖均不得指向其他业务领域或组合层。
- 供给、API 连接、供应商履约和结算单属于 erp-supply；采购订单属于 erp-procurement。
- 主体资料、敏感令牌和资质附件通过 Port 取得；跨域资料保存由 erp-processes 编排。
- 需要其他领域的能力时，由组合层或应用将本域接口接到实际提供方；公开入口见 [src/lib.rs](src/lib.rs)。

## 代码入口

| 入口 | 用途 |
| --- | --- |
| [src/service/supplier/mod.rs](src/service/supplier/mod.rs) | SupplierService |
| [src/service/supplier/eligibility.rs](src/service/supplier/eligibility.rs) | 能力与资质资格规则 |
| [src/ports/mod.rs](src/ports/mod.rs) | 主体、文件资产和敏感令牌合同 |
| [src/entity/mod.rs](src/entity/mod.rs) | 本域实体、值对象和确定性规则 |
| [src/repository/mod.rs](src/repository/mod.rs) | 本域 MongoDB 仓储与集合访问器 |
| [src/indexes/mod.rs](src/indexes/mod.rs) | 公开索引注册入口 |

## 修改执行要求

1. 将无 I/O 的校验、状态迁移和不变式放入本域实体或值对象；Service 组织本域用例。
2. Repository 使用调用方传入的 `persistence_core::Executor`；跨集合原子写入由本域用例或 Process 控制事务。
3. 新增或调整集合查询时同步评估索引；组合根复用本域公开索引入口，保持既有逐集合注册顺序。
4. HTTP 请求和响应优先复用本域 DTO；扩展公开合同须同步检查 Process、ReadModel 和应用调用方。
5. 业务改动补充本域库单元测试，覆盖成功、失败、边界及相关幂等或版本冲突路径。

## 验证要求

以下命令在 `backend/` 目录执行：

```bash
cargo check -p erp-supplier --locked
env -u ERP_TEST_MONGO_URI cargo test -p erp-supplier --lib --locked
```

代码变更还须执行[统一质量门禁](../README.md#质量门禁)。测试范围限定为库单元测试，不执行集成测试或真实外部服务测试。
