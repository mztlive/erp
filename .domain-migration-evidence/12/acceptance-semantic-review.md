# 阶段 12 验收、DTO 与读模型交付核验合同

## 1. 输入与证据边界

- 输入提交：`0a297ab0`；候选树：`/private/tmp/erp-domain-crate-12-fulfillment`。
- 本分片 10 个原文件副本已逐字节匹配输入提交。原过程 `task.rs` 由 E 分片负责，本记录不将其变更计入 C。
- 静态证据：`/private/tmp/fulfillment12-acceptance-static-evidence.json`；可重复采集：`/private/tmp/check-fulfillment12-acceptance.py`。
- 主集成执行的首轮 workspace check 已编译通过本分片。其报告唯一未使用 Quantity 导入已删除；该清理已定向格式化。
- 本分片未执行 Cargo、Mongo 或历史测试目录入口。新增及保留测试尚待主集成统一执行；真实数据库运行未验证。

## 2. 唯一导出合同

| 接口 | 责任与执行约束 |
| --- | --- |
| `erp_fulfillment::dto::{原单域 DTO}` | 五类创建、更新、过账、列表、详情请求及响应；分页 normalize、SortDir、PageView 保持原合同。 |
| `erp_fulfillment::service::FulfillmentService` | customer_acceptance_list/detail 与原 8 个 validate/build/load/prepare/persist 关联函数。仅持本域 DB。 |
| `erp_fulfillment::service::customer_acceptance::prepare_customer_acceptance_draft` | 接收已分配表头 ID、原时点生成的单号与请求；生成表头后生成行 ID/实体。 |
| `erp_fulfillment::service::customer_acceptance_lines::acceptance_line_specs` | 复用唯一原 DTO→行规格映射与 next_id 时点。 |
| `erp_fulfillment::service::acceptance_eligibility::{EligibilitySources,build_line_eligibilities}` | 只消费本域事实与 AcceptanceSalesLineFact/AcceptanceSalesQuantityFact；三类数量守恒规则唯一。 |
| `erp_read_models::fulfillment_center::FulfillmentReadService::new(Database)` | acceptance_eligibility(&str)、committed_customer_acceptance_view(&str,&SalesOrderId)。 |
| `erp_read_models::fulfillment_center::dto::{CommitCustomerAcceptanceView,EligibleFulfillmentFactView,AcceptanceSalesLineGroupView,AcceptanceEligibilityView}` | 四个跨域响应的唯一实体定义。 |
| `erp_read_models::fulfillment_center::repository::load_customer_acceptance_progress` | `(db, &mut dyn Executor, &SalesOrderId)` 返回原 Result<Option<AcceptanceProgress>>；全部读取复用传入 Executor。 |
| `erp_processes::fulfillment_execution::customer_acceptance::CustomerAcceptanceProcess::new` | 保留 `(Database,FulfillmentService,SharedRbacService,Arc<dyn ApprovalObjectReadPort>)` 四参；内部显式构造跨域 read service。 |
| `CustomerAcceptanceProcess::{create_customer_acceptance,commit_customer_acceptance,post_customer_acceptance,reverse_customer_acceptance}` | 唯一验收根事务；create 新上移，其他三根原实现继续持有。 |

## 3. 字段与测试合同

- 45 个原 DTO/内部分页查询结构均在候选中各有 1 个定义；字段 tokens、derive/serde/validator 属性均一致。仅 qualified type 所有权路径规范化用于对比。
- 21 个本分片原测试均各保留 1 次；原断言与测试职责保留，结构测试仅更新实际生产路径和拆分组合源。
- 新增 3 个生产 helper 回归：重复稳定行后数量覆盖且首次顺序保留；无销售行派生 None；本域三种进度映射为销售三种进度且保持非 ZST Executor 身份。
- 原 completion 测试继续覆盖 Commit/Post/Reverse 的 task/audit/receipt 顺序、每步失败截断，以及 None 不调用销售 writer。

## 4. 变更函数逐项语义核销

