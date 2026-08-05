# S06 财务往来票款成本与经营分析

## 1. 元信息

- 分支：`feat/erp-s06-finance`
- 业务期：`p1`
- 依赖阶段：`S03`、`S04`
- `must_compile=false`；`docs/dev-plan/S06-PATCHNOTES.md`
- 依据：phase-1 §5.1/§6.5/§7.5/§8；data-model §6.8–§6.11；W11–W13、W15、W16

## 2. 目标与业务范围

### 2.1 应收与客户往来（W11）

- 销售生效派生 `receivable_account`+原始 entry；`counterparty_party_id`=settlement_party；变更只追加差额
- 回款 `customer_receipt`+`receipt_allocation`（目标 entry）；销项票 allocation 目标 account
- 多对多核销会话；普通登记不过复核；纠错岗位分离

### 2.2 应付与供应商往来（W12）

- 采购审核通过派生应付；付款/进项对称；门禁仅计有效已过账净核销付款
- 优先级策略缺失 fail-closed

### 2.3 卡券票款复核（W13）

- 期初已收/已开票=0；逐单人工复核；禁止估算批量结清
- `receivable_funds_review` 追加链 + CARD_FUNDS_REVIEW / DELTA；同事务完成 work_item
- 驳回无后继任务猜测

### 2.4 非卡券成本与盈亏（W16）

- `cost_entry`/`allocation`；实际盈亏仅 NON_VOUCHER + ACTUAL/REDUCTION
- 卡券供货成本缺失不进利润、不当 0

### 2.5 客户经营质量（W15）

- 只读派生；卡券进规模/回款、不进实际盈亏；`fundsReview=reviewed_only` 整单过滤

### 2.6 退拒纠错

销售退货/采购退货/客户退款/供应商退款/回款冲正/付款冲正/红票 + `document_relation` + `FINANCE_CORRECTION_REVIEW`

## 3. 明确不在范围

完整总账/法定报表；银行流水接口；卡券实际供货成本盈亏（一期）；按耗月结；商城收付款同步；估算批量结清卡券票款；可配置审批流。

## 4. 代码落点

### owns_modules

entities：`receivable`、`payable`、`customer_receipt`、`supplier_payment`、`invoice`、`cost`、`sales_return`、`purchase_return`、`customer_refund`、`supplier_refund`、`receipt_reversal`、`payment_reversal`  
repository 同上；services：`finance`、`customer_quality`、`profit_loss`  
handler：`admin/finance`、`customer_quality`、`profit_loss`

Service 按用例拆：derive_receivable/payable、receipt/payment/invoice、funds_review、cost、returns、allocation_session、correction_review。

## 5. 数据模型与索引

集合名=表名。账户 uk 与业务幂等 uk 见 data-model；复核链 uk (account, review_no) 与 work_item_id；过账锁定资金→子账→目标分录。

## 6. API 与权限草图

- `/admin/finance/*`：客户/供应商账户、核销会话、过账、冲正、红票、卡券复核、成本、退拒
- `/admin/analytics/*`：customer-quality、profit-loss
- permission：`finance.*`、`analytics.customer_quality`、`analytics.profit_loss`
- 幂等 + `GET .../operations/{operation_id}`

## 7. 前端集成点

| Feature | 路由 |
| --- | --- |
| customer-receivables | `/finance/customer-accounts` |
| supplier-payables | `/finance/supplier-accounts` |
| card-funds-review | `/finance/card-funds-review` |
| customer-quality | `/analytics/customer-quality` |
| actual-profit-loss | `/analytics/profit-loss` |

替换 api.ts；分配目标类型不可混用；keys 含 permissionVersion。

## 8. 实现任务清单

entities 不变式 → repository → derive/过账/复核/纠错/分析 query → handler → S06-PATCHNOTES → 超额/跨主体/幂等/岗位分离测试

## 9. Worktree / 并行约定

`feat/erp-s06-finance`；与 S05 可并行；依赖销售生效/采购审核版本契约；work_item 类型仅注册表。

## 10. 验收标准

- [ ] 应收/应付幂等派生；W11/W12 多对多；门禁口径
- [ ] W13 期初 0/逐单/禁止批量；W15/W16 覆盖与卡券不进利润
- [ ] 纠错追加反向+岗位分离；风格/文档；`must_compile=false`

---

*阶段 ID：S06 · 分支：feat/erp-s06-finance · phase_tag：p1 · must_compile：false*
