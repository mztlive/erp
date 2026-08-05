# S08 一期商城同步映射与期初导入

## 1. 元信息

- 分支：`feat/erp-s08-mall-sync-p1`
- 业务期：`p1`（卡券主责在商城，ERP 主动轮询）
- 依赖阶段：`S02`、`S03`
- `must_compile=false`；`docs/dev-plan/S08-PATCHNOTES.md`

## 2. 目标与业务范围

### 2.1 商城轮询与同步版本（W17 / phase-1 §8）

- 期初基线：正式卡券销售单（草稿不导）；默认 5 分钟增量完整快照
- 同步版本由 **ERP** 生成；商城禁止单据版本号
- 幂等键：**唯一** = 来源商城 + 来源销售单号 + 来源更新时间
- **禁止**内容指纹协议；**禁止**来源版本补发
- 水位：每来源一个 cursor；分页全成功才前移；基线后初值=B
- 映射失败写 `master_mapping_task` 不阻塞商城；未映射禁止错误应收
- 映射通过 reapply → 销售版本；派生应收交财务（W13）
- 每日全量四元组核对 `mall_sales_reconciliation_*`

### 2.2 W17 人工动作

立即增量/单号补拉/重试/每日核对/指派映射；确认映射仅业务角色；禁止手改水位。

### 2.3 W18 期初迁移

客户合同、供应商能力资质、供应商商品库、公司 SKU/卡券类目、仓库、期初实盘库存、卡券销售基线、卡券应收（已收/已开票=0）。流水线：接收→校验→确认→应用。

协作：W05 卡券只读时间线；W13 复核派生（S08 不实现复核写）。

## 3. 明确不在范围

事件/webhook/inbox/外发 API/错误中心；改造商城模型；玩法/卡号卡密；历史实物/期初应付/实体卡库存；ERP→商城类目下发；商城收付款同步；内容指纹主协议；T 切换/W23/W29/W30 写路径。

## 4. 代码落点

### owns_modules

- entities：`mall_sales_sync`、`master_mapping`、`legacy_import`
- repository 同上；services：`mall_sync`、`legacy_import`
- handler：`admin/mall_sync`、`legacy_import`

跨域写 sales_order revision / external_identity / work_item / background_job：经依赖阶段接口，不新建 sales 实体目录。

## 5. 数据模型与索引

- snapshot 唯一 `(source_system_id, external_order_key, source_updated_at)`
- cursor 乐观锁；reconciliation item 四元组
- mapping_task 同快照+类型进行中唯一
- legacy_import batch/row/confirmation 唯一约束

## 6. API 与权限草图

- `/admin/mall-sync/*`：context/jobs/snapshots/mapping/reconciliation + 触发/重试/指派/确认/reapply
- `/admin/legacy-imports/*`：batches 创建/upload/validate/apply/cancel/retry/issues/download
- work_item actions：映射确认、`COMPLETE_IMPORT_BUSINESS_CONFIRMATION`
- permission：`admin.mall_sync.*`、`admin.legacy_import.*`

## 7. 前端集成点

- `features/mall-sync`、`import-opening`；协作 sales-orders / card-funds-review
- keys：`mallSyncKeys`、`importOpeningKeys`；文案禁实现词

## 8. 实现任务清单

实体不变式（幂等/迟到/水位 B）→ 仓储 → poll/baseline/apply/reapply/mapping/reconciliation/legacy 全链路 → handler → S08-PATCHNOTES → 边界测试

## 9. Worktree / 并行约定

`feat/erp-s08-mall-sync-p1`；可与履约财务并行；合并依赖 S02+S03。

## 10. 验收标准

- [ ] 基线/增量/幂等三元组/迟到丢弃/ERP 版本
- [ ] 映射失败不阻塞；reapply 后 revision+期初 0/0
- [ ] 四元组核对；W17 人工与 W18 确认矩阵；无指纹主协议
- [ ] 风格/文档；`must_compile=false`

---

*阶段 ID：S08 · 分支：feat/erp-s08-mall-sync-p1 · phase_tag：p1 · must_compile：false*
