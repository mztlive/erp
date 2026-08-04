# Phase 8：期初导入与一期商城卡券销售单同步

## 1. 分支与隔离

| 项目 | 约定 |
| --- | --- |
| 分支名 | `codex/backend-p1-08-mall-import-sync` |
| 基线 | 与全部 phase 相同的冻结 `BACKEND_PHASE_BASE_SHA` |
| 实现语言 | 读取冻结基线中的统一后端语言/版本决定；缺失时停止，不自行选栈 |
| 独占目录 | `backend/modules/mall-import-sync/**` |
| 编译要求 | 不要求根工程编译；同步算法、端口、安全规则和测试向量必须完成 |
| 禁止修改 | 正式销售/库存/票款表实现、全局迁移/API、其他 phase、前端 |

## 2. 目标与范围

实现第一期商城 → ERP 的只读兼容层：

- 期初基础资料、仓库实盘库存、供应商商品/供给等受控批次的接收、manifest、试算、
  责任确认和应用编排；具体领域事实始终由各对象 phase 唯一写入；
- 已生效及之后状态且未作废的商城卡券销售单期初基线；
- 基线后的完整快照增量轮询、按单补拉和每日全量清单核对；
- 来源身份键派生、内容指纹、同刻游标、来源快照、映射差异和重新归集意图；
- 期初卡券应收“已收 0、已开 0、待财务复核”意图；
- 原始数据安全清洗、HMAC 谱系、资产分级保留和运行指标。

明确排除第二期的主责迁移、ERP → 商城执行投影、卡实例、卡号/卡密、绑定/激活、
消费与支付回流、供应商自动下单、退款和余额恢复。

依据：`erp-phase-1.md` §5.3、§8；`erp-data-model.md` §6.1、§6.12～§6.14、
§8、§11；`erp-mall-data-mapping.md` §1～§5、§9～§13；W17、W18。

## 3. 目录结构

```text
backend/modules/mall-import-sync/
  domain/{source_identity_key,snapshot,fingerprint,cursor,mapping,retention}/
  application/{import,sync,reconciliation,reapply}/
  ports/
  contracts/
  security/
  fixtures/
  tests/
  DECISIONS.md
```

正式对象写入只输出 `FormalObjectWriteIntent` 并进入 recording fixture；本 phase 不写
Phase 4、6、7 或 Phase 1 的存储。规范 `source_system` / `external_identity_map` 由 Phase 1
唯一写入；本 phase 只验证来源协议、派生二进制键并提交 `ExternalIdentityIntent`。
本 phase 只持久化通用开账治理的 `legacy_import_batch/row`；供应商目录专用
`supplier_catalog_intake_batch/item` 及目录/供给事实由 Phase 3 唯一写入。

## 4. 输入输出契约

### 4.1 商城只读端口

必须支持：

- 按 `[from, to)` 和 `(sourceUpdatedAt, externalOrderKey)` 升序稳定分页查询变更完整快照；
- 按单号读取完整正式销售单快照；
- 查询指定时点的正式销售单全量清单及按相同规范生成的完整内容指纹。

### 4.2 写入意图

- `OpeningInventoryIntent`：只含统一基准日、仓库 + SKU 的实盘数量和证据；
- `MallVoucherSalesIntent`：来源身份、完整快照、规范化商业字段、映射结果；
- `OpeningVoucherReceivableIntent`：成交金额、`received=0`、`invoiced=0`、待复核；
- `MappingResolutionIntent`、`ReapplySnapshotIntent`、`ReconciliationDifferenceIntent`。
- `SupplierCatalogImportIntent` 等领域导入意图；W18 只编排批次和确认，目录行的正式校验/
  应用仍由 Phase 3 唯一负责。
- `ImportBusinessConfirmationIntent`：固定任务
  `IMPORT_BUSINESS_CONFIRMATION`，业务对象 `LEGACY_IMPORT_BATCH`，按
  `batchId × confirmationScope × trialVersion × subjectHash` 生成一个有效责任确认；
  `subjectHash` 必须包含责任范围，`confirmationScope + ownerRole` 分责，handler 为 `import_business_confirmation`，完成动作为
  `COMPLETE_IMPORT_BUSINESS_CONFIRMATION`，decision 仅为 `CONFIRM_SCOPE | RETURN_FOR_FIX`。

Phase 10 才将这些意图绑定到销售、库存、财务和任务的真实同事务写入。确认完成事务必须同时写
确认/退回事实、`workflow_action` 和当前任务 `COMPLETED`；`RETURN_FOR_FIX` 的业务结论为
`REJECTED`，不是转交、关闭或暂挂。修复并形成新的 `trialVersion` 或 `subjectHash` 后才可创建新任务。

## 5. 状态与不变量

### 5.1 外部身份与快照

- 身份固定为 `sourceSystemId + CARD_SALES_ORDER + externalOrderKey`；保留原值，按二进制
  语义比较，不按名称/金额猜对象。
- 一张来源单只能映射同一个 ERP 销售单；重复处理复用同一身份。
- 快照白名单只包含商业事实：状态、客户、合同、结算主体、项目/备注、卡券类目、
  履约期限、唯一卡券明细、金额、税率和开票要求。
