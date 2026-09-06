# Common 公共基元（P0-1.4/1.5，P0 冻结后只读）

本目录是 P0 为全部 34 个域预置的共享基元：字段基元、时间、来源类型与固定状态机。
P1 各域实施者按下方判定表选用，**不得自行复制字段结构或另建一套时间/数值类型**。

## 何时用哪个基元

| 对象性质                                                                                 | 基元                  | 说明                                                                                                                                                |
| ---------------------------------------------------------------------------------------- | --------------------- | --------------------------------------------------------------------------------------------------------------------------------------------------- |
| 稳定基础资料 / 可编辑草稿（客户、供应商、商品、SKU、仓库、来源系统…）                    | `StableBase<Status>`  | `status`、`current_revision_id`、`created_by`、`updated_by`；与 `BaseModel` 组合使用                                                                |
| 不可变修订（`party_revision`、`product_revision`、`sku_revision`、`contract_revision`…） | `RevisionBase`        | `revision_no`（从 1 递增）；修订正文在域内自己的修订表中                                                                                            |
| 正式事实（库存流水、收付款、发票、商城事实、成本、退款、纠错…）                          | `FactBase`            | `fact_no`、`occurred_at`、`recorded_at`、`recorded_by`、`source_type`、`source_reference`、`reason_code`、`reason_text`；事实不可变，纠错用反向事实 |
| 事实来源标注                                                                             | `SourceType`          | `erp` / `mall_sync` / `history_backfill` / `supplier_callback` / `manual_import`；展示用 `label()`                                                  |
| 业务自然日（到期日、结算期间）                                                           | `BusinessDate`        | 无时区语义；serde 为 `YYYY-MM-DD` 字符串                                                                                                            |
| 业务时间 / 记录时间（`occurred_at`、`recorded_at`）                                      | `Instant`             | UTC 统一时基；serde 为秒级 i64 时间戳，展示层转业务时区                                                                                             |
| 固定状态机                                                                               | `DocumentState` trait | 域内枚举实现 `allowed_next()`；迁移一律走 `ensure_transition`；邻接矩阵固化，禁止运行时扩展（数据模型 4.6、13.3）                                   |

## 与 `BaseModel` 的关系

`BaseModel`（`entity_core`）承担持久化元数据：`id`、`version`、`created_at`、
`updated_at`、`deleted_at`。**`BaseModel.version` ≡ 数据模型的 `lock_version`**
（乐观并发版本），P0 已定，此后不再改名；`created_at`/`updated_at` 为 u64 秒，
与 `Instant` 的 i64 秒时间戳同一 JSON 数值形态。

组合方式：实体 `#[serde(flatten)] BaseModel` + 按对象性质内嵌 `StableBase` /
`RevisionBase` / `FactBase`。`StableBase.current_revision_id` 指向当前生效修订的
`*RevisionId`（存 `String`，业务引用仍须用 `entities::ids` 的类型化 ID）。

## 数值类型

金额/单价/数量/税率一律使用 `entities::money`（P0 冻结，见 `money.rs` 与
conventions.md 第 5 节）：`Amount`(2) / `UnitPrice`(4) / `Quantity`(6) / `Rate`(6)，
BSON 形态固定 `Decimal128`，HTTP 传输为字符串。禁止 `f64`、禁止裸 `String` 传金额。
