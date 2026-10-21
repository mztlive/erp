# 销售询价接口与存储合同

版本：1.0。基准日期：2026-10-21。业务依据：[销售询价业务合同](sales-inquiry-contract.md)。执行与验收依据：[实施计划](sales-inquiry-implementation-plan.md)。状态：接口及存储代码已落盘，检查与业务验收按实施计划登记。

## 1. 通用协议

1. 本文路径均以 `/admin` 为前缀，执行后台 JWT 认证、路由权限和用例内当前数据范围重验。
2. JSON 字段使用 `snake_case`；通用响应信封沿用 `ApiResponse` 的 `status`、`errorMessage`、`code`、`fieldErrors`、`retryable`、`requestId`、`data`、`success`。业务调用方只使用已解包的 `data`。
3. 金额、数量、单价、税率使用十进制字符串。金额最多 2 位有效小数，单价最多 4 位，数量与税率最多 6 位；数量另外受当前单位精度限制。不得使用浮点运算替代现有定点类型。
4. `Instant` 字段使用 Unix 秒级整数；日期字段 `BusinessDate` 使用 `YYYY-MM-DD`。Excel 客户交期按中国标准时间当日 23:59:59 转为时刻。
5. 客户端不得自行指定系统生成的后继版本、状态、实际任务归属或来源余额。本合同允许的预期版本、责任分派申请及采购内部报价输入必须经服务端重验。独立请求 DTO 的 `deny_unknown_fields` 拒绝未定义字段；客户商品快照在服务端按当前资格回填。
6. 写命令携带 `command_key`；修改既有询价另携带聚合 `expected_version`，采购任务写入另携带 `task_id`、`task_version`。同键同载荷恢复原结果，同键异载荷返回冲突。操作结果未知时保留原键和完整载荷，不生成新命令。
7. 询价协作命令键最多 256 字符；Excel 确认命令键为 1 至 128 字节。前端生成短随机键，重试时原样保留。
8. 未明确列出的业务变更继续使用现有销售、采购、审批、财务接口，不建立绕过审批的询价快捷写入。

## 2. 权限与数据范围

| 接口类别 | 权限资源 | 范围判定 |
| --- | --- | --- |
| 销售询价 | `sales_inquiry` | 当前账号、有效角色、负责人及业务组织范围；个人限制继续作为上限 |
| 采购询价 | `procurement_inquiry` | 当前账号及有效动作权限，再校验当前开放任务、责任人、需求版本和责任行 |
| 客户模板商品 | `sales_inquiry:create` 及 `sellable_sku:list` | 全部 SKU 按当前可售资格整体重验 |
| 新增或修改需求商品、Excel 预览及首次确认 | 对应询价动作及 `sellable_sku:list` | 同一有效角色同时授予的范围内操作，再重验 SKU；历史只读及回执恢复不额外要求目录权限 |
| 转销售 | `sales_inquiry:convert_sales` 加现有销售创建资格 | 询价当前范围、已接受版本、数量及地址余额；实际建单沿用销售命令 |
| 询价费用采购 | `purchase_order:create` 及 `procurement_inquiry:convert_expenses` | 保留既有生效销售费用来源范围，再校验明确费用归属和来源额度；不复活已完成任务 |
| 财务直接登记 | `cost_entry:create` 及既有销售读取范围 | 只消费明确分摊的 ERP 外费用来源；不授予供应商及内部付款依据读取 |

1. 采购任务不授予普通销售订单、客户档案或内部文件的完整读取权。提交、补充后重提及重询均由服务端按采购责任配置自动分派，销售无需人员目录权限。分派必须完整覆盖当前需求行，每条需求行恰有一位责任采购。
2. 任务改派、终态、需求版本变化、责任范围损坏、账号或角色失效时，原任务读写立即失败关闭。
3. 销售响应使用独立 `InquirySalesView`。允许需求、已发布反馈、接受、重询、事件、数量来源及公开费用进度；禁止供应商、供给身份、原始采购价、内部附件、未发布采购工作和内部费用收款依据。
4. 采购响应使用独立 `InquiryProcurementView`，只保留当前任务责任行及必要客户和配送事实，剔除销售参考价。部分责任任务不得读取整单无明确行归属的费用。
5. 不存在与越范围采用统一对象不存在语义；商品不存在与当前不可选用采用统一对外语义。
6. 客户模板按请求中的勾选顺序整体校验并返回，不得静默省略失败商品。批量商品校验失败返回参数校验错误并给出失败总数，最多展示前 5 项：不存在或当前不可选用的项仅提示勾选序号；已通过授权及可售资格校验的卡券、缺少单位的商品提示名称、SKU 编码、原因和处理动作，不返回内部供给资料。

## 3. 销售接口

