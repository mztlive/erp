# S03 销售单变更与客户验收

## 1. 元信息

- 分支：`feat/erp-s03-sales`
- 业务期：`p1`（二期卡券审批字段/挂点预留，完整投影增强可延至 S09）
- 依赖阶段：`S02`
- `must_compile=false`；禁止并行修改共享汇合文件；`docs/dev-plan/S03-PATCHNOTES.md`
- 工作面：W05 销售单、W06 客户验收（嵌套 `section=acceptance`）

## 2. 目标与业务范围

1. **实物与服务销售单 ERP 开单**：working_copy → submission → revision + goods_service 明细
2. **卡券销售单实体同构 + 一期只读主责**：`business_type=VOUCHER`；`origin_system=MALL` 商业字段只读
3. **list/detail**：主状态、review_status、三轨进度、`allowedActions`/`actionBlockers`
4. **低毛利上级确认 + 采购驳回三出路**：`RESUBMIT_CHANGED_TERMS` / `REQUEST_LOW_MARGIN_ACCEPTANCE` / `VOID_AFTER_REJECTION`
5. **销售变更单**：完整目标提交；生效只追加新 revision
6. **导出**（筛选+权限一致）
7. **客户验收 W06**：`customer_acceptance` + allocation；一期仅 `DIRECT_OBJECT`；卡券单无验收编辑
8. **固定状态机**：`commercial_status` DRAFT|PENDING_REVIEW|EFFECTIVE|VOIDED；关闭=履约完成+应收结清（开票非关闭条件）
9. **二期预留** `sales_order_review` 双级审批字段与 CARD_SALES_* 挂点
10. **跨域配套**：`business_document`、`document_participant`、`workflow_action`、`work_item`（固定类型，禁止同义码）

依据：`erp-data-model.md` §6.4–§6.5/§6.7/§7；`w05`/`w06`；`erp-phase-1.md`。

## 3. 明确不在范围

| 禁止项 |
| --- |
| 两类销售单混单；卡券一期履约/采购/库存触发；玩法/卡号卡密 |
| 可配置审批流；T 后投影完整实现（S09）；商城自动发卡消费同步 |
| W07 确认本体；W09 履约过账；W11 核销；人工关闭；开票作为关闭条件 |
| 修改共享汇合文件 |

## 4. 代码落点

### owns_modules

- `entities/src/{sales_order,sales_change,customer_acceptance}`
- `repository/{sales_order,sales_change,customer_acceptance}`
- `services/src/sales`
- `handler/admin/sales`

### 建议树

```text
services/sales/{mod,dto,create,working_copy,submit,query,export,
  procurement_rejection,low_margin,card_approval,change_order,acceptance,status,access}
handler/admin/sales/{list,detail,create,working_copy,submit,procurement_rejection,
  low_margin,card_approval,change_order,export,acceptance,operation}
```

汇合仅 PATCHNOTES：mod、DatabaseExt、indexes、routes nest、permission keys。

硬约束：多集合写事务；submission/revision/已过账验收不可变；金额/关闭资格仅服务端；卡券恰好一行；`origin_system`/`business_type` 创建后不可变。

## 5. 数据模型与索引

表：`sales_order(+line/working_copy/submission/revision/goods_service/voucher/review)`、`sales_change_*`、`customer_acceptance(+line)`、`acceptance_fulfillment_allocation`、公共 `business_document`/`work_item`/`workflow_action`/`document_participant`。

关键：`order_no` 唯一；卡券生命周期一稳定行；关闭 `close_status` + `CloseEligibility`（开票不阻断）。

## 6. API 与权限草图

- `GET/POST /admin/sales-orders`、working-copy、submit、procurement-rejection/resolve、low-margin、card-sales-approval、change-orders、export
- acceptance-workspace、drafts、post、reverse；`GET .../operations/{operation_id}`
- permission：`sales_order`/`sales_change_order`/`customer_acceptance` 各 action
- 正式写返回 `operationId` + COMMITTED|NOT_COMMITTED|RESULT_UNKNOWN；幂等键

## 7. 前端集成点

- `erp-client/features/sales-orders/*`：types/api/queries/acceptance；保留 `salesOrderKeys`
- 只渲染服务端 `allowedActions`；关闭条件只展示服务端 `closeEligibility`
- 验收仅 DIRECT_OBJECT

## 8. 实现任务清单

A 建模与状态守卫 → B 仓储 → C services 全用例 → D handler → E 测试 → F 前端 api 接线（契约冻结后）

## 9. Worktree / 并行约定

```bash
git worktree add ../erp-s03 -b feat/erp-s03-sales <base-after-S02>
```

对采购提供 PROCUREMENT_CONFIRMATION work_item；对财务提供应收挂点；对商城同步提供只读 origin/source 字段。

## 10. 验收标准

- [ ] GOODS_SERVICE 草稿/提交/采购任务；驳回三出路；卡券 MALL 只读；混单拒绝
- [ ] 变更追加 revision；验收 DIRECT+冲正；关闭条件正确；review 挂点；RESULT_UNKNOWN 可恢复
- [ ] 风格/文档范围/PATCHNOTES；`must_compile=false`

---

*阶段 ID：S03 · 分支：feat/erp-s03-sales · phase_tag：p1 · must_compile：false*
