# erp-client Dialog 精简执行清单

状态：审查完成；本清单为优化建议，业务代码未修改。审查日期：2026-09-08。

## 1. 范围与证据

已静态扫描 1,389 个生产 TS/TSX 文件，排除测试与依赖目录。索引包含 144 个弹窗承载或调用文件，其中 77 个文件直接承载 Dialog/AlertDialog，共 81 个根节点；另有 31 处 FormalActionConfirmDialog 调用、4 处原生 window.confirm。根节点、复用调用和运行时业务弹窗不是同一个计数口径，不得相加宣称唯一弹窗数。命名包装层、表单框架、复制文案及实际事件处理函数已结合调用链核对；纯重导出不重复计为弹窗。

现场浏览器抽查使用本地已登录 ERP：新建 API 连接、新建品牌。未填写或提交业务表单。库存页当前为 0 个库存组合，无法现场进入库存调整；该项依据代码分析。其他业务状态未逐一触发，不作为运行时验收通过。

仓库 AGENTS.md 指向的 docs/ui-glossary.md 当前不存在，仓库文件搜索未找到同名文件。本次术语判断依据 erp-client/AGENTS.md 第 5 节和当前 lib/ui-text.ts、业务 copy 文件；未创建替代术语表。

## 2. 执行规则

1. 同一事实在同一弹窗仅说明一次；优先保留操作对象、实际金额/数量、状态变化和后果。
2. 表单标签已明确的输入不再用描述重复列举；规则和格式提示放在对应字段旁。
3. 普通提交、草稿编辑不得套用危险操作外观；真正不可恢复的作废、资金冲正、结算影响仍须确认。
4. 一个业务提交只设置一次最终确认；本地可撤销编辑优先采用即时操作与撤销。
5. 精简不得改变审批、版本、生效日期、原因记录、权限、并发校验、重试和结果未知契约。
6. 共享组件先增加明确配置，再逐调用方迁移；不得用一个工作面的措辞覆盖其他页面。

## 3. 修改顺序

P1：先修正错误语义、重复步骤和要求输入内部标识的交互。P2：处理文案、输入结构和条件提示。P3：保留当前合理结构，补充运行时验证。