| 方法与路径 | 权限 | 请求 | 响应 `data` |
| --- | --- | --- | --- |
| `GET /sales-inquiries` | `sales_inquiry:list` | §5 分页参数 | `InquiryListView<InquirySalesView>` |
| `POST /sales-inquiries` | `sales_inquiry:create` | `CreateInquiryCommand` | `InquiryMutationView` |
| `GET /sales-inquiries/{id}` | `sales_inquiry:detail` | 询价 ID | `InquirySalesView` |
| `PUT /sales-inquiries/{id}/draft` | `sales_inquiry:update` | `SaveInquiryCommand` | `InquiryMutationView` |
| `POST /sales-inquiries/{id}/submit` | `sales_inquiry:submit` | `SubmitInquiryCommand` | `InquiryMutationView` |
| `POST /sales-inquiries/{id}/accept` | `sales_inquiry:accept` | `AcceptInquiryCommand` | `InquiryMutationView` |
| `POST /sales-inquiries/{id}/reinquire` | `sales_inquiry:reinquire` | `ReinquireInquiryCommand` | `InquiryMutationView` |
| `POST /sales-inquiries/{id}/close` | `sales_inquiry:close` | `InquiryReasonCommand` | `InquiryMutationView` |
| `POST /sales-inquiries/template-products` | `sales_inquiry:create` | `{sku_ids: string[]}`，1 至 1000 个唯一 ID | 客户安全商品数组 |
| `POST /sales-inquiries/imports/preview` | `sales_inquiry:create`、`sales_inquiry:import`、`sellable_sku:list` | §7.3 客户及文件 multipart | `InquiryImportPreviewView` |
| `POST /sales-inquiries/imports/create` | 同上 | `CreateInquiryImportCommand` | `InquiryImportResult` |
| `POST /sales-inquiries/{id}/imports/preview` | `sales_inquiry:import` | §7 multipart | `InquiryImportPreviewView` |
| `POST /sales-inquiries/{id}/imports/confirm` | `sales_inquiry:import` | §7 确认命令 | `InquiryImportResult` |
| `GET /sales-inquiries/{id}/sales-basis?round=...` | `sales_inquiry:convert_sales` | 已接受反馈轮次 | `InquirySalesBasis` |
| `GET /sales-inquiries/{id}/fee-sales` | `sales_inquiry:detail` | 询价 ID | `{id,order_no,status_label,eligible_for_fee_allocation}[]` |
| `GET /sales-orders/{id}/inquiry-fees` | `cost_entry:create` 及既有销售读取范围 | 生效销售 ID | 财务专用询价费用候选，不含供应商或付款依据 |
| `POST /sales-inquiries/{id}/fee-allocations` | `sales_inquiry:convert_sales` | `AllocateInquiryFeesCommand` | `InquiryMutationView` |
| `GET /sales-inquiries/receipts/{command_key}` | `sales_inquiry:recover` | 原创建键 | 原 `InquiryMutationView` 或 `null` |
| `GET /sales-inquiries/{id}/receipts/{command_key}?action=...` | `sales_inquiry:recover` | 原动作及原命令键 | 原 `InquiryMutationView` 或 `null` |

### 3.1 命令字段

| DTO | 字段 |
| --- | --- |
| `CreateInquiryCommand` | `command_key`、`data: InquiryCreateData` |
| `CreateInquiryImportCommand` | `command_key`、`customer_id`、`preview_id`、`input_fingerprint`、`confirm_merge_duplicates` |
| `InquiryCreateData` | `customer_id`、`customer_name`、`sales_owner_user_id`、`business_org_unit_id`、`deadline_at`、可空 `copied_from_inquiry_id`、`draft: InquiryRequirementData` |
| `SaveInquiryCommand` | `command_key`、`expected_version`、可空 `deadline_at`、`data: InquiryRequirementData` |
| `SubmitInquiryCommand` | `command_key`、`expected_version`、`reason` |
| `AcceptInquiryCommand` | `command_key`、`expected_version`、`feedback_round` |
| `ReinquireInquiryCommand` | `command_key`、`expected_version`、`data` |
| 重询 `data` | `reason`、`line_ids`、`fee_ids`、`concerns`、可空完整 `requirement`；不传新版需求表示仅重询条件 |
| `InquiryReasonCommand` | `command_key`、`expected_version`、`reason` |
| `AllocateInquiryFeesCommand` | `command_key`、`expected_version`、1 至 100 项 `allocations` |
| 费用归属项 | `allocation_id`、`fee_id`、`sales_order_id`、`feedback_round`、十进制 `amount` |
| `InquiryMutationView` | `inquiry_id`、聚合 `version`、`requirement_version`、`feedback_round`、`status`、`task_ids` |

`customer_name`、负责人组织及商品字段必须由命令服务按当前主数据证明；请求带有显示快照不代表客户端可以任意改写主数据身份。

提交和重询命令不得携带 `assignments` 或 `shared_fee_owner_user_id`，传入即按未知字段拒绝。服务端在原写事务内按 SKU、商品分类（从当前分类逐级到父分类）、商品类型、默认调度人的优先级解析当前完整需求，再核验命中账号、主属组织及询价处理权限。同一采购的商品合并为一个任务，禁止漏行或重复责任。规则缺失、冲突或负责人失效时拒绝整个提交，不创建无主任务，不向销售开放人员目录作为补救。

整单费用优先沿用仍在本轮责任组内的原负责人；没有有效原负责人时，由首个需求商品的责任采购负责。该顺序取自当前需求行，不使用数据库返回顺序或任务创建顺序。采购侧改派仍核验目标资格，并按原规则同步费用责任。

### 3.2 需求字段

1. `InquiryRequirementData` 包含 `lines` 和 `note`。需求说明最多 4000 字；提交时至少 1 条需求，最多 200 条。
   草稿及待销售补充状态允许同时修改询价截止时间和需求说明。`SaveInquiryCommand.deadline_at` 未传或为 `null` 时保留原值，传 `0` 时清空；传入值与需求草稿同事务保存，版本冲突或需求校验失败时均不修改。已提交需求快照保持不变。页面通过页头“修改需求说明”弹窗编辑这两个字段，“应用修改”仅更新本地草稿，随“保存需求”提交；取消弹窗不修改草稿。