- 玩法、卡号、卡密、绑定、激活、支付、消费和连接密钥不得进入快照或指纹。

### 5.2 幂等、迟到与游标

- 同一 `source + orderKey + sourceUpdatedAt + contentHash` 幂等。
- 当前内容相同为 `NO_CHANGE`；A→B→A 的第三次 A 是新观察，不能因旧 hash 出现过而丢弃。
- 更早快照保留为迟到证据但不回退当前版本；同一来源时间不同指纹必须标记来源冲突，
  不按到达顺序选胜者。
- 游标保存安全高水位及同刻已处理来源键；页内快照未全部安全持久化时不得前移。
- 基线开始记录 `B`，成功后增量从 `B` 开始，而非基线结束时间。
- 商城一期没有来源版本号，本 phase 不伪造版本间隙检测或补发协议；每日全量核对只能
  发现当前态差异，不能恢复两个轮询周期之间已被覆盖的中间变化。

### 5.3 导入与映射

- 导入经过接收、校验、试算、按责任范围确认、应用和结果阶段；生产应用前先在验证环境。
- 正常导入确认只使用 `IMPORT_BUSINESS_CONFIRMATION`，不得借用 `BUSINESS_EXCEPTION`；handler
  未在后端和 W01/W02/W18 受控注册表实际接线前，确认能力保持 fail-closed。
- 已成功对象不因取消、失败重跑或新文件回滚；同来源身份重跑幂等跳过已成功项。
- 零/多卡券明细、未知状态、金额/税率解析失败、客户/合同/结算主体/类目无法映射时，
  只形成差异，不生成销售、应收、库存或经营归属。
- 映射状态与重新归集操作状态独立；结果未知不回滚已解决映射，也不自动完成/下一项。
- 每日核对只生成差异和补拉意图，不直接覆盖正式事实。

### 5.4 安全与保留

- 原始 SQL、连接头和含禁止字段导出只在受控临时区处理。
- 长期保留白名单规范化包、manifest、规则版本、成功结果、映射审计和带密钥 HMAC 谱系；
  不保存普通摘要代替 HMAC。
- 失败合规包及行列诊断保留 30 天；失败批次元数据、汇总计数和脱敏错误长期保留。
- 日志、错误、fixture 和导出不得含卡密、完整手机号、Token、签名密钥或连接信息。

## 6. 测试要求

1. 来源身份二进制语义、白名单指纹和禁止字段拒绝。
2. 重复、乱序、跨页重试、同刻多单、页失败和进程中断的游标性质测试。
3. 基线 `B` 期间变化、迟到快照、A→B→A、同刻不同内容和商城不可用。
4. 全量清单的商城缺失、ERP 缺失、状态/内容差异和重复身份；只产差异。
5. 验证环境确认、试算失效、生产应用、部分失败和成功行幂等重跑；覆盖
   `IMPORT_BUSINESS_CONFIRMATION` 的 scope/owner 分责、同版本去重、`CONFIRM_SCOPE` 与
   `RETURN_FOR_FIX` 的同事务完成，以及新 `trialVersion` / `subjectHash` 才产生新任务。
6. 期初库存只接受实盘；商城 stock、历史流水和实体卡库存全部拒绝。
7. 期初卡券应收固定零已收/已开，商城支付字段不能生成回款。
8. 敏感字段扫描、日志脱敏、资产保留到期和下载再鉴权意图。

## 7. 未决项与 fail-closed

- `erp-mall-data-mapping.md` §4.3 的 P0 数据剖析任一不通过，不得形成正式卡券销售单。
- W17 Q1：映射 SLA/升级策略未配置时不生成默认 SLA、升级任务或超期管理结论。
- W17 Q2：结算主体映射唯一 owner 未配置时确认动作阻断。
- W17 Q3：人工立即增量/按单补拉治理策略未配置时禁用；定时同步不受影响。
- W17 Q4：来源修复请求只追加当前任务说明，不创建未经注册的外部协同状态。
- W17 Q5：历史留存与导出策略未确认时，证据按安全保留基线留存但普通导出禁用；
  不由本 phase 猜测归档年限或角色。
- W18 Q1～Q4：确认矩阵、生产双人复核、批次上限和验证结果有效期必须由版本化策略提供。
- W18 Q5：只接受已登记的治理型批量导入对象集；日常正式业务不得借 W18 绕过对象工作面，
  未登记对象类型直接拒绝。
- W18 正常导入确认的类型、对象、handler、去向和完成动作已经固定；在 Phase 10 的真实任务
  注册、W01/W02 展示映射、W18 handler 与结果查询全部接线并验证前，确认入口仍保持 blocker。
- W17 文档含第二期冻结/封存内容；本 phase 仅实现第一期运行态，Phase 10 不得误注册
  主责迁移命令。

## 8. 完成标准

- 同步/导入只写独占兼容层，并通过意图调用正式对象唯一写者。
- 游标、指纹、幂等、乱序、全量核对和安全保留有性质/场景测试。
- 二期能力和禁止敏感字段没有进入第一期契约。
- 向 Phase 10 交付只读端口、写入意图、运行指标、错误码、逻辑约束和 blockers。
