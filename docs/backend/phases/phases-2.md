# Phase 2：伙伴、客户、供应商资料与合同归档

## 1. 分支与隔离

| 项目 | 约定 |
| --- | --- |
| 分支名 | `codex/backend-p1-02-partner-contracts` |
| 基线 | 与全部 phase 相同的冻结 `BACKEND_PHASE_BASE_SHA`，不得从其他 phase 分支创建 |
| 实现语言 | 读取冻结基线中的统一后端语言/版本决定；缺失时停止，不自行选栈 |
| 独占目录 | `backend/modules/partner-contracts/**` |
| 编译要求 | 不要求接入根工程；要求完成领域代码、端口和测试向量 |
| 禁止修改 | 根构建/锁文件、共享路由/OpenAPI、正式迁移、其他模块、前端 |

本 phase 不依赖 Phase 1。权限、文件、任务和审计均通过本目录内声明的端口表达，
Phase 10 再绑定真实平台实现。

## 2. 目标与对象所有权

本 phase 是以下对象的唯一写者：

- `party`、`party_revision`、主体角色和联系人/地址修订；
- `customer_account`、`supplier_account`；
- 客户负责人、协作销售与历史参与关系；
- 供应商商务资料、能力、资质、评级和银行资料修订；
- `contract`、`contract_revision`、合同 PDF 归档和引用关系；
- 仓库基础身份、修订和 `warehouse_sku_policy`。策略代码仍由本 phase 唯一拥有并由
  Phase 6 只读消费；业务写责任部门未确认前运行时只读。

不拥有商品/SKU、供应商商品和供给、公司商品池、销售单、采购单、票款或库存余额。

依据：`erp-phase-1.md` §4.3、§4.5、§5.1、§11；
`erp-data-model.md` §6.2、§6.4、§6.3 中的仓库部分；W03、W04、W14。

## 3. 模块结构

```text
backend/modules/partner-contracts/
  domain/{party,customer,supplier,assignment,contract,warehouse}/
  application/{commands,queries}/
  ports/
  contracts/
  persistence-spec/
  fixtures/
  tests/
  DECISIONS.md
```

逻辑 schema 只写在 `persistence-spec/`；不得创建数据库方言 DDL 或全局 migration 序号。

## 4. 命令与查询

### 4.1 命令

- `CreateParty`、`AppendPartyRevision`、`SetPartyRoleStatus`；
- `CreateCustomerAccount`、`ChangeCustomerAssignment`、`SetCustomerCollaborators`；
- `CreateSupplierAccount`、`AppendSupplierCommercialRevision`；
- `AppendSupplierCapabilityRevision`、`AppendSupplierQualificationRevision`、
  `AppendSupplierRatingRevision`；
- `CreateContract`、`AppendContractRevision`、`VoidContractRevision`；
- `CreateWarehouse`、`AppendWarehouseRevision`、`SetWarehouseStatus`。
- `AppendWarehouseSkuPolicyRevision`：仅在 W14 Q1 书面确认业务责任方后注册运行时写入口。

所有写命令携带幂等键、预期修订/锁版本、操作者和原因；敏感资料命令只接受受控引用，
不在普通响应回显完整值。

### 4.2 查询

- W03 客户列表、详情、关系、合同/销售摘要和负责人历史；
- W04 合同列表、对象中心、不可变版本和安全 PDF 下载授权意图；
- W14 客户、供应商、仓库基础资料查询；
- 提供给其他 phase 的稳定只读快照：`CustomerSnapshot`、`SupplierSnapshot`、
  `SupplierCapabilitySnapshot`、`ContractSnapshot`、`WarehouseSnapshot`。

查询返回稳定 ID、当前修订、数据水位、`allowedActions` / `actionBlockers`；权限过滤由
本地 `AuthorizationPort` 代表，不能写死角色名。

## 5. 领域不变量

- 伙伴稳定身份和历史修订不可因停用而删除；相似名称/税号只能生成候选，不自动合并。
- 客户负责人变更只影响新业务范围；已参与的历史单据查看关系保留。
- 供应商类型不能替代多选能力；资质独立保存并按业务日期控制能力有效性。
- 银行账号、证件、联系人电话和地址按敏感字段处理；日志、错误和审计不得保留完整值。
- 合同与销售单一对多；本 phase 只管理合同，不创建销售单。
- 合同正文为 PDF-only 归档：扫描通过、内容签名和不可变版本形成原子意图；纸质投影
  不能反推正式金额、状态或签署结果。
- 合同修订被正式销售提交引用后不得覆盖；纠错必须追加新修订。
- 仓库停用不得删除身份；被库存、履约或策略引用时必须由服务端影响检查阻断。
- 任何跨 phase 引用只暴露稳定 ID + 修订号/生效区间，不暴露 ORM 对象。

## 6. 端口与独立测试

本 phase 自有端口至少包含：

- `AuthorizationPort`、`AuditPort`、`FileSecurityPort`；
- `BusinessReferenceCheckPort`：记录需要由销售、采购、库存、票款确认的占用检查；
- `DuplicateCandidatePort`：只生成候选，不执行自动合并；
- `ContractUsagePort`：按稳定引用检查合同修订是否已被正式提交使用。

在 `fixtures/` 提供 recording/deny-by-default 实现，证明本 phase 可在没有其他 phase 的
情况下执行领域测试。

测试至少覆盖：

1. 同一主体并发修订、有效期重叠、旧版本写冲突。
2. 客户归属变更后的新业务范围与历史参与权。
3. 停用客户/供应商/仓库时存在下游引用的 blocker。
4. 供应商多能力、资质过期、未来生效和被撤销后的业务日期校验。
5. 合同 PDF 扫描失败、重复上传、版本引用后覆盖拒绝、短时下载再鉴权意图。
6. 敏感字段掩码、完整揭示审计和权限收回。
7. 同名/相似税号只生成候选而不自动合并。

## 7. 未决项与 fail-closed

- W03 Q1～Q4 涉及协作销售编辑权、客户停用影响、默认关注规则和银行资料权限；
  未确认前以最小权限返回 blocker，不把建议写死。
- W14 Q1：仓库与仓库 SKU 策略的唯一业务写责任部门未确认。代码和数据对象固定由本
  phase 拥有，默认不向运行时暴露写能力；Phase 10 只在书面确认责任方后注册。
- W14 Q2：未生效修订取消规则未确认；被引用或已生效的修订始终只能追加纠正版本。
- W14 Q3：完整供应商银行资料维护入口未定；后端始终按财务字段权限控制，不以页面入口
  代替授权。
- W14 Q5：不得建设通用可配置审批流；没有已确认资源规则时不临时创建审核任务。

## 8. 完成标准

- 对象所有权和 Phase 3 的商品/供给边界无重叠。
- 所有修订、有效期、停用、敏感字段和合同归档规则有测试向量。
- 没有真实跨模块外键、共享类型、HTTP 注册或物理迁移。
- 向 Phase 10 交付稳定快照契约、逻辑约束、占用检查清单、错误码和未决项。