2. 每条需求包含 `line_id`、`sku_id`、`sku_revision_id`、`sku_code`、`product_name`、`specification`、`unit_code`、`quantity_scale`、十进制 `quantity`、`fulfillment_kind`、`delivery_due_at`、`delivery_terms`、可空 `sales_reference_price_gross`、`allocations`。
3. `fulfillment_kind` 为 `physical`、`virtual`、`service`。同 SKU 不同交期或交付条件必须保存为不同稳定需求行；不得只按 SKU 合并。
4. 配送项包含 `allocation_id`、`recipient`、`phone`、`region_code`、`region_name`、`address`、十进制 `quantity`。地区必须落到有效叶节点，名称由服务端冻结。每条需求最多 100 个地址。
5. 实物提交时必须有完整地址，地址数量合计不得少于商品需求数量，允许超量提交；创建或保存草稿允许暂未录入地址，也允许地址分配合计不足或超过需求数量。创建、保存、提交及需求重询均保留销售填写的需求与分配数量，不自动调整。草稿仍校验已填地址的完整性、数量正值、单位精度、地址上限及既有占用下限；提交时重新校验地址完整性和分配下限。超量不扩大报价供量及转单的商品需求额度。
6. 换 SKU 必须新建稳定需求行。已有来源占用或已办理事实下，不得通过重写行或地址身份重置余额。
7. 询价聚合持久化 BSON 上限为 14 MiB，保留 MongoDB 16 MiB 文档限制的安全空间。仓储在创建、CAS 更新前按实际 BSON 编码大小检查；超限时拒绝本次写入并提示复制新询价继续，不截断已发布历史、来源或台账。

### 3.3 销售进度与复制

1. 销售列表和详情返回 `sales_progress`：`transfer_status` 为 `not_started`、`partial`、`complete`；`total_lines` 与 `completed_lines` 按当前稳定需求行计数，数量不跨单位相加，草稿占用与已消耗均计入转单进度。
2. `quote_validity` 表示当前需求版本最新完整反馈的商品报价有效性，为 `unavailable`、`valid`、`partially_expired`、`expired`，独立于处理状态；`can_create_sales` 按已接受范围、重询、关闭、有效期和剩余数量判定，不替代转销售及销售创建权限校验。
3. 关闭询价后可使用“复制为新询价”。新询价保存 `copied_from_inquiry_id`，需求行及地址重新生成身份；不得复制反馈、接受记录、费用台账或数量占用。截止时间重新填写，商品资格及原询价读取资格由创建事务重验。
4. 已提交的收件资料只读；无有效销售占用时通过“替换地址”建立新身份。有占用的地址先处理下游，或新增地址分配未转数量，不改写历史收件资料。

## 4. 采购接口

| 方法与路径 | 权限 | 请求 | 响应 `data` |
| --- | --- | --- | --- |
| `GET /procurement-inquiries` | `procurement_inquiry:list` | §5 分页参数 | 当前任务范围的采购列表 |
| `GET /procurement-inquiries/{id}?task_id=...` | `procurement_inquiry:detail` | 当前任务 ID | `ProcurementInquiryDetail`，附安全任务身份和版本 |
| `POST /procurement-inquiries/{id}/claim` | `procurement_inquiry:claim` | `InquiryTaskCommand` | `InquiryMutationView` |
| `POST /procurement-inquiries/{id}/supplement` | `procurement_inquiry:supplement` | `InquiryTaskReasonCommand` | `InquiryMutationView` |
| `POST /procurement-inquiries/{id}/reassign` | `procurement_inquiry:reassign` | `ReassignInquiryCommand` | `InquiryMutationView` |
| `PUT /procurement-inquiries/{id}/feedback` | `procurement_inquiry:save_feedback` | `SaveInquiryFeedbackCommand` | `InquiryMutationView` |
| `POST /procurement-inquiries/{id}/publish` | `procurement_inquiry:publish` | `InquiryTaskCommand` | `InquiryMutationView` |
| `GET /procurement-inquiries/{id}/internal-quotes?task_id=...` | `procurement_inquiry:internal_quote` | 当前任务 ID | 当前责任行内部报价 |
| `POST /procurement-inquiries/{id}/internal-quotes` | `procurement_inquiry:internal_quote` | `SaveInquiryQuoteCommand` | `InquiryMutationView` |
| `GET /procurement-inquiries/{id}/fee-bases?task_id=...&feedback_round=...` | `procurement_inquiry:internal_quote` | 当前任务及明确轮次 | 当前责任费用内部依据 |
| `POST /procurement-inquiries/{id}/fee-bases` | `procurement_inquiry:internal_quote` | `SaveInquiryFeeBasisCommand` | `InquiryMutationView` |
| `GET /procurement-inquiries/{id}/internal-quotes/{quote_id}/attachments/{asset_id}?task_id=...` | `procurement_inquiry:internal_quote` | 当前任务、准确报价版本及文件身份 | 受保护的附件响应 |
| `GET /procurement-inquiries/{id}/receipts/{command_key}?action=...` | `procurement_inquiry:recover` | 原动作及原命令键 | 本人原安全回执或 `null` |
| `GET /purchase-orders/expense-sources/{id}/inquiry-fees` | `purchase_order:create` 及 `procurement_inquiry:convert_expenses` | 生效销售 ID | 本销售明确分摊的费用余额、不可用原因及必要收款依据 |