| 变更符号 | 实际落点与核验结论 |
| --- | --- |
| `create_customer_acceptance` | `erp-processes/.../customer_acceptance/create.rs` 调用顺序保持 request.validate → 表头 next_id → next_customer_acceptance_no → 新 domain prepare（表头工厂 → 行规格 next_id → 批工厂）→ audit 构造 → 根事务。事务内 register → header/lines write → audit；不新增 task。 |
| `committed_customer_acceptance_view` | `erp-read-models/.../fulfillment_center/mod.rs` 将原 self.customer_acceptance_detail 替换为同 DB 的 domain.customer_acceptance_detail，随后重新执行 acceptance_eligibility。回放保持详情先、资格后。普通 commit 成功仍用事务返回 posted.into() 再计算资格，原分支未调整。 |
| `build_line_eligibilities` | `erp-fulfillment/.../service/acceptance_eligibility.rs` 原数量组织 body 仅将 revision_line.base.id 改为最小 fact.id。RM 显式逐项映射 id、stable line ID、revision ID、quantity；不筛选、不排序、不生成 ID、不读 clock。后出现稳定行覆盖 required_quantity，首次出现顺序不变。 |
| `reverse_customer_acceptance` | `erp-processes/.../customer_acceptance/reverse.rs` 收据回放详情接 self.domain，原查询方法及 DB 相同；receipt 构造/首次查、事务、副作用、失败后二次 receipt 查保持原代码 body。 |
| `apply_projection` | `erp-processes/.../customer_acceptance/completion.rs` None 仍立即返回 false；Some 显式逐项映射三种履约自有状态到销售状态，再调用原 writer 和返回 remaining。无额外余额读取、任务或 clock。 |
| `CustomerAcceptanceProcess::new` | 领域 detail 配置由入参保存为 domain；跨域 read 由同 db.clone() 构造，不引入新配置、密钥、权限缺省或新事务。 |

## 5. 原实现保持与错误、执行器合同

- `load_customer_acceptance_progress` 的销售类型 guard、版本指针/版本/行、数量行、发货/电子/服务、分配读取次序保持；非 GoodsService 原位返回 None，空行由 AcceptanceProgress::derive 返回 None。
- 工作台与事务进度加载都在调用领域规则前过滤 ServiceFulfillment::is_acceptance_eligible；展示排序仍按 line_no 稳定排序。历史读取仍在工作台三类分配查询之后。
- commit 的 task/existing 参数配对 → command receipt 构造/查回放 → 新单号 → 表头/行 ID 保持。普通 post 不新增 command receipt，先草稿守卫再 task。
- commit/post 逐行校验和逐分配 ID 生成仍穿插原写入位置，不预校验后续行；写分配后 mark_posted/update。
- reverse 保留原单/版本/行/分配读取 → reversible guard → 反向头/行/分配 ID → 反向头行 → REVERSE 分配 → 新头 posted/update → 原头 reverse/update。
- completion 顺序保持销售投影 → task → 非 Commit 业务 audit → 非 Post command receipt audit；reverse 仅 remaining=true 时重开任务。
- 所有原 validation/conflict/notfound 文案及 map_err 分类保留。领域 Error 由全局 typed 转换接入 services；没有字符串分类、默认值吞错或新增内部事务。

## 6. 按原符号核验清单