| 编号 | 优先级 | 执行项 |
| --- | --- | --- |
| D01 | P1 | [审批提交确认去重，纠正危险色语义](#d01) |
| D02 | P1 | [供给分配只保留一次最终确认](#d02) |
| D03 | P1 | [四类退款、冲正合并输入与确认](#d03) |
| D04 | P1 | [移除草稿分配行取消二次确认](#d04) |
| D05 | P1 | [库存调整合并确认并突出调整结果](#d05) |
| D06 | P1 | [结算复核人由输入内部 ID 改为选人](#d06) |
| D07 | P1 | [验收登记避免“确认后再确认”](#d07) |
| D08 | P1 | [业务摘要不得装入“锁定字段”](#d08) |
| D09 | P2 | [API 连接文案改为配置任务语言](#d09) |
| D10 | P2 | [健康检查按环境减少空确认](#d10) |
| D11 | P2 | [主数据新增删除教程式说明](#d11) |
| D12 | P2 | [供应商保存与商品保存按信息量区别处理](#d12) |
| D13 | P2 | [商品原生确认按实际损失分级](#d13) |
| D14 | P2 | [SKU 编辑说明保留保存时点](#d14) |
| D15 | P2 | [添加供给表单分常用与补充信息](#d15) |
| D16 | P2 | [审批决定保留结果，删除核对套话](#d16) |
| D17 | P2 | [审批流程创建只说明来源选择](#d17) |
| D18 | P2 | [权限与删除文案只保留用户后果](#d18) |
| D19 | P2 | [客户与合同新增删除实现说明](#d19) |
| D20 | P2 | [核销入口和反向操作用具体业务名](#d20) |
| D21 | P2 | [导入操作删除任务实现术语](#d21) |
| D22 | P2 | [供应商异常与结算结果说明压缩](#d22) |
| D23 | P2 | [责任配置帮助文字按条件展示](#d23) |
| D24 | P3 | [区分隐藏说明和可见赘述，保留有效轻弹窗](#d24) |

## 4. 逐项要求

<a id="d01"></a>
### D01 · 审批提交确认去重，纠正危险色语义（P1）

**证据：** 采购单等确认框同时显示“草稿→审批中”“确认后启动审批”“内容锁定并进入审批”“按已绑定的审批流程办理”“形成提交并进入审批”。共享组件依据 irreversibleEffects 非空将确认按钮设为 destructive；普通审批提交因此也被标成危险操作。采购、销售另有撤回审批入口，不能把提交审批概括为不可撤回。

**执行要求：** 保留单据、主体、金额、审批路线和审批通过后的实际结果；同一事实只显示一次。说明采用“提交后进入审批，审批通过后生效。驳回后重新提交将从首节点审批。”锁定范围确实影响编辑时保留一句“审批期间不可修改”。移除以“形成提交并进入审批”为内容的不可逆警告。共享组件增加显式动作风险属性，迁移并核对各调用点，禁止靠影响列表是否为空推导危险色。

**验收要求：** 普通提交使用正常主按钮；作废、冲正等实际风险保留对应提示。金额、审批路线不得丢失；撤回、驳回和重提的文案与实际状态一致。

**涉及文件：** [components/business/workflow-actions.tsx](../../erp-client/components/business/workflow-actions.tsx)、[features/purchase-orders/components/purchase-order-submit-confirm-dialog.tsx](../../erp-client/features/purchase-orders/components/purchase-order-submit-confirm-dialog.tsx)、[features/purchase-orders/components/purchase-change-order-submit-confirm-dialog.tsx](../../erp-client/features/purchase-orders/components/purchase-change-order-submit-confirm-dialog.tsx)、[features/sales-orders/components/sales-change-order-submit-confirm-dialog.tsx](../../erp-client/features/sales-orders/components/sales-change-order-submit-confirm-dialog.tsx)、[features/customer-receivables/components/customer-receipt-submit-confirm-dialog.tsx](../../erp-client/features/customer-receivables/components/customer-receipt-submit-confirm-dialog.tsx)、[features/customer-receivables/components/customer-refund-submit-confirm-dialog.tsx](../../erp-client/features/customer-receivables/components/customer-refund-submit-confirm-dialog.tsx)、[features/customer-receivables/components/receipt-reversal-submit-confirm-dialog.tsx](../../erp-client/features/customer-receivables/components/receipt-reversal-submit-confirm-dialog.tsx)、[features/supplier-payables/components/supplier-refund-submit-confirm-dialog.tsx](../../erp-client/features/supplier-payables/components/supplier-refund-submit-confirm-dialog.tsx)、[features/supplier-payables/components/payment-reversal-submit-confirm-dialog.tsx](../../erp-client/features/supplier-payables/components/payment-reversal-submit-confirm-dialog.tsx)、[features/inventory/pages/components/adjustment-confirm-dialog.tsx](../../erp-client/features/inventory/pages/components/adjustment-confirm-dialog.tsx)

<a id="d02"></a>
### D02 · 供给分配只保留一次最终确认（P1）

**证据：** 页面打开完整供给分配预览；预览按钮已写“确认库存分配并提交 N 张采购单”等。onConfirm 却仅关闭预览并打开第二个 AlertDialog，用户还要点“确认提交”。预览说明还包含“由一个后端事务统一处理”。

**执行要求：** 把库存预留数、采购单数、含税合计和审批去向集中在预览底部，直接执行最终提交。删除后续重复确认。删除“后端事务”说明；如需解释整批效果，使用“本次库存分配与采购建单一并提交”。

**验收要求：** 流程为编辑→预览→一次提交；保留提交前最新校验、失败输入、重复提交保护和结果未知处理。采购单提交审批不得被描述为审批已经通过。

**涉及文件：** [features/purchase-orders/components/purchase-order-create-preview.tsx](../../erp-client/features/purchase-orders/components/purchase-order-create-preview.tsx)、[features/purchase-orders/pages/purchase-order-create-page.tsx](../../erp-client/features/purchase-orders/pages/purchase-order-create-page.tsx)

<a id="d03"></a>
### D03 · 四类退款、冲正合并输入与确认（P1）

**证据：** 第一层输入原因、展示原单与全额金额，按钮为“下一步”；第二层再展示审批确认。核对 use-reverse-flow、use-supplier-refund-flow、use-payment-reversal-flow 后确认：prepare*Draft 仅绑定本地意图并换弹窗，不写后端。第二层主要列出抽象的“退款金额”等字段名，未持续展示第一层的实际主体和金额。

**执行要求：** 合并为一个弹窗，固定展示原单、客户/供应商、全额金额、原因和可用审批信息，主按钮用“提交退款审批”或“提交冲正审批”。原记录保留仅说明一次；保留退款的资金方向与冲正的纠错含义。已有草稿提交继续兼容原入口。

**验收要求：** 关闭不写入；确认后才执行现有提交服务。不得把退款与冲正混为一类。审批版本校验、提交失败、结果未知、相同意图重试及已有草稿入口均须保留。

**涉及文件：** [features/customer-receivables/components/customer-refund-request-dialog.tsx](../../erp-client/features/customer-receivables/components/customer-refund-request-dialog.tsx)、[features/customer-receivables/components/receipt-reversal-request-dialog.tsx](../../erp-client/features/customer-receivables/components/receipt-reversal-request-dialog.tsx)、[features/customer-receivables/components/customer-refund-submit-confirm-dialog.tsx](../../erp-client/features/customer-receivables/components/customer-refund-submit-confirm-dialog.tsx)、[features/customer-receivables/components/receipt-reversal-submit-confirm-dialog.tsx](../../erp-client/features/customer-receivables/components/receipt-reversal-submit-confirm-dialog.tsx)、[features/customer-receivables/components/customer-receivables-workspace.tsx](../../erp-client/features/customer-receivables/components/customer-receivables-workspace.tsx)、[features/supplier-payables/components/supplier-refund-request-dialog.tsx](../../erp-client/features/supplier-payables/components/supplier-refund-request-dialog.tsx)、[features/supplier-payables/components/payment-reversal-request-dialog.tsx](../../erp-client/features/supplier-payables/components/payment-reversal-request-dialog.tsx)、[features/supplier-payables/components/supplier-refund-submit-confirm-dialog.tsx](../../erp-client/features/supplier-payables/components/supplier-refund-submit-confirm-dialog.tsx)、[features/supplier-payables/components/payment-reversal-submit-confirm-dialog.tsx](../../erp-client/features/supplier-payables/components/payment-reversal-submit-confirm-dialog.tsx)、[features/supplier-payables/pages/supplier-accounts-page.tsx](../../erp-client/features/supplier-payables/pages/supplier-accounts-page.tsx)

<a id="d04"></a>
### D04 · 移除草稿分配行取消二次确认（P1）

**证据：** 每次移除行都打开“移除该分配行？”；实际 removeLine 只在本地 allocations 中过滤该行，没有调用服务端或撤销已生效核销。

**执行要求：** 点击移除后直接更新草稿，并提供短时“撤销”；撤销恢复原行、金额及顺序。已生效的核销冲正继续使用独立业务流程。

**验收要求：** 一次点击移除；撤销恢复完整本地内容；未提交前不改变已生效核销记录。

**涉及文件：** [features/customer-receivables/components/session-remove-line-dialog.tsx](../../erp-client/features/customer-receivables/components/session-remove-line-dialog.tsx)、[features/customer-receivables/components/allocation-session-panel.tsx](../../erp-client/features/customer-receivables/components/allocation-session-panel.tsx)、[features/customer-receivables/hooks/use-allocation-session.ts](../../erp-client/features/customer-receivables/hooks/use-allocation-session.ts)

<a id="d05"></a>
### D05 · 库存调整合并确认并突出调整结果（P1）

**证据：** 输入框说明和“提交约束”都写不立即改库存、提交审批；form.onSubmit 再 setConfirmOpen(true)，第二层再次解释同一规则，并把“已按当前数据版本核对”放入锁定字段。

**执行要求：** 单弹窗保留仓库、SKU、增减方向、调整数量、原因与审批路线；将影响压缩为“审批通过后调整库存，经办人不能审批本单”。需要额外核对时在同一弹窗切换到摘要，不叠加模态层。隐藏常态数据版本说明，冲突时再展示下一步。

**验收要求：** 数量和方向始终可见；不能直接调整库存或跳过审批；保留岗位分离、并发冲突及输入恢复。

**涉及文件：** [features/inventory/pages/components/adjustment-dialog.tsx](../../erp-client/features/inventory/pages/components/adjustment-dialog.tsx)、[features/inventory/pages/components/adjustment-confirm-dialog.tsx](../../erp-client/features/inventory/pages/components/adjustment-confirm-dialog.tsx)、[features/inventory/pages/hooks/use-adjustment-workflow.ts](../../erp-client/features/inventory/pages/hooks/use-adjustment-workflow.ts)

<a id="d06"></a>
### D06 · 结算复核人由输入内部 ID 改为选人（P1）

**证据：** 提交复核弹窗要求填写“复核人用户 ID”，placeholder 为“请输入明确的复核人用户 ID”。用户需要离开当前任务查找技术标识。

**执行要求：** 改为“复核人”搜索选择，显示姓名、账号和必要部门信息。候选仅含符合复核资格的人；说明压缩为“提交后交由所选人员复核”。

**验收要求：** 不得要求人工查用户 ID；候选资格和经办/复核分离由服务端再次核验。若当前没有合格人员查询接口，先补接口再替换控件。

**涉及文件：** [features/supplier-settlements/components/settlement-center-dialogs.tsx](../../erp-client/features/supplier-settlements/components/settlement-center-dialogs.tsx)

<a id="d07"></a>
### D07 · 验收登记避免“确认后再确认”（P1）

**证据：** 登记层按钮为“全部通过并确认”或“确认本次验收”，外层仍挂载 AcceptanceDialogs 的最终确认。登记默认选中全部待验批次并设为通过，因此最终范围、短少与拒收摘要有实际核对价值。

**执行要求：** 将批次数、通过/短少/拒收统计和异常数量放入登记层底部，保留一次最终确认；若保留独立核对步骤，则前一按钮必须改为“核对验收结果”。顶部改为“默认选中全部待验批次并设为通过，请按实际结果调整。”

**验收要求：** 默认全通过必须明确可见；本次不验、短少、拒收与服务不通过不得隐藏；多批和部分验收校验保留。

**涉及文件：** [features/sales-orders/components/acceptance-register-dialog.tsx](../../erp-client/features/sales-orders/components/acceptance-register-dialog.tsx)、[features/sales-orders/components/acceptance-dialogs.tsx](../../erp-client/features/sales-orders/components/acceptance-dialogs.tsx)、[features/sales-orders/components/acceptance-workspace.tsx](../../erp-client/features/sales-orders/components/acceptance-workspace.tsx)

<a id="d08"></a>
### D08 · 业务摘要不得装入“锁定字段”（P1）

**证据：** 付款确认把收款户名、银行、账号、金额放入 lockedFields，实际标题显示“提交后锁定字段”。供应商订单完成确认还显示任务版本与处理凭证内部标识。

**执行要求：** 将业务核对信息放入独立摘要区域：付款显示收款方、银行、掩码账号、金额；订单显示业务单号、供应商和核实结果。lockedFields 仅用于真正需要解释的编辑限制。处理凭证采用可读名称/业务编号，内部版本不占主内容。

**验收要求：** 付款关键核对信息不得减少；内部 ID 不代替人类可读单号；共享组件按调用方迁移，其他工作面的默认行为须兼容。

**涉及文件：** [components/business/workflow-actions.tsx](../../erp-client/components/business/workflow-actions.tsx)、[features/supplier-payables/components/supplier-payment-submit-confirm-dialog.tsx](../../erp-client/features/supplier-payables/components/supplier-payment-submit-confirm-dialog.tsx)、[features/supplier-orders/components/supplier-order-preview-center-dialogs.tsx](../../erp-client/features/supplier-orders/components/supplier-order-preview-center-dialogs.tsx)、[features/supplier-settlements/components/settlement-center-dialogs.tsx](../../erp-client/features/supplier-settlements/components/settlement-center-dialogs.tsx)

<a id="d09"></a>
### D09 · API 连接文案改为配置任务语言（P2）

**证据：** 现场截图 01 证实“新建连接身份”“不可与环境组合复用”和未提交时“正在创建生产环境连接身份”。能力配置另有“不复用采购确认写入口”；密钥选择说明“无明文密钥输入框；页面、URL 与结果均不返回正文”。

**执行要求：** 标题改“新建连接”；连接代码字段说明“连接代码不可重复，不同环境请使用不同代码”。未提交前环境提示改“当前环境：生产”。能力配置说明改“保存后需重新验证所选能力”。密钥与地址操作按实际动作使用“绑定密钥配置”“更换密钥配置”“绑定地址配置”；说明“从已配置的密钥中选择”，不展示实现边界。

**验收要求：** 全局唯一约束、生产环境提示、现有密钥引用机制保持；不得新增明文密钥输入。创建后是否自动进入详情须与实际回调一致。

**涉及文件：** [features/supplier-api-connections/components/connection-create-dialog.tsx](../../erp-client/features/supplier-api-connections/components/connection-create-dialog.tsx)、[features/supplier-api-connections/components/dialogs/cap-config-dialog.tsx](../../erp-client/features/supplier-api-connections/components/dialogs/cap-config-dialog.tsx)、[features/supplier-api-connections/components/dialogs/reference-bind-dialog.tsx](../../erp-client/features/supplier-api-connections/components/dialogs/reference-bind-dialog.tsx)

<a id="d10"></a>
### D10 · 健康检查按环境减少空确认（P2）

**证据：** 健康检查无可输入字段，测试和开发环境也要“执行健康检查→确认执行”。停用说明与正文重复“不删除任何数据”，并混入字段遮罩说明和多个替代入口。

**执行要求：** 测试/开发环境在确认无计费及额外业务副作用后改为直接执行并显示进度；生产环境保留适当确认。停用仅列连接名、环境、受影响发布/订单/任务数量和一句“历史记录保留”。启用说明改为“启用后恢复此连接的接口请求”。

**验收要求：** 不得因精简跳过生产环境治理规则；健康检查实际调用范围需在实施前核验，当前仅有源码所述不创建真实订单的证据。

**涉及文件：** [features/supplier-api-connections/components/dialogs/run-health-check-dialog.tsx](../../erp-client/features/supplier-api-connections/components/dialogs/run-health-check-dialog.tsx)、[features/supplier-api-connections/components/dialogs/enable-connection-dialog.tsx](../../erp-client/features/supplier-api-connections/components/dialogs/enable-connection-dialog.tsx)、[features/supplier-api-connections/components/dialogs/disable-connection-dialog.tsx](../../erp-client/features/supplier-api-connections/components/dialogs/disable-connection-dialog.tsx)

<a id="d11"></a>
### D11 · 主数据新增删除教程式说明（P2）

**证据：** 现场截图 02 证实新建品牌顶部两行说明“保存后生成资料编号和第一版内容。以后如需修改，请用更新资料，历史记录会保留”。仅为新建品牌却要求填写“变更原因”。同类描述通过 masterDataCopy 复用。

**执行要求：** 新建时省略常态版本教程；更新时最多保留“更新后保留历史版本，不影响已有单据”。新建原因标签改“创建说明”，不直接移除既有必填校验。将来若改为系统生成新建原因，须单独确定审计要求。编辑表单次按钮统一“取消”，纯查看统一“关闭”。

**验收要求：** 分类/品牌/计量单位全部核对调用点；历史版本和原因记录照常生成。不得为一个弹窗直接改变所有共享默认描述。

**涉及文件：** [features/master-data/components/brand/brand-form-dialog-frame.tsx](../../erp-client/features/master-data/components/brand/brand-form-dialog-frame.tsx)、[features/master-data/components/category/category-form-dialog-frame.tsx](../../erp-client/features/master-data/components/category/category-form-dialog-frame.tsx)、[features/master-data/components/unit-of-measure/unit-of-measure-form-dialogs.tsx](../../erp-client/features/master-data/components/unit-of-measure/unit-of-measure-form-dialogs.tsx)、[features/master-data/lib/copy.ts](../../erp-client/features/master-data/lib/copy.ts)

<a id="d12"></a>
### D12 · 供应商保存与商品保存按信息量区别处理（P2）

**证据：** 供应商保存弹窗仅补一个原因，标题、说明与 placeholder 多次解释新版本；商品保存弹窗则额外包含修改摘要、生效时间和原因，有真实核对信息。

**执行要求：** 供应商原因可移到页面保存区，点击保存一次提交；若保留弹窗，标题“保存供应商资料”，原因必填标记足够，placeholder 改“例如：更新结算信息”。商品保留摘要与生效时间；描述改“核对修改内容和生效时间”，原因提示不再重复版本机制。

**验收要求：** 禁止把商品保存窗按普通空确认删除；定时生效、原因必填、未保存修改和无内容变化生成版本的现有规则须保留或单独变更。

**涉及文件：** [features/master-data/components/supplier/supplier-save-reason-dialog.tsx](../../erp-client/features/master-data/components/supplier/supplier-save-reason-dialog.tsx)、[features/master-data/components/supplier/supplier-editor-dialogs.tsx](../../erp-client/features/master-data/components/supplier/supplier-editor-dialogs.tsx)、[features/master-data/components/product/product-save-dialog.tsx](../../erp-client/features/master-data/components/product/product-save-dialog.tsx)、[features/master-data/components/product/product-effective-section.tsx](../../erp-client/features/master-data/components/product/product-effective-section.tsx)

<a id="d13"></a>
### D13 · 商品原生确认按实际损失分级（P2）

**证据：** 4 个 window.confirm 调用涉及规格重建、批量价格、商品下架和 SKU 停用。批量价格即使没有覆盖既有价格也要求确认；规格重建把全部受损 SKU 与多类损失塞进浏览器文本框。

**执行要求：** 首次批量填价直接应用并可撤销；仅实际覆盖非空价格时提示覆盖字段及 SKU 数。规格变更用结构化确认：移除 N 个 SKU、列表、无法继承的价格/主图/条码/供给关系、保存后生效。商品下架与 SKU 停用保留作用范围，按钮明确“下架商品”“停用 SKU”。

**验收要求：** 仅填销售价时不得误称会覆盖市场价。规格重建的损失信息不可删；取消应用须保持当前已应用版本。

**涉及文件：** [features/master-data/lib/product-form-bindings.ts](../../erp-client/features/master-data/lib/product-form-bindings.ts)、[features/master-data/hooks/use-product-list-state.ts](../../erp-client/features/master-data/hooks/use-product-list-state.ts)、[features/master-data/components/product/product-sku-table.tsx](../../erp-client/features/master-data/components/product/product-sku-table.tsx)

<a id="d14"></a>
### D14 · SKU 编辑说明保留保存时点（P2）

**证据：** 编辑态说明“修改后返回商品页统一保存；关闭此窗口会保留本次编辑内容”，只读态说明重复编码、名称、条码字段。

**执行要求：** 编辑态压缩为“修改将在保存商品时生效”；只读态省略字段目录。保留“完成编辑”与“关闭”按权限切换。

**验收要求：** 不得把“完成编辑”改为暗示已经持久化的“保存”；关闭后输入仍按现有规则保留。

**涉及文件：** [features/master-data/components/product/product-sku-table.tsx](../../erp-client/features/master-data/components/product/product-sku-table.tsx)

<a id="d15"></a>
### D15 · 添加供给表单分常用与补充信息（P2）

**证据：** 添加供给包含五个大分区；从商品 SKU 入口可带入对象。说明介绍供给关系模型；可供情况弹窗解释“独立于商业条款版本、高频更新”。

**执行要求：** 预选 SKU 时显示只读商品摘要，不要求重复选择。首屏优先供应商、订货编码、价格、税率、起订量；选填区域/时效/物流费用按数据和任务展开，已有值不得默认藏匿。更新可供情况说明改“不修改供给价格和条款”；供应商 SKU 编码帮助改“供应商下单时使用的编码”。状态切换只保留目标状态、版本和必要影响。

**验收要求：** 不得折叠必填错误；价格、数量上限留空含义、条款与可供情况的独立性保留。商品供给→添加供给需实测焦点、回退和刷新，不能仅据嵌套代码认定故障。

**涉及文件：** [features/supplier-offerings/components/dialogs/register-supply-for-sku-dialog.tsx](../../erp-client/features/supplier-offerings/components/dialogs/register-supply-for-sku-dialog.tsx)、[features/supplier-offerings/components/dialogs/revise-offering-dialog.tsx](../../erp-client/features/supplier-offerings/components/dialogs/revise-offering-dialog.tsx)、[features/supplier-offerings/components/dialogs/update-availability-dialog.tsx](../../erp-client/features/supplier-offerings/components/dialogs/update-availability-dialog.tsx)、[features/supplier-offerings/components/dialogs/change-offering-status-dialog.tsx](../../erp-client/features/supplier-offerings/components/dialogs/change-offering-status-dialog.tsx)、[features/master-data/components/product/product-supply-dialog.tsx](../../erp-client/features/master-data/components/product/product-supply-dialog.tsx)

<a id="d16"></a>
### D16 · 审批决定保留结果，删除核对套话（P2）

**证据：** 审批通过说明“请核对单据、金额、当前节点和结果影响。只有确认后才会提交审批决定”，正文紧接相同四项；驳回影响也重复。恢复审批人解释新旧办理记录与重放机制。

**执行要求：** 审批决定保留单据、金额、当前节点和实际下一步，删除“只有确认后才会提交”等空说明；通过意见标签改“审批意见（可选）”。恢复说明改“重新通知原审批人处理当前节点，已完成的审批不变”。流程版本更新保留前后路线；应急代办、受阻取消结果必须保留。

**验收要求：** 末节点通过和中间节点通过须显示各自真实效果；驳回原因必填、首节点重审、应急身份记录等约束不变。

**涉及文件：** [features/approval-workflow/components/decision-dialog.tsx](../../erp-client/features/approval-workflow/components/decision-dialog.tsx)、[features/approval-workflow/components/resume-approver-dialog.tsx](../../erp-client/features/approval-workflow/components/resume-approver-dialog.tsx)、[features/approval-workflow/components/upgrade-binding-dialog.tsx](../../erp-client/features/approval-workflow/components/upgrade-binding-dialog.tsx)、[features/approval-workflow/components/cancel-approval-dialog.tsx](../../erp-client/features/approval-workflow/components/cancel-approval-dialog.tsx)

<a id="d17"></a>
### D17 · 审批流程创建只说明来源选择（P2）

**证据：** 新建草稿解释“创建更高版本草稿。必须明确选择来源，不会默认复制历史版本”。发布和退役说明包含对已有单据及新单据的实际影响。

**执行要求：** 新建说明压缩为“选择空白流程，或复制当前已发布版本”。继续要求显式选来源，不擅自默认复制。发布、退役只精简重复措辞，保留已有单据不受影响和无可用版本时不能创建新单据。

**验收要求：** 流程来源必须可辨；不得因简短而移除退役阻断新单据的提示。

**涉及文件：** [features/approval-processes/components/create-draft-dialog.tsx](../../erp-client/features/approval-processes/components/create-draft-dialog.tsx)、[features/approval-processes/components/publish-dialog.tsx](../../erp-client/features/approval-processes/components/publish-dialog.tsx)、[features/approval-processes/components/retire-dialog.tsx](../../erp-client/features/approval-processes/components/retire-dialog.tsx)

<a id="d18"></a>
### D18 · 权限与删除文案只保留用户后果（P2）

**证据：** 删除账号/角色提示“系统内置…会被后端拒绝删除”；授权变更头部与底部重复版本冲突处理。账号编辑说明和密码 placeholder 都写留空不修改。RoleAssignmentDialog 当前无直接导入方，不计为已上线操作入口。

**执行要求：** 账号删除说明“删除后，该账号将无法登录”；角色删除说明“已绑定账号将失去该角色提供的权限”。内置对象直接禁用删除并说明原因。密码留空提示靠近密码字段保留一次。授权预览保留具体人员和权限差异，版本冲突指导仅在冲突发生时出现。

**验收要求：** 删除确认保留；不能仅靠前端禁止内置对象删除。未接入组件不得计入用户当前操作次数。

**涉及文件：** [features/admin/components/accounts/delete-admin-dialog.tsx](../../erp-client/features/admin/components/accounts/delete-admin-dialog.tsx)、[features/admin/components/roles/delete-role-dialog.tsx](../../erp-client/features/admin/components/roles/delete-role-dialog.tsx)、[features/admin/components/accounts/account-form-dialog.tsx](../../erp-client/features/admin/components/accounts/account-form-dialog.tsx)、[features/access-audit/pages/components/access-change-dialog.tsx](../../erp-client/features/access-audit/pages/components/access-change-dialog.tsx)、[features/access-audit/components/role-assignment-dialog.tsx](../../erp-client/features/access-audit/components/role-assignment-dialog.tsx)

<a id="d19"></a>
### D19 · 客户与合同新增删除实现说明（P2）

**证据：** 新建客户描述客户主体、首版资料及相似名称不合并；合同上传描述“系统不新建或编辑合同正文…形成可引用的合同版本”。客户归属调整说明包含替换负责人和新增协作两种不同效果。

**执行要求：** 新建客户省略常态首版说明，相似名称规则在出现候选时解释。合同改“上传已签署的合同 PDF，并填写合同信息”，保留付款条件用于带出、完整条款以 PDF 为准。归属说明按当前选择仅展示换任或协作对应后果。

**验收要求：** 客户不得自动合并；合同上传不代表系统签署合同。归属结束日期、历史责任保留和重叠换任语义不变。

**涉及文件：** [features/customers/components/customer-create-dialog.tsx](../../erp-client/features/customers/components/customer-create-dialog.tsx)、[features/customers/components/customer-assignment-dialog.tsx](../../erp-client/features/customers/components/customer-assignment-dialog.tsx)、[features/contracts/components/contract-upload-dialog.tsx](../../erp-client/features/contracts/components/contract-upload-dialog.tsx)

<a id="d20"></a>
### D20 · 核销入口和反向操作用具体业务名（P2）

**证据：** 选择主体后按钮写“打开核销工作区”“进入本次核销”；反向操作统一写“确认追加反向记录”；付款详情的退出按钮为“取消”。发票确认列有“追加式分配”“净分配（系统）”“重复提交不会重复生成记录”等。

**执行要求：** 从入口明确的场景带入客户/供应商；需要选择时保留选择步骤，说明简化为“创建后不可更换结算主体”。按钮按动作写“登记回款”“登记进项发票”“提交红票”。付款详情改“关闭”，描述省略字段目录。发票确认展示实际金额、分配笔数、未分配余额和纠错路径，删除机制条目。

**验收要求：** 经营客户与结算主体可能不同的提示必须保留在会影响选择的场景；部分红票金额上限和原记录保留规则不变。

**涉及文件：** [features/customer-receivables/components/receivable-action-dialogs.tsx](../../erp-client/features/customer-receivables/components/receivable-action-dialogs.tsx)、[features/supplier-payables/pages/components/pick-supplier-dialog.tsx](../../erp-client/features/supplier-payables/pages/components/pick-supplier-dialog.tsx)、[features/supplier-payables/pages/components/reverse-dialog.tsx](../../erp-client/features/supplier-payables/pages/components/reverse-dialog.tsx)、[features/supplier-payables/components/supplier-payment-detail-dialog.tsx](../../erp-client/features/supplier-payables/components/supplier-payment-detail-dialog.tsx)、[features/customer-receivables/components/allocation-session-panel.tsx](../../erp-client/features/customer-receivables/components/allocation-session-panel.tsx)、[features/supplier-payables/components/allocation-workspace.tsx](../../erp-client/features/supplier-payables/components/allocation-workspace.tsx)

<a id="d21"></a>
### D21 · 导入操作删除任务实现术语（P2）

**证据：** 范围确认解释“正式确认事实”“同一操作中完成当前任务”；重新准备失败项把“仍需再次点击提交应用”列为不可逆影响。

**执行要求：** 确认范围改“确认本范围的试算结果”；退回改“退回修复后，需重新试算并确认”。重新准备失败项清楚说明“只准备失败项；准备完成后仍需提交应用”，作为下一步提示而非不可逆警告。保留实际导入数量、已成功项不回滚和仅取消未应用项。

**验收要求：** 确认范围不得自动启动导入；重试准备不得自动执行；取消范围与既有成功结果明确可见。

**涉及文件：** [features/import-opening/components/confirm-section.tsx](../../erp-client/features/import-opening/components/confirm-section.tsx)、[features/import-opening/components/execution-actions.tsx](../../erp-client/features/import-opening/components/execution-actions.tsx)

<a id="d22"></a>
### D22 · 供应商异常与结算结果说明压缩（P2）

**证据：** 供应停止确认列有“完成当前 W21 工作项”“暂停修订继续生效”；异常关闭多处重复退出待办。结算确认反复解释追加式记录、冻结、唯一应付；登记差异的“受控结论”“原因码”偏内部术语。

**执行要求：** 供应停止改“完成核对后，供给与商品发布仍保持暂停”。安全重发保留“再次向供应商下单、不会新建业务订单”的风险。异常关闭改“关闭当前任务，不改变业务记录”；已解决保留证据要求。差异表单改“处理结论”“处理原因”；确认结算保留金额变化、生成应付、不可撤回与经办/复核分离，删除唯一性和内部任务编号。

**验收要求：** 不得把完成任务表述为恢复供给或业务纠错完成；结算确认的财务后果和不可撤回性质不得弱化。

**涉及文件：** [features/supplier-offerings/components/supply-exception-task-panel.tsx](../../erp-client/features/supplier-offerings/components/supply-exception-task-panel.tsx)、[features/supplier-orders/components/supplier-order-preview-center-dialogs.tsx](../../erp-client/features/supplier-orders/components/supplier-order-preview-center-dialogs.tsx)、[features/integration-errors/components/terminal-action-dialog.tsx](../../erp-client/features/integration-errors/components/terminal-action-dialog.tsx)、[features/supplier-settlements/components/settlement-center-dialogs.tsx](../../erp-client/features/supplier-settlements/components/settlement-center-dialogs.tsx)

<a id="d23"></a>
### D23 · 责任配置帮助文字按条件展示（P2）

**证据：** 采购规则头部解释销售不能改负责人；履约转交字段解释完整权限、全部开放任务提交时再校验。财务和仓库配置含“仅影响新任务”的真实边界。

**执行要求：** 采购规则只说明“更具体的规则优先”；规则优先级在选定类型后就近解释。转交摘要保留当前责任人、目标人以及是否影响整张采购单的开放任务，删除常态权限校验机制。财务/仓库的“仅影响新任务”各保留一次。

**验收要求：** 不得误导用户认为会调整历史任务；整单批量转交的范围必须突出显示。

**涉及文件：** [features/procurement-responsibilities/components/procurement-responsibility-rules-page.tsx](../../erp-client/features/procurement-responsibilities/components/procurement-responsibility-rules-page.tsx)、[features/finance-responsibilities/components/finance-responsibility-rules-page.tsx](../../erp-client/features/finance-responsibilities/components/finance-responsibility-rules-page.tsx)、[features/workspace/components/workspace-fulfillment-task.tsx](../../erp-client/features/workspace/components/workspace-fulfillment-task.tsx)、[features/master-data/components/warehouse/warehouse-action-dialogs.tsx](../../erp-client/features/master-data/components/warehouse/warehouse-action-dialogs.tsx)

<a id="d24"></a>
### D24 · 区分隐藏说明和可见赘述，保留有效轻弹窗（P3）

**证据：** 销售/采购/工作台纸质预览长说明使用 sr-only，不占可见空间；不能当作视觉冗余。未保存离开与版本冲突承担真实的数据保护作用。履约确认通过 confirmDescription 已按操作给出一句实际结果。

**执行要求：** 保持纸质预览、图片预览、未保存保护、冲突处理及履约确认的现有结构。屏幕阅读器说明可缩短但必须保留可访问标题和必要说明。经营标签保留收入与利润口径差异。图片同名标题与“图片预览”可去重。

**验收要求：** 不得删除可访问名称；长文是否可见以 DOM/class 为准；焦点回归、Esc、取消和移动端滚动单独补验。

**涉及文件：** [features/sales-orders/components/sales-order-paper-dialog.tsx](../../erp-client/features/sales-orders/components/sales-order-paper-dialog.tsx)、[features/sales-orders/components/sales-order-paper-preview-dialog.tsx](../../erp-client/features/sales-orders/components/sales-order-paper-preview-dialog.tsx)、[features/purchase-orders/components/purchase-order-paper-dialog.tsx](../../erp-client/features/purchase-orders/components/purchase-order-paper-dialog.tsx)、[features/workspace/components/workspace-document-paper-dialog.tsx](../../erp-client/features/workspace/components/workspace-document-paper-dialog.tsx)、[features/contracts/components/contract-paper-dialog.tsx](../../erp-client/features/contracts/components/contract-paper-dialog.tsx)、[features/master-data/components/product/product-editor-media.tsx](../../erp-client/features/master-data/components/product/product-editor-media.tsx)、[features/master-data/components/shared/media-list-field.tsx](../../erp-client/features/master-data/components/shared/media-list-field.tsx)、[components/ui/file-upload.tsx](../../erp-client/components/ui/file-upload.tsx)、[components/business/feedback.tsx](../../erp-client/components/business/feedback.tsx)、[components/business/workflow.tsx](../../erp-client/components/business/workflow.tsx)、[features/fulfillment-operations/pages/components/fulfillment-operations-workspace.tsx](../../erp-client/features/fulfillment-operations/pages/components/fulfillment-operations-workspace.tsx)、[features/customer-quality/components/business-tag-dialog.tsx](../../erp-client/features/customer-quality/components/business-tag-dialog.tsx)

## 5. 现场抽查

1. API 连接列表 → 新建连接：表单可正常打开，名称和说明存在技术化表达；未提交前展示“正在创建”不准确。焦点进入连接代码字段，取消后回到列表的新建入口。

![新建 API 连接现场](01-create-connection.png)

2. 品牌列表 → 新建：表单可正常打开；版本教程占两行，创建表单使用“变更原因”。焦点进入名称字段；未输入、未保存。

![新建品牌现场](02-create-brand.png)

截图只支持上述可见文案与入口判断；不构成键盘全路径、读屏、移动端、保存或业务权限验收。

## 6. 验证交付要求

实施时必须覆盖：正常提交、取消/返回、必填错误、并发冲突、提交失败保留输入、结果未知重试、已有草稿提交。涉及弹窗合并时必须额外验证焦点、单层遮罩、键盘确认、重复提交防护。涉及业务状态的改动须使用对应实际页面验证，不得以静态预览代替登录后的业务操作证据。

本次仅扫描和分析，未修改生产代码，未运行编译或测试；完整索引见 [dialog-inventory.md](dialog-inventory.md)，原始结构见 [dialog-index.json](dialog-index.json)。