1. `InquiryTaskCommand` 为 `command_key`、`expected_version`、`task_id`、`task_version`。退回命令增加 `reason`，改派命令增加 `owner_user_id`、`reason`。
2. 客户端保存本地反馈时同时保存当前需求责任及本人已保存工作内容作为编辑基准；他人独立责任更新仅同步命令版本，不覆盖输入。本人工作内容发生变化时必须展示当前保存内容并由用户确认后继续编辑。结果未知的命令继续使用原键及原载荷，禁止据新版本重写原命令。
3. 保存反馈增加 `data: InquiryFeedbackData`，仅接受当前责任行。发布请求不得提交拼接后的整单反馈；服务端从仍有效的各责任采购工作片段合成并重验完整结果。
4. 最终供给结论 `conclusion` 为 `available`、`partially_available`、`unavailable`、`pending`。存在 `pending`、遗漏需求行或地址供量不守恒时禁止发布。
5. 每个公开价格段保存 `price_id`、可供 `quantity`、`unit_price_gross`、`tax_rate`、`delivery_due_at`、`valid_until`、`min_quantity`、可空 `max_quantity`、`allow_cumulative`、地址额度及 `included_cost_note`。公开价格段不等于实际采购采用报价。
6. 费用保存稳定 `fee_id`、类别、名称、含税金额和税率、相关需求行/地址、`included_in_goods_price`、`confirmation`、`requires_erp_payment`、有效期、适用条件及公开说明。
7. `confirmation` 为 `estimated` 或 `confirmed`。暂估、已含商品价和未接受费用不得直接形成费用采购。
8. `SaveInquiryQuoteCommand` 包含通用任务命令字段及 `quote:{quote_id?,revision_no,line_id,supplier_id,supplier_offering_id,terms,internal_note,attachment_ids}`。`terms` 明确实际采购成本、税率、起订量、供量、稳定地址限制、交付日、有效日期、付款条件和是否允许累计批次。询价/需求版本、SKU 和登记人由服务端赋值，不接受客户端注入。
9. `SaveInquiryFeeBasisCommand` 包含通用任务命令字段及 `basis:{fee_id,payee_supplier_id,payment_term_code,internal_note,attachment_ids}`。每轮费用依据独立冻结，不产生采购、应付或付款授权。
10. 附件在存储读取前后均重新校验当前任务、准确报价附件关系及文件治理状态，禁止公开存储键和公共缓存；不得通过销售接口读取内部报价 DTO 或下载内部附件。

## 5. 分页与授权版本

1. 两类列表接受 `q`、`customer_id`、`status`、`page`、`scope_version`；单页固定 20 项，页码为 1 至 500，搜索最多 100 字。
2. 响应为 `{items,total,page,page_size,scope_version}`。第一页取得 `scope_version`，第二页及以后必须回传；任何显式版本与当前版本不符均拒绝。
3. 销售版本包含当前授权范围；采购版本另外包含当前责任任务身份、版本和需求版本。范围变化返回 `DATA_SCOPE_CHANGED`，调用方清空分页并从第一页重新读取，不合并旧范围缓存。
4. 页面后台刷新只更新服务器快照。用户编辑副本、待确认文件和未确认命令须按账号、询价及任务隔离保存，不随查询刷新重置。

## 6. 状态与版本

| 动作 | 允许前态 | 后态 | 版本及任务行为 |
| --- | --- | --- | --- |
| 创建 | 无 | `draft` | 生成稳定询价身份及独立命令回执 |
| 保存草稿或确认导入 | `draft`、`awaiting_sales_supplement` | 原态 | 聚合 CAS 更新；不覆盖已提交快照 |
| 提交 | `draft`、`awaiting_sales_supplement` | `awaiting_procurement` | 需求有变化才追加需求版本；完整替换责任任务 |
| 接手 | `awaiting_procurement`、`procurement_processing` | `procurement_processing` | 核对当前任务责任及版本 |
| 退回补充 | `procurement_processing` | `awaiting_sales_supplement` | 冻结采购工作并关闭当前任务 |
| 保存采购工作 | `procurement_processing` | 原态 | 只替换本人的当前责任工作片段，旧片段冻结 |
| 发布 | `procurement_processing` | `awaiting_sales` | 追加反馈轮次；冻结工作并完成任务 |
| 销售接受 | `awaiting_sales` | `confirmed` | 记录确切需求版本与反馈轮次 |
| 重询 | `awaiting_sales`、`confirmed` | `awaiting_procurement` | 仅价格变化不增加需求版本；需求变化追加版本；重建任务 |
| 关闭 | 除 `closed` 的有效状态 | `closed` | 保留全部历史；冻结工作并关闭任务 |

1. 聚合 `version` 用于并发控制，`requirement_version` 用于需求快照，`feedback_round` 用于已发布反馈，三者不得混用。
2. 已提交需求、已发布反馈、接受和来源引用保持历史事实。重新询价暂停受影响未转部分，既有销售或采购不被新轮次覆盖。局部重询及新反馈尚未接受期间，最近已接受轮次中未受影响的范围继续按原条件及余额使用；所有尚未接受重询的受影响范围合并阻断。销售接受新轮次后，旧轮次不得再用于新占用或增额。需求版本变化使旧需求版本的接受结果全部失效。
3. 提交、发布、接受、重询、关闭在同一事务内处理业务聚合、工作项、审计及回执。

## 7. 客户 Excel

### 7.1 工作簿格式