| 原文件 | 符号 | 候选落点 | body 核验 |
| --- | --- | --- | --- |
| `customer_acceptance.rs` | `customer_acceptance_list` | `crates/erp-fulfillment/src/service/customer_acceptance.rs` | qualified path 规范化后一致 |
| `customer_acceptance.rs` | `customer_acceptance_detail` | `crates/erp-fulfillment/src/service/customer_acceptance.rs` | qualified path 规范化后一致 |
| `customer_acceptance.rs` | `create_customer_acceptance` | `crates/erp-processes/src/fulfillment_execution/customer_acceptance/create.rs` | 见第 4 节逐项核销 |
| `customer_acceptance.rs` | `from` | `crates/erp-fulfillment/src/service/customer_acceptance.rs` | qualified path 规范化后一致 |
| `customer_acceptance.rs` | `from` | `crates/erp-fulfillment/src/service/customer_acceptance.rs` | qualified path 规范化后一致 |
| `customer_acceptance.rs` | `from` | `crates/erp-fulfillment/src/service/customer_acceptance.rs` | qualified path 规范化后一致 |
| `customer_acceptance.rs` | `customer_acceptance_create_binding_decision` | `crates/erp-processes/src/fulfillment_execution/customer_acceptance/registration.rs` | qualified path 规范化后一致 |
| `customer_acceptance.rs` | `ensure_customer_acceptance_skips_approval_binding` | `crates/erp-processes/src/fulfillment_execution/customer_acceptance/registration.rs` | qualified path 规范化后一致 |
| `customer_acceptance.rs` | `ensure_customer_acceptance_has_no_adapter` | `crates/erp-processes/src/fulfillment_execution/customer_acceptance/registration.rs` | qualified path 规范化后一致 |
| `customer_acceptance.rs` | `customer_acceptance_binding_organization_id` | `crates/erp-processes/src/fulfillment_execution/customer_acceptance/registration.rs` | qualified path 规范化后一致 |
| `customer_acceptance.rs` | `customer_acceptance_bind_command` | `crates/erp-processes/src/fulfillment_execution/customer_acceptance/registration.rs` | qualified path 规范化后一致 |
| `customer_acceptance.rs` | `apply_customer_acceptance_create_binding` | `crates/erp-processes/src/fulfillment_execution/customer_acceptance/registration.rs` | qualified path 规范化后一致 |
| `customer_acceptance.rs` | `persist_unbound_customer_acceptance_document` | `crates/erp-processes/src/fulfillment_execution/customer_acceptance/registration.rs` | qualified path 规范化后一致 |
| `customer_acceptance.rs` | `register_created_customer_acceptance_document` | `crates/erp-processes/src/fulfillment_execution/customer_acceptance/registration.rs` | qualified path 规范化后一致 |
| `customer_acceptance.rs` | `persist_created_customer_acceptance` | `crates/erp-processes/src/fulfillment_execution/customer_acceptance/create.rs` | qualified path 规范化后一致 |
| `customer_acceptance.rs` | `draft_acceptance` | `crates/erp-processes/src/fulfillment_execution/customer_acceptance/registration.rs` | qualified path 规范化后一致 |
| `customer_acceptance_lines.rs` | `acceptance_line_specs` | `crates/erp-fulfillment/src/service/customer_acceptance_lines.rs` | qualified path 规范化后一致 |
| `customer_acceptance_posting.rs` | `validate_customer_acceptance_task_context` | `crates/erp-fulfillment/src/service/customer_acceptance_posting.rs` | qualified path 规范化后一致 |
| `customer_acceptance_posting.rs` | `build_customer_acceptance_lines` | `crates/erp-fulfillment/src/service/customer_acceptance_posting.rs` | qualified path 规范化后一致 |
| `customer_acceptance_posting.rs` | `load_customer_acceptance_commit_draft` | `crates/erp-fulfillment/src/service/customer_acceptance_posting.rs` | qualified path 规范化后一致 |
| `customer_acceptance_posting.rs` | `prepare_customer_acceptance_commit` | `crates/erp-fulfillment/src/service/customer_acceptance_posting.rs` | qualified path 规范化后一致 |
| `customer_acceptance_posting.rs` | `persist_customer_acceptance_commit` | `crates/erp-fulfillment/src/service/customer_acceptance_posting.rs` | qualified path 规范化后一致 |
| `customer_acceptance_posting.rs` | `load_customer_acceptance_for_post` | `crates/erp-fulfillment/src/service/customer_acceptance_posting.rs` | qualified path 规范化后一致 |
| `customer_acceptance_posting.rs` | `persist_customer_acceptance_post` | `crates/erp-fulfillment/src/service/customer_acceptance_posting.rs` | qualified path 规范化后一致 |
| `customer_acceptance_posting.rs` | `persist_customer_acceptance_reverse` | `crates/erp-fulfillment/src/service/customer_acceptance_posting.rs` | qualified path 规范化后一致 |
| `customer_acceptance_posting.rs` | `committed_customer_acceptance_view` | `crates/erp-read-models/src/fulfillment_center/mod.rs` | 见第 4 节逐项核销 |
| `customer_acceptance_posting.rs` | `load_customer_acceptance_progress` | `crates/erp-read-models/src/fulfillment_center/repository.rs` | qualified path 规范化后一致 |
| `customer_acceptance_posting.rs` | `ensure_existing_acceptance_draft` | `crates/erp-fulfillment/src/service/customer_acceptance_posting.rs` | qualified path 规范化后一致 |
| `customer_acceptance_posting.rs` | `ensure_task_context_pair` | `crates/erp-fulfillment/src/service/customer_acceptance_posting.rs` | qualified path 规范化后一致 |
| `customer_acceptance_posting.rs` | `ensure_post_lines_match` | `crates/erp-fulfillment/src/service/customer_acceptance_posting.rs` | qualified path 规范化后一致 |
| `customer_acceptance_posting.rs` | `write_acceptance_allocation` | `crates/erp-fulfillment/src/service/customer_acceptance_posting.rs` | qualified path 规范化后一致 |
| `customer_acceptance_posting.rs` | `load_fulfillment_fact` | `crates/erp-fulfillment/src/service/customer_acceptance_posting.rs` | qualified path 规范化后一致 |
| `customer_acceptance_posting.rs` | `draft_acceptance` | `crates/erp-fulfillment/src/service/customer_acceptance_posting.rs` | qualified path 规范化后一致 |
| `acceptance_eligibility.rs` | `acceptance_eligibility` | `crates/erp-read-models/src/fulfillment_center/acceptance_eligibility.rs` | qualified path 规范化后一致 |
| `acceptance_eligibility.rs` | `so_line_ids` | `crates/erp-read-models/src/fulfillment_center/acceptance_eligibility.rs` | qualified path 规范化后一致 |
| `acceptance_eligibility.rs` | `build_line_eligibilities` | `crates/erp-fulfillment/src/service/acceptance_eligibility.rs`, `crates/erp-read-models/src/fulfillment_center/acceptance_eligibility.rs` | 见第 4 节逐项核销 |
| `acceptance_eligibility.rs` | `build_eligibility_views` | `crates/erp-read-models/src/fulfillment_center/acceptance_eligibility.rs` | qualified path 规范化后一致 |
| `acceptance_eligibility.rs` | `revision_line` | `crates/erp-read-models/src/fulfillment_center/acceptance_eligibility.rs` | qualified path 规范化后一致 |
| `acceptance_eligibility.rs` | `goods_line` | `crates/erp-read-models/src/fulfillment_center/acceptance_eligibility.rs` | qualified path 规范化后一致 |
| `acceptance_eligibility.rs` | `delivery` | `crates/erp-read-models/src/fulfillment_center/acceptance_eligibility.rs` | qualified path 规范化后一致 |
| `acceptance_eligibility.rs` | `delivery_line` | `crates/erp-read-models/src/fulfillment_center/acceptance_eligibility.rs` | qualified path 规范化后一致 |
| `acceptance_eligibility.rs` | `electronic_delivery` | `crates/erp-read-models/src/fulfillment_center/acceptance_eligibility.rs` | qualified path 规范化后一致 |
| `acceptance_eligibility.rs` | `service_fulfillment` | `crates/erp-read-models/src/fulfillment_center/acceptance_eligibility.rs` | qualified path 规范化后一致 |
| `acceptance_eligibility.rs` | `apply_allocation` | `crates/erp-read-models/src/fulfillment_center/acceptance_eligibility.rs` | qualified path 规范化后一致 |
| `process/commit.rs` | `commit_customer_acceptance` | `crates/erp-processes/src/fulfillment_execution/customer_acceptance/commit.rs` | qualified path 规范化后一致 |
| `process/reverse.rs` | `reverse_customer_acceptance` | `crates/erp-processes/src/fulfillment_execution/customer_acceptance/reverse.rs` | 见第 4 节逐项核销 |
| `process/completion.rs` | `mode` | `crates/erp-processes/src/fulfillment_execution/customer_acceptance/completion.rs` | qualified path 规范化后一致 |
| `process/completion.rs` | `complete_acceptance` | `crates/erp-processes/src/fulfillment_execution/customer_acceptance/completion.rs` | qualified path 规范化后一致 |
| `process/completion.rs` | `finish` | `crates/erp-processes/src/fulfillment_execution/customer_acceptance/completion.rs` | qualified path 规范化后一致 |
| `process/completion.rs` | `refresh_sales` | `crates/erp-processes/src/fulfillment_execution/customer_acceptance/completion.rs` | qualified path 规范化后一致 |
| `process/completion.rs` | `persist_task` | `crates/erp-processes/src/fulfillment_execution/customer_acceptance/completion.rs` | qualified path 规范化后一致 |
| `process/completion.rs` | `business_audit` | `crates/erp-processes/src/fulfillment_execution/customer_acceptance/completion.rs` | qualified path 规范化后一致 |
| `process/completion.rs` | `command_receipt` | `crates/erp-processes/src/fulfillment_execution/customer_acceptance/completion.rs` | qualified path 规范化后一致 |
| `process/completion.rs` | `apply_projection` | `crates/erp-processes/src/fulfillment_execution/customer_acceptance/completion.rs` | 见第 4 节逐项核销 |
| `process/completion.rs` | `write` | `crates/erp-processes/src/fulfillment_execution/customer_acceptance/completion.rs` | qualified path 规范化后一致 |
| `process/completion.rs` | `session` | `crates/erp-processes/src/fulfillment_execution/customer_acceptance/completion.rs` | qualified path 规范化后一致 |
| `process/completion.rs` | `record` | `crates/erp-processes/src/fulfillment_execution/customer_acceptance/completion.rs` | qualified path 规范化后一致 |
| `process/completion.rs` | `refresh_sales` | `crates/erp-processes/src/fulfillment_execution/customer_acceptance/completion.rs` | qualified path 规范化后一致 |
| `process/completion.rs` | `persist_task` | `crates/erp-processes/src/fulfillment_execution/customer_acceptance/completion.rs` | qualified path 规范化后一致 |
| `process/completion.rs` | `business_audit` | `crates/erp-processes/src/fulfillment_execution/customer_acceptance/completion.rs` | qualified path 规范化后一致 |
| `process/completion.rs` | `command_receipt` | `crates/erp-processes/src/fulfillment_execution/customer_acceptance/completion.rs` | qualified path 规范化后一致 |
| `process/completion.rs` | `cases` | `crates/erp-processes/src/fulfillment_execution/customer_acceptance/completion.rs` | qualified path 规范化后一致 |
| `process/completion.rs` | `write` | `crates/erp-processes/src/fulfillment_execution/customer_acceptance/completion.rs` | qualified path 规范化后一致 |
| `process/post.rs` | `post_customer_acceptance` | `crates/erp-processes/src/fulfillment_execution/customer_acceptance/post.rs` | qualified path 规范化后一致 |
| `process/mod.rs` | `new` | `crates/erp-read-models/src/fulfillment_center/mod.rs`, `crates/erp-processes/src/fulfillment_execution/customer_acceptance/mod.rs` | 见第 4 节逐项核销 |

## 7. 源路径核销

以下 5 个旧文件已删除，不保留旧路径转发或第二实现：

- `backend/services/src/fulfillment/acceptance_eligibility.rs`
- `backend/services/src/fulfillment/customer_acceptance.rs`
- `backend/services/src/fulfillment/customer_acceptance_lines.rs`
- `backend/services/src/fulfillment/customer_acceptance_posting.rs`
- `backend/services/src/fulfillment/dto.rs`

已有 process 的 commit/post/reverse/completion/mod 原地接入新接口；新 create/registration 为实际组合实现。所有 include_str 指向候选真实文件，历史 tests/** 未修改。
