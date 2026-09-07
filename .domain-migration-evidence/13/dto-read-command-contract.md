# 阶段 13：DTO、读模型与无审批退货实施合同

## 1. 输入与归属

- 唯一实施树：`/private/tmp/erp-domain-crate-13-returns`。
- 输入源码提交：`de112614e03714bb6d1ca4ac9e25f580c5c273c8`。
- 本分片只修改下表所列归属文件；四个资金命令主文件与四个审批 adapter 主文件由各自 owner 删除迁出片段。
- 原始快照：`/private/tmp/returns13-c-before/backend/services/src/returns`。
- 逐 DTO、函数、测试、文件哈希证据：`/private/tmp/returns13-c-semantic-review.json`；复核脚本：`/private/tmp/returns13-c-semantic-review.py`。

## 2. 唯一公共出口

| 消费者 | 公共合同 |
| --- | --- |
| 创建、commit、提交、取消、HTTP post 请求 | `erp_returns::dto::*`；21 个请求 struct、4 个 Commit aliases。 |
| 分页、列表参数、详情及审批展示 DTO | `erp_read_models::returns_center::dto::*`；与请求合计 46 个原定义的字段、serde、Validate token 全等。 |
| 六类详情、三类列表 | `erp_read_models::returns_center::ReturnsReadService::new(Database)`；原方法名与参数、返回 View 保持。 |
| 销售退货构造及本域写入 | `erp_returns::service::sales_return::{build_sales_return_case_and_line,persist_sales_return_case_with_line}`。 |
| 采购退货构造及本域写入 | `erp_returns::service::purchase_return::{build_purchase_return_order_and_line,persist_purchase_return_order_with_line}`。 |
| 无审批创建根 | `erp_processes::reverse_flow::ReturnsProcess::{create_sales_return_case,create_purchase_return_order}`。 |

ReadService 公开方法为 `sales_return_case_list/detail`、`purchase_return_order_list/detail`、`customer_refund_list/detail`、`supplier_refund_detail`、`receipt_reversal_detail`、`payment_reversal_detail`。所有写后详情在原根成功后原位置重新读取。

## 3. 文件核销

| 原源 | 实际目标 |
| --- | --- |
| `services/src/returns/dto.rs` | `erp-returns/src/dto/{mod,requests}.rs`；`erp-read-models/src/returns_center/dto.rs`。 |
| `services/src/returns/customer_refund_list.rs` | `erp-read-models/src/returns_center/customer_refund_list.rs`，保留全部 5 个原测试。 |
| `services/src/returns/{sales_return,purchase_return}.rs` | 各自 domain service、process、read-model 叶。 |
| 四个资金主文件的 detail/view、客户退款 list | `erp-read-models/src/returns_center/{customer_refund,supplier_refund,receipt_reversal,payment_reversal}.rs`。 |
| 四个 adapter 的 view/allowed_actions 与定义摘要 | `erp-read-models/src/returns_center/approval/{mod,customer_refund,supplier_refund,receipt_reversal,payment_reversal}.rs`。 |
| `database/src/repository/returns_customer_refund_search.rs` | `erp-read-models/src/returns_center/repository/returns_customer_refund_search.rs`；原 2 个 ignored/Mongo 门控库测试。 |

本分片拥有的旧 DTO、客户列表、销售退货、采购退货和 ignored 查询测试 5 个源文件已删除。新查询测试通过 `returns_center/mod.rs` 的 `#[cfg(test)] mod repository` 注册，只消费 `erp_returns::repository::returns` 与 `erp_returns::indexes::ensure`。

## 4. 不可变执行顺序

1. `req.validate()` 仍在创建根首位。
2. 领域按原位置生成头 ID、构造头、生成首行 ID、构造 `req.lines[0]`。请求第二行及后续行仍不写入，不新增来源存在性、库存、金额或数量累计查询。
3. 原 audit 在事务打开前构造。
4. 同一原 session 依次执行政策与 adapter 检查、原统一绑定端口、BusinessDocument 写入、本域头与首行写入、audit 写入。
5. 生产 `CreationSteps` 的 `register_document → persist_return → audit` 逐步复用调用方 Executor；任何一步首错停止后续写入。
6. 事务成功后按原 ID 调统一 ReadService 读取详情。
7. 客户退款列表保留本域分页后批量业务注册查询，包括空页原查询调用、缺注册行保留以及原审批缺省值。其他列表保留原逐行详情读取顺序。

## 5. 验证登记

- 46 个 DTO/alias 定义 token 全等。
- 41 个直接迁移函数在明确路径、可见性替换后 token 全等；采购原内联构造与唯一领域工厂 payload token 全等。
- 本分片保留 21 个原测试，其中 2 个 ignored；原断言与 ignore 原因保留。客户退款既有源码测试改为读取实际 query/process/domain 生产片段，保留全部断言。
- 新增 6 个测试：2 个真实领域构造首行合同；两条真实生产创建步骤各 2 个同 Executor、顺序、逐步首错测试。
- 定向 rustfmt 与 `git diff --check` 已执行；未执行 Cargo、测试运行或 MongoDB。测试结果以 root 统一门禁日志为准。
- **真实数据库运行未验证**。静态 token 和 Port 替身定义不作为真实 Mongo 事务回滚、并发或未知提交恢复证据。

## 6. 集成登记

root/E 负责注册 `erp-returns::dto`、`service::{sales_return,purchase_return}`、`erp-read-models::returns_center`、`reverse_flow::{sales_return,purchase_return}`，并完成 Cargo、HTTP、共享根导出接线。四个资金命令 owner 使用 ReadService 写后重读，禁止保留旧详情转发实现。adapter owner 删除已迁入 RM 的展示函数与专属测试。