| 项目 | 首版约束 |
| --- | --- |
| 文件 | `.xlsx`；Excel MIME 或 `application/octet-stream`，不接受其他类型 |
| 原文件 | 非空，最多 5 MiB；multipart 总体上限额外保留 64 KiB 协议空间 |
| ZIP | 最多 4096 项；单项解压最多 16 MiB，总解压最多 32 MiB；拒绝不安全路径、重复项、宏、外部链接和实体声明 |
| 模板版本 | `填写说明!A1=模板版本`；询价详情模板 `B1=sales-inquiry-v1`，公司商品池导出 `B1=sales-inquiry-pool-v4`；已发出的 `sales-inquiry-pool-v1`、`sales-inquiry-pool-v2`、`sales-inquiry-pool-v3` 文件继续按各自版本读取 |
| 工作表 | 询价详情模板为 `商品清单`、`配送需求`、`填写说明`；公司商品池导出为 `选品`、`配送需求`、`填写说明`，均恰好三张，版本必须匹配 |
| 数量限制 | 商品清单最多 2000 数据行、配送需求最多 2000 实际填写行（不计未使用的备用行）；分组后的需求最多 200 条，每条需求最多 100 个地址 |
| 文本身份 | SKU 编码、电话必须为文本单元格，保留前导零；业务字段除 v2 及以后版本的系统商品联动公式外，其他公式须先转换为值；v3/v4 数量核对辅助列按第 12 项直接丢弃 |

公司商品池导出合同：

1. 原有导出按钮及确认流程保留，不再提供独立客户模板按钮；表格未勾选时的当前结果导出也生成同一格式 XLSX。
2. 「选品」第 1 行为说明，第 2 行为表头，第 3 行起为商品。原 14 列全部保留：主图、商品名称、规格、SKU 编号、SPU 编号、一件代发价（含税）、集采价（含税）、市场价（含税）、集采起订量、供应商编号、供应保障、商品类型、基础单位、条码。追加需求数量、销售参考价、需求备注、询价说明和隐藏的单位编码、配送关联列。
3. 销售自行处理内部信息后发送客户；允许删除展示列和调整列顺序。v2 及以后版本的商品名称、SKU 编号、SPU 编号、单位编码、配送关联、需求数量、销售参考价、需求备注必须保留，表头不得重复或新增未知列。解析只提取需求白名单；供应商、供应保障及三种展示价格不得进入询价需求或反馈，销售参考价仅取明确填写的同名列。
4. 仅需求数量非空的商品进入校验；空数量不是零数量，填写零或非法数量仍须报错。v2 及以后版本 配送页按选品需求数量自动显示商品名称、SKU 编号、SPU 编号和配送数量；未选用且无任何手填配送内容的行忽略，旧版只预填 SKU 而其余列为空的行忽略；填写部分地址或数量的行必须完整校验，不得静默丢弃。错误保留「选品」页原始行号及列名。
5. 卡券保留在导出清单并标明不适用询价；未选用卡券不影响其他商品导入，填写卡券需求则拒绝确认。
6. 每份选品清单最多 2000 商品，客户实际需求分组上限仍为 200 条。图片按缩略图处理；生成文件必须满足 5 MiB 导入上限，超限拒绝并提示减少商品。解析允许最多 4096 个压缩条目以容纳图片，但解压总量仍不得超过 32 MiB，单条目不得超过 16 MiB。

7. v2 及以后版本 选品页使用 `InquirySelection` 表和隐藏的唯一配送关联键。每个可询价商品默认对应一个自动配送行，公式按关联键读取选品资料及需求数量；改变数量或整表排序不得将已有地址转移到另一商品。卡券不生成自动配送行。
8. v3/v4 每个商品默认一个配送行，配送数量默认承接全部需求数量。多地址须复制该商品整行到页末，并将原行及新增行的数量分别改为实际分配值。各行分配合计须等于需求数量。取消选品时只清空自动列，保留手填地址并在导入中报错，禁止静默丢弃已填资料。
9. v2 及以后版本 允许配送页商品名称、SKU 编号、SPU 编号、配送数量四列使用系统生成的 `IF`、`INDEX`、`MATCH` 固定格式公式。导入必须从选品页当前单元格重新读取并按关联键计算，不得信任公式缓存或执行任意表达式。任意改写公式、缺失或重复关联、同一配送行的多种关联混用、手填 SKU 与自动列关联不一致时必须报错。复制后的手填数量保留为明确分配值。
10. 导出设置打开时重新计算；未填写数量的商品在对应配送行留空，固定行不随选品动态压缩。销售可筛选配送页 SKU 编号非空的行填写。删除供应商等展示列不得破坏自动引用；保留自动列依赖的必需列。

11. v3/v4 配送页追加需求数量、已分配数量、剩余数量、分配状态四列。按 SKU 汇总全部地址分配，核对范围至第 10000 行，数量按六位小数核对；超量标红并显示超出数量，不足标黄并显示尚差数量，一致标绿并显示数量一致。颜色同时作用于配送数量与核对区，不得只靠颜色传递结果。绿色只表示数量一致，不表示地址完整或通过导入校验。非法数量须显示错误，不得默认为零。
12. v3/v4 核对列只作填写提示。导入保留表头和列范围校验，直接丢弃辅助列中的值、公式及缓存，不解析或执行其表达式，不要求公式文本与导出时逐字一致；普通公式、共享公式及 WPS/Excel 保存改写不得影响业务导入。商品、SKU、配送数量等业务字段仍执行既有公式与数据校验，并由领域规则按当前商品数量精度、配送明细执行守恒和地址完整性校验。不得用表内绿色状态作为确认依据。

