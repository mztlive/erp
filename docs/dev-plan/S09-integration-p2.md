# S09 二期集成域

## 1. 元信息

- 分支：`feat/erp-s09-integration-p2`
- 业务期：`p2`（P0/P1 同批研发上线）
- 依赖阶段：`S01`、`S03`、`S04`、`S08`
- `must_compile=false`；`docs/dev-plan/S09-PATCHNOTES.md`

## 2. 目标与业务范围

| 能力 | 摘要 | 依据 |
| --- | --- | --- |
| P2-C01 | API 供应商连接 + capability；密钥仅引用；双角色启停 | phase-2；data-model §6.14；W20 |
| P2-C02 | API 目录同步写入**同一** supplier_catalog 模型 | W21 |
| P2-C03 | product_publication 修订/投递；安全暂停不可恢复 ON_SALE | §6.15；W22 |
| P2-C04 | 双级审批契约 + sales_order_projection 下发 | §6.16；W05/W23 |
| P2-C05 | 商城消费五类事实、inbox、卡实例、组合支付分摊 | §6.17–6.18；W25 |
| P2-C06 | 固定供应关系供应商履约（仅 T 后 ERP_AUTOMATED） | §6.19；W26 |
| P2-C07 | 周期结算→应付；岗位分离 | §6.20；W27 |
| P2-C08 | 卡券消费成本评估 + 经营分析只读 | W28 |
| P2-C09 | 集成治理普通表组（**不建** outbox/中间件） | §6.21；W29/W30 |
| 闸门 | `mall_consumption_cutover` 唯一 T；P0/P1 闭环后才自动履约 | phase-2 §8.5 |

前端面：W20–W23、W05 增强、W25–W30。

## 3. 明确不在范围

卡券生产/绑定/激活/卡密；CRM；询价比价；H5 供应商；动态比价路由；财务软件接口；调拨/批次；完整总账；outbox/消息中间件；玩法规则；安全暂停恢复 ON_SALE；并行改一期核心聚合文件；汇合文件。

## 4. 代码落点

### owns_modules

entities：`supplier_api`、`product_publication`、`sales_order_projection`、`mall_consumption`、`mall_after_sales`、`supplier_fulfillment`、`supplier_settlement`、`integration_governance`  
repository 同上；services 另加 `card_business`；handler 对应 admin 子域。

硬约束：消息幂等先于业务；支付同事务建 mall_order；T 比较用 occurred_at；退款/余额恢复/供应商退款三账分离；投影字段白名单。

## 5. 数据模型与索引

表清单见阶段 JSON。关键唯一键：inbox (source_system, source_event_id)；mall_order (mall_id, external_order_no)；fulfillment_order_no；settlement statement_no；error_task (message_id, error_class) 等。PATCHNOTES 声明 ensure_indexes。

## 6. API 与权限草图

- supplier-api connections/capabilities/health/catalog-sync
- product-publications revisions/pause/retry
- sales-order-projections query/retry/escalate
- mall-consumption orders/inbox/cutover/backfill
- supplier-fulfillment / settlements / integration-governance / card-business analytics
- permission resource 命名见阶段 docs；写操作带 version

## 7. 前端集成点

features：supplier-api-connections、supplier-catalog（API 增量）、product-publications、execution-projections、sales-orders、mall-consumption-orders、supplier-orders、supplier-settlements、card-business-analytics、integration-errors、history-backfill。mock 直至集成；金额禁止前端重算分摊。

## 8. 实现任务清单

建模 → 仓储 → 各 service 用例（含 cutover/backfill）→ handler → S09-PATCHNOTES → 幂等/T 前后/投影不回退/结算岗位分离测试

## 9. Worktree / 并行约定

`feat/erp-s09-integration-p2`；不碰 supplier_catalog 实体文件（调用 S08）；不引入 outbox。

## 10. 验收标准

- [ ] W20–W30 主路径契约；P0/P1 闭环闸门；无 out_of_scope
- [ ] 风格/文档；`must_compile=false`；集成后 cargo 门禁

---

*阶段 ID：S09 · 分支：feat/erp-s09-integration-p2 · phase_tag：p2 · must_compile：false*
