# S05 履约与库存

## 1. 元信息

- 分支：`feat/erp-s05-fulfillment-inventory`
- 业务期：`p1`（实物与服务销售主链履约/库存）
- 依赖阶段：`S04`
- `must_compile=false`；PATCHNOTES：`services/src/fulfillment/PATCHNOTES.md`、`inventory/PATCHNOTES.md` 与/或 `docs/dev-plan` 登记
- 前端：`fulfillment-operations(W09)`、`inventory(W10)`
- 注：`ui-workspaces` 索引缺独立 w09 文档；契约以 phase-1 / data-model / ui-design / 前端 types 为准

## 2. 目标与业务范围

1. **W09 同一履约作业引擎**（侧栏两入口、五类表单）：入库 / 仓发 / 直发 / 电子 / 服务
2. **采购入库** `purchase_receipt(+line)`：合格入账+预占；不合格不入账
3. **发货** `delivery`：`WAREHOUSE_SHIP` 消耗预占写库存；`SUPPLIER_DIRECT` 不写自有库存
4. **电子交付 / 服务履约**：不写自有库存
5. **`acceptance_fulfillment_allocation`**：建模+读侧（验收完整流程属 W06/S03）
6. **W10 库存台账**：`stock_movement`/`balance`/`reservation`/`adjustment`；调整经办→仓储复核→财务确认→过账
7. **PrepaymentGate**：读采购 `payment_term_snapshot`；PREPAY 以有效已过账付款净核销判定；`PREPAYMENT_BLOCKED`
8. **公共注册**：过账注册 `business_document`；调整复核 `INVENTORY_ADJUSTMENT_REVIEW`

依据：data-model §6.7/§8；phase-1 §6–§7；w10；ui-glossary PrepaymentGate。

## 3. 明确不在范围

- 卡券一期履约/实体卡库存；调拨/盘点/批次保质期
- 客户验收完整作业（仅 allocation 协作）；采购/付款写路径
- 成本表自建（预留钩子）；期初导入（W18）；安全库存标签
- 用户文案「过账」口语化；五套一级菜单

## 4. 代码落点

### owns_modules

- entities：`purchase_receipt`、`delivery`、`electronic_delivery`、`service_fulfillment`、`stock`
- repository 同上；services：`fulfillment`、`inventory`
- handler：`admin/fulfillment`、`inventory`

```text
services/fulfillment/{mod,dto,queue,claim,draft,prepayment_gate,
  post_receipt,post_warehouse_ship,post_supplier_direct,post_electronic,post_service,defer,fact_query}
services/inventory/{mod,dto,query_ledger,query_balance,create/update/submit/review/post_adjustment,export}
```

汇合文件禁止改。流水禁止 update/delete；余额 `available=on_hand-reserved` 且 ≥0。

## 5. 数据模型与索引

集合名=表名。`stock_balance (warehouse_id,sku_id)` 唯一；流水业务动作唯一；预占归属销售行。事务不变量：入库+预占；仓发消耗本单预占；直发/电子/服务零库存流水；调整岗位分离。

## 6. API 与权限草图

| 路径 | 权限建议 |
| --- | --- |
| `GET /admin/fulfillment/queue`、claim/draft/post/defer/post-result、facts | `fulfillment:*` |
| `GET /admin/inventory/ledger`、balances、adjustments 全链路、exports | `inventory:*` |

`allowedActions`/`actionBlockers` 服务端返回；无仓库范围 `NO_DATA_SCOPE`。

## 7. 前端集成点

- `features/fulfillment-operations/`、`inventory/`
- FormalAction `unknown` 不乐观改库存；禁止前端重算 available
- keys：`fulfillmentKeys`、`inventoryKeys` 含仓库范围版本

## 8. 实现任务清单

建模五类事实+库存不变量 → 仓储 → prepayment_gate + 五类 post + 调整全链路 → handler → 前端 api → 并发/超额/门禁/岗位分离测试

## 9. Worktree / 并行约定

`feat/erp-s05-fulfillment-inventory`；depends S04；提供履约净成功量只读（W06）；消费采购分配与付款净核销。

## 10. 验收标准

- [ ] 五类确认成功；仓发/入库库存可见；直发等零库存流水
- [ ] PREPAY 门禁；预占本单可用；调整分离过账后非负
- [ ] 幂等/unknown；无卡券/调拨入口；风格/文档；`must_compile=false`

---

*阶段 ID：S05 · 分支：feat/erp-s05-fulfillment-inventory · phase_tag：p1 · must_compile：false*