13. v4 在配送页增加隐藏的核对数量、核对商品列。每行先将本行配送数量转换为数字并校验正数及最多六位小数，再用 `SUMIF`、`COUNTIFS` 普通条件汇总；不得依赖跨行 `IF` 数组求值。汇总键须保留 SKU 前导零并按字面匹配通配符。复制地址须通过行号复制整行，连同隐藏核对列一起保留。复制两行各 100、需求 100 时，已分配须为 200、剩余须为 -100，并显示红色超出 100。

商品清单列顺序：`SKU编码`、`商品名称`、`规格`、`单位`、`需求数量`、`销售参考价`、`需求备注`。

询价详情模板及商品池 v1 的配送需求列顺序：`SKU编码`、`收件人`、`联系电话`、`省`、`市`、`区县`、`详细地址`、`配送数量`、`要求交付日期`、`交付条件`。

v2 配送需求列顺序：`商品名称`、`SKU 编号`、`SPU 编号`、`收件人`、`联系电话`、`省`、`市`、`区县`、`详细地址`、`配送数量`、`要求交付日期`、`交付条件`。商品名称和 SPU 仅辅助填写，不作为商品匹配身份。v3 在上述十二列后依次追加 `需求数量`、`已分配数量`、`剩余数量`、`分配状态`。v4 继续追加隐藏的 `核对数量`、`核对商品` 两列。

1. 商品清单同 SKU 只允许一行，单位填写公司单位编码。名称、规格采用当前公司快照，不能按客户名称猜测匹配或创建新主数据。
2. 配送行按 SKU、明确交期和交付条件分组成需求行。全部配送数量合计必须等于商品清单需求数量。
3. 同组内收件人、电话、地区代码和详细地址相同的行在预览中展示合并行号和合计数量；必须销售明确确认，不静默丢弃。
4. 非实物仍填写明确交期与数量；全部收件及地址字段留空时不生成配送分配。不得为其推断默认交期或伪造实物地址。
5. 省、市、区县须唯一匹配内置地区名称或代码；实物缺失地址、错误地区、非法精度均定位工作表、原行号及列名。
6. 客户销售参考价只作为销售参考；导入不接受供应商、内部报价、采购确认价、内部附件或任务责任字段。
   卡券不属于首版询价范围；即使商品池中可售，也不进入导入资格事实，预览与确认均不能将其转换为普通虚拟商品。
7. 导出对文本使用字符串单元格，禁止把客户文字作为公式写出。

### 7.2 已有询价的预览和确认

1. 预览 multipart 只提交 `expected_version` 与一个 `file`。服务器先证明当前询价访问资格及版本，再有界解析文件。
2. 返回 `{preview_id,template_version,expected_version,input_fingerprint,errors,merges,data,expires_at}`。`errors` 项为 `{sheet,row_number,column,message}`，`merges` 项为 `{sheet,row_numbers,sku_code,merged_quantity}`。
3. 预览绑定询价、当前账号、聚合版本及原文件 SHA-256，保留 1 小时；TTL 只清理临时预览。原文件不上传至公共存储。
4. 确认提交 `{command_key,expected_version,preview_id,input_fingerprint,confirm_merge_duplicates}`。确认不接受自由修改后的需求行，防止替换已校验预览。
5. 存在错误或重复行未明确确认时，确认整份失败。修改或移除错误行后必须重新上传预览，不允许部分正式写入。
6. 在写事务内重新校验当前授权、预览绑定、过期、需求版本、SKU 当前修订、单位及可售资格，再保存草稿、确认结果、永久回执和审计。
7. 成功返回 `{inquiry_id,preview_id,version,line_count,allocation_count}`。同一命令键重复确认在当前询价权限内恢复原结果，不依赖预览仍存在。
8. 确认使用当前权限政策版本绑定的写事务。提交未知、回执唯一竞争或瞬态事务冲突后只读取原回执并重新核对身份、指纹和当前访问资格，不自动重放写事务；软删除或损坏回执拒绝恢复，不当作可重新建单的空结果。

### 7.3 导入创建询价草稿

1. 销售询价列表的“导入客户选品”弹窗仅要求选择客户并上传一个工作簿。调用 `POST /sales-inquiries/imports/preview`，multipart 恰好包含 `customer_id` 和 `file`；拒绝缺失、重复或额外字段。客户字段为非空 UTF-8 文本，最多 128 字节；文件限制同 §7.1。
2. 预览前及确认事务内均证明当前账号的创建、导入与商品池读取权限，同一角色须同时承载三项权限；客户须在当前访问范围内且有效。询价负责人及业务组织由当前账号确定，客户名称取当前主体修订，不接受客户端覆盖。
3. 预览返回结构同 §7.2，`expected_version=0`，绑定当前账号、客户、文件 SHA-256 及新建用途，有效期 1 小时。该阶段只保存临时预览，不创建询价。切换客户或修改文件必须重新预览；新建预览与已有询价预览不得混用。
4. 用户点击“创建询价草稿”后，客户端先生成预览；无错误且无重复配送行时自动调用 `POST /sales-inquiries/imports/create`。存在错误时展示逐行问题，存在重复配送行时先展示合并结果并要求用户明确确认。创建只提交 `CreateInquiryImportCommand`。服务端重验预览、当前权限及客户、商品修订与资格；错误未清除、重复配送行未经明确确认、预览过期或已用于其他创建命令时整份拒绝。
5. 询价草稿、完整需求与地址、预览处理结果、永久命令回执及审计必须在同一授权事务内写入。询价截止时间采用既有未设置值 `0`，不得以客户商品交期推断询价截止时间。成功返回 `InquiryImportResult`，客户端打开对应草稿详情。
6. 同键同载荷重试在当前客户及询价访问资格内返回首次结果，预览过期或清理后仍可恢复；同键异客户、异文件、异预览或异合并确认必须拒绝。结果未知时保留原命令并按原载荷核对，不生成新命令键绕过恢复。事务异常及回执恢复规则同 §7.2。
7. 临时预览新增可选 `creation_customer_id`，旧记录缺省为已有询价预览。无需批量迁移存量数据或变更索引；回滚时移除新建入口与接口，待新建预览按现有 TTL 清理，保留已创建询价及永久回执。

## 8. 销售、采购与费用来源

1. 实际转销售使用现有 `POST /sales-orders`，增加可空 `inquiry_source`；包含 `inquiry_id`、`feedback_round`、各销售行对应的 `inquiry_line_id`、`price_id`、数量及稳定地址分配。成交价由销售明确录入。
2. 创建销售草稿与来源占用同事务。来源保存确切需求版本、反馈轮次、SKU 修订及原地址映射；已有销售改版不得反向覆盖询价历史。
3. 草稿编辑按来源规则核对客户、SKU、交期及地址；减量调整预留。撤回、驳回保留占用，草稿作废释放，首次生效转为已消耗；已消耗事实不因正式销售变更而重置。
4. 正式采购来源 `PurchaseOfferingSource.inquiry_quote` 为可空专项条款快照，精确保存询价、需求行、报价稳定身份和修订、供给成本税率、数量、地址、交期、期限及付款条件。
5. 实际选源继续使用现有采购创建依据及 sourcing 命令。库存足额、直发、入仓、手动选源和推荐沿用原流程；专项候选只能由服务端在精确来源范围生成，不进入其他客户通用推荐。
6. 专项采用与采购、审批和报价累计余额同事务；低于起订量、过期或范围不符必须阻断，不得以整条销售数量虚构累计采购承诺。
7. 费用先通过 `fee-allocations` 对本询价关联的生效销售明确归属。稳定费用行跨反馈轮次累计，分配合计不得超过已接受额度，也不得低于已有有效办理占用。
8. 费用采购复用现有来源、预览、创建、提交和审批接口，在费用行携带可空 `inquiry_source:{inquiry_id,feedback_round,fee_id,allocation_id}`。服务端核对接受、确认状态、含价标记、类别税率、收款供应商、付款条件及归属销售。确认创建后立即提交审批；采购、来源占用、提交审批和回执在同一用例事务中完成，草稿仅为内部中间状态。
9. ERP 外费用沿用财务额外费用登记，增加同形窄来源引用。`requires_erp_payment=false` 的费用不得生成费用采购；财务登记和费用采购消费同一费用余额，禁止形成第二笔应付。
10. 专项改选原因、正式采购变更复验、累计承诺及完整界面操作的交付状态按实施计划证据记录；缺少证据不得视为本节已通过业务验收。

### 8.1 费用候选和额度生命周期

1. 采购候选返回 `{items:[{inquiry_id,inquiry_no,feedback_round,fee_id,allocation_id,name,category,tax_rate,available_amount,valid_until,valid_conditions,unavailable_reason,payee_supplier_id?,payment_term_code?}]}`。候选不可用时保留具体原因，不能据此绕过创建时重验。已接受费用尚无内部收款依据时，允许在初次转换或明确采用新轮次、增加金额时选择合格供应商及付款条件，并在原事务首次追加该轮不可变依据；已有依据必须严格匹配，不得覆盖。
2. 财务候选使用独立 DTO，仅保留公开费用身份、金额、税率和办理原因，不返回收款供应商和付款条件。`requires_erp_payment=true` 的费用禁止直接财务登记。
3. 采购费用稳定询价、费用行和分摊身份不得替换。允许显式将 `feedback_round` 更新为当前已接受轮次，并重新核对该轮金额、税率、类别、收款对象及付款条件；不能通过新轮次重置已有有效占用。
4. 财务费用登记以完整规范化业务请求和真实凭证内容 HMAC 形成独立强类型回执。同一命令键、同一业务输入及凭证内容允许更换临时文件引用、文件资产 ID 或存储对象键后恢复原结果；业务字段或凭证内容变化时拒绝恢复。旧事实保持可读；历史缺少完整回执的旧命令键重试失败关闭。更换命令键不属于原命令恢复，仍须核验当前来源额度。

| 业务事件 | 来源额度处理 |
| --- | --- |
| 确认创建费用采购并立即提交审批 | 由明确分摊额度转为 `reserved`，采购、审批及来源同事务 |
| 提交、驳回、撤回及重提 | 保留原有效占用，不释放给第二笔费用采购 |
| 未发生且无付款事实的金额减少 | 保留稳定身份后减少占用；单纯减少或保持原额不因原报价过期而阻断 |
| 金额增加或采用新接受轮次 | 重新核对当前接受、费用状态、金额税率、收款依据和余额；超过来源或归属上限拒绝 |
| 确认发生或财务直接登记 | 转为 `handled`，不再次扣减第二次；禁止释放已办理事实 |
| 费用采购业务允许作废 | 仅释放未办理且无受保护付款事实的来源，保留历史回执及单据 |
| 已发生费用减少 | 拒绝；已确认发生或已财务登记的费用来源保持 `handled` |
| 部分付款后的费用减少 | 付款继续按采购单核销，不虚构逐费用已发生金额；正式减额同事务重验采购单净已付款及净已收票下限，通过后只减少未发生费用来源占用，不自动释放额度 |

费用来源的额度状态不替代采购审批、发生确认、付款或财务凭证状态；实际业务规则继续以原单据为准。

## 9. 错误合同

| HTTP | `code` | 适用范围 | 调用方动作 |
| --- | --- | --- | --- |
| 400 | `INVALID_REQUEST` | 非法字段、格式、模板、数量精度、缺少明确确认 | 修正输入；保留编辑副本 |
| 401 | `UNAUTHENTICATED` | 会话失效 | 重新认证 |
| 403 | `PERMISSION_DENIED` | 当前动作资格失效 | 停止动作，不以历史参与替代权限 |
| 404 | `NOT_FOUND` | 不存在或不在当前范围的询价、任务、预览、来源 | 停止读取；不区分存在性 |
| 409 | `CONFLICT` | 聚合或任务版本过期、同键异载荷、来源状态或预览绑定变化 | 保留输入并重新读取；相同未知命令沿原键核对 |
| 409 | `DATA_SCOPE_CHANGED` | 列表范围版本变化 | 丢弃旧分页，从第一页读取 |
| 422 | `BUSINESS_RULE_BLOCKED` | 起订量、有效期、余额、状态等业务条件不满足 | 展示服务端原因并修正条件 |
| 500 | `OUTCOME_UNKNOWN` | 提交结果未能确认 | 保留原键和载荷，查询原回执；禁止盲目新建命令 |

1. 正常预览的逐行错误位于成功响应 `data.errors`，不将整份校验结果压成单条 HTTP 文案。
2. 异键并发仍必须依靠原事务和聚合 CAS 防止超转；HTTP 回执或替身单测不能代替真实数据库并发、回滚和未知提交验证。

## 10. 存储与索引

| 所有者 | 集合 | 主要约束 |
| --- | --- | --- |
| `erp-sales` | `sales_inquiries` | 稳定 ID、单号唯一；负责人/组织/客户分页索引；需求、反馈、接受及跨版本数量费用台账内嵌聚合 |
| `erp-sales` | `sales_inquiry_command_receipts` | ID 唯一；操作人、动作和键摘要联合唯一；永久强类型结果 |
| `erp-sales` | `sales_inquiry_import_previews` | ID 唯一；`expires_at_date` 为 1 小时到期时刻，TTL 到期清理 |
| `erp-sales` | `sales_inquiry_import_receipts` | ID 为账号、动作及键派生的摘要，唯一；永久保存确认结果 |
| `erp-sales` | `sales_order_inquiry_sources` | 销售单 ID 唯一；询价及反馈轮次追溯；保留冻结需求映射和消耗/释放状态 |
| `erp-supply` | `sales_inquiry_internal_quotes` | ID 唯一；稳定报价 ID 与修订号联合唯一；精确询价/需求/反馈/行范围查询 |
| `erp-supply` | `sales_inquiry_internal_fee_bases` | ID 唯一；询价/需求版本/反馈/费用行联合唯一 |
| `erp-supply` | `sales_inquiry_quote_balances` | 稳定报价余额 ID 唯一，跨修订 CAS |
| `erp-supply` | `sales_inquiry_quote_reservations` | 采用记录 ID 唯一；采购单及报价修订追溯 |
| `erp-finance` | 既有 `finance_command_receipts` | 财务费用登记复用既有回执仓储和唯一索引；保存完整请求及凭证内容指纹，恢复确切费用事实 |
| `erp-workflow` | 既有 `work_items` | `SalesInquiryProcessing`；稳定询价、需求版本、当前责任行及现有去重约束 |

1. 需求版本和反馈轮次在单询价聚合中追加，并由聚合 CAS 维护顺序，不另建可覆盖历史版本集合。
2. 索引定义分别位于 `erp-sales/src/indexes` 与 `erp-supply/src/inquiry/indexes.rs`；启动时由既有领域 `ensure` 和组合根依次调用。不得在 HTTP handler 中创建索引或直接访问集合。
3. 新集合及来源字段不要求为历史无来源销售、采购或财务资料伪造询价。可空来源字段使用默认空值，旧单继续沿原流程读取。
4. 索引登记使用固定名称和固定键定义，可重复启动执行。唯一索引冲突必须中止发布并核对事实，不自动删除或合并业务文档。

## 11. 发布、迁移与回退

1. 发布前执行 Rust 全量质量门禁、权限漂移、边界门禁和前端检查，核对生成权限与本合同路由一致。检查结果记录在实施计划，不将未执行项目标为通过。
2. 先发布兼容可空来源的新后端和索引定义，再同步权限生成物、岗位授权及前端入口。新权限不得通过扩大普通销售或客户范围实现。
3. 本次存储采用新集合与兼容可空字段，索引由启动 `ensure` 幂等创建；无须为本合同另行执行手工业务数据迁移或清库命令。
4. 上线执行人记录后端版本、索引结果、权限版本和前端版本；首次业务写入前检查新集合唯一索引存在。自动建索引尚未运行时不得登记“迁移已完成”。
5. 回退先关闭新增提交、转销售、专项选源及费用转换入口，保留已生成单据、来源、余额和历史读取；不得删除询价、来源台账、采购、财务登记或应付。
6. 已产生来源单据后，禁止退回不能识别该来源的旧版本继续写入。采用兼容修复或前向修复；临时预览允许按 TTL 自然过期，永久回执不得随预览清理。
7. 单元测试、静态检查、提交、推送、部署、索引实际创建与业务验收分别记录，任何一项不替代另一项。
