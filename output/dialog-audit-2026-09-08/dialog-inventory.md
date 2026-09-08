# Dialog 全量静态索引

范围：生产 TS/TSX 中的 Dialog/AlertDialog、以 Dialog/Modal 结尾的 JSX 组件、原生确认；组件直接导入关系经 TypeScript 模块解析取得。纯包装调用不是新的唯一弹窗。空调用列表只表示未发现直接静态导入，不证明死代码。建议编号见 [执行清单](dialog-audit.md)。

## app/(workspace)/procurement/orders/purchase-order-create-client.tsx

**节点：** SalesOrderPaperPreviewDialog（行 21）

**建议：** 未列为优先修改项：保留当前结构；调用/装配代码跟随关联弹窗调整。未逐状态实操，不构成运行时验收。

**直接导入方：** [app/(workspace)/procurement/orders/page.tsx](../../erp-client/app/(workspace)/procurement/orders/page.tsx#L4)

## components/business/feedback.tsx

**节点：** AlertDialog（行 720）

**建议：** D24

**直接导入方：** [components/business/data-table-body.tsx](../../erp-client/components/business/data-table-body.tsx#L10)、[components/business/index.ts](../../erp-client/components/business/index.ts#L8)、[components/business/editor.tsx](../../erp-client/components/business/editor.tsx#L38)、[features/supplier-api-connections/lib/operations.ts](../../erp-client/features/supplier-api-connections/lib/operations.ts#L1)、[features/supplier-api-connections/components/connection-list.tsx](../../erp-client/features/supplier-api-connections/components/connection-list.tsx#L18)、[features/supplier-api-connections/components/connection-center.tsx](../../erp-client/features/supplier-api-connections/components/connection-center.tsx#L56)、[features/supplier-api-connections/components/connection-create-dialog.tsx](../../erp-client/features/supplier-api-connections/components/connection-create-dialog.tsx#L7)、[features/inventory/pages/hooks/use-adjustment-workflow.ts](../../erp-client/features/inventory/pages/hooks/use-adjustment-workflow.ts#L5)、[features/inventory/pages/components/adjustment-result-banner.tsx](../../erp-client/features/inventory/pages/components/adjustment-result-banner.tsx#L6)、[features/access-audit/pages/lib/outcome-state.ts](../../erp-client/features/access-audit/pages/lib/outcome-state.ts#L1)、[features/fulfillment-operations/pages/hooks/use-fulfillment-operations-controller.ts](../../erp-client/features/fulfillment-operations/pages/hooks/use-fulfillment-operations-controller.ts#L7)、[features/access-audit/pages/hooks/use-access-audit-page.ts](../../erp-client/features/access-audit/pages/hooks/use-access-audit-page.ts#L6)、[features/access-audit/pages/hooks/use-access-change-flow.ts](../../erp-client/features/access-audit/pages/hooks/use-access-change-flow.ts#L6)、[features/fulfillment-operations/pages/components/fulfillment-result-panel.tsx](../../erp-client/features/fulfillment-operations/pages/components/fulfillment-result-panel.tsx#L7)、[features/supplier-settlements/components/settlement-list.tsx](../../erp-client/features/supplier-settlements/components/settlement-list.tsx#L16)、[features/supplier-settlements/components/settlement-center-result.tsx](../../erp-client/features/supplier-settlements/components/settlement-center-result.tsx#L8)、[features/customer-receivables/hooks/use-allocation-session.ts](../../erp-client/features/customer-receivables/hooks/use-allocation-session.ts#L7)、[features/customer-receivables/components/customer-receivables-workspace.tsx](../../erp-client/features/customer-receivables/components/customer-receivables-workspace.tsx#L8)、[features/customer-receivables/pages/hooks/use-reverse-flow.ts](../../erp-client/features/customer-receivables/pages/hooks/use-reverse-flow.ts#L5)、[features/supplier-settlements/lib/operations.ts](../../erp-client/features/supplier-settlements/lib/operations.ts#L1)、[features/supplier-settlements/hooks/use-settlement-result-focus.ts](../../erp-client/features/supplier-settlements/hooks/use-settlement-result-focus.ts#L5)、[features/supplier-settlements/hooks/use-settlement-center-actions.ts](../../erp-client/features/supplier-settlements/hooks/use-settlement-center-actions.ts#L7)

<details><summary>本文件中文字符串索引（包含非弹窗、隐藏说明与动态分支，不等同可见弹窗文案）</summary>

- L81：尚无更改
- L82：有未保存更改
- L83：正在保存
- L84：已保存
- L85：保存失败
- L130：重试保存
- L292：结果已经形成，并可在关联单据与审计记录中追溯。
- L293：已完成
- L299：本次操作未形成目标结果，请根据原因继续处理。
- L300：未通过
- L306：当前前置条件尚未满足，本次操作未形成处理结果。
- L307：已阻断
- L313：结果尚未确定，可安全离开并在后台任务中继续查看。
- L314：处理中
- L320：不得按成功处理，请等待核对或进入异常处理流程。
- L321：结果未知
- L332：结果编号
- L426：等待执行
- L427：执行中
- L428：已完成
- L429：部分成功
- L430：执行失败
- L431：已冻结
- L446：后台任务进度
- L463：整批按原子方式执行；任一项失败时，整批结果不生效。
- L464：允许部分成功；已经形成的有效结果不会因同批其他失败而回退。
- L489：整批原子执行
- L490：允许部分成功
- L514：成功
- L518：跳过
- L522：失败
- L552：敏感信息
- L657：读取中
- L657：隐藏
- L657：显示
- L667：将在
- L667：秒后自动隐藏
- L675：暂时无法显示敏感信息
- L684：重试
- L711：放弃未保存的更改？
- L712：本次输入尚未保存，离开后将丢失。
- L713：放弃更改
- L714：继续编辑

</details>

## components/business/workflow-actions.tsx

**节点：** AlertDialog（行 311）

**建议：** D01、D08

**直接导入方：** [components/business/workflow.tsx](../../erp-client/components/business/workflow.tsx#L42)、[components/business/workflow.tsx](../../erp-client/components/business/workflow.tsx#L43)、[components/business/workflow.tsx](../../erp-client/components/business/workflow.tsx#L515)、[features/sales-orders/hooks/use-change-review-actions.ts](../../erp-client/features/sales-orders/hooks/use-change-review-actions.ts#L13)、[features/supplier-orders/hooks/use-supplier-order-center-derivation.ts](../../erp-client/features/supplier-orders/hooks/use-supplier-order-center-derivation.ts#L5)、[features/supplier-orders/components/supplier-order-preview-center-panels.tsx](../../erp-client/features/supplier-orders/components/supplier-order-preview-center-panels.tsx#L14)、[features/integration-errors/pages/lib/selection.ts](../../erp-client/features/integration-errors/pages/lib/selection.ts#L1)、[features/integration-errors/pages/components/integration-item-progress.tsx](../../erp-client/features/integration-errors/pages/components/integration-item-progress.tsx#L2)、[features/integration-errors/pages/components/integration-detail-workflow.tsx](../../erp-client/features/integration-errors/pages/components/integration-detail-workflow.tsx#L3)、[features/integration-errors/pages/components/integration-action-zone.tsx](../../erp-client/features/integration-errors/pages/components/integration-action-zone.tsx#L3)、[features/supplier-settlements/lib/settlement-responsibility.ts](../../erp-client/features/supplier-settlements/lib/settlement-responsibility.ts#L1)

<details><summary>本文件中文字符串索引（包含非弹窗、隐藏说明与动态分支，不等同可见弹窗文案）</summary>

- L98：操作未完成，请稍后重试。
- L119：状态变化
- L128：变更为
- L228：返回修改
- L296：请核对状态变化和业务影响后再继续。
- L337：提交后锁定字段
- L342：本次动作产生的影响
- L352：下一责任部门
- L360：无法自动撤回的影响
- L371：操作未完成
- L508：处理当前任务
- L510：处理并打开下一条
- L514：返回队列
- L538：连续处理操作
- L605：正在处理
- L627：正在处理

</details>

## components/business/workflow.tsx

**节点：** Dialog（行 249）

**建议：** D24

**直接导入方：** [components/business/index.ts](../../erp-client/components/business/index.ts#L28)

<details><summary>本文件中文字符串索引（包含非弹窗、隐藏说明与动态分支，不等同可见弹窗文案）</summary>

- L67：批量操作影响预览
- L68：执行前请再次核对当前筛选、选择范围和预计处理结果。
- L77：预计处理
- L78：可处理
- L79：将跳过
- L102：将创建后台任务
- L102：当前页面内执行
- L112：当前筛选
- L124：选择范围
- L179：包含敏感字段
- L181：结果将继续执行当前用户的字段权限和遮罩规则。
- L223：数据已更新
- L224：当前数据已经更新，你输入的内容不能直接覆盖。
- L270：当前系统版本
- L276：当前有效
- L285：你输入的内容版本
- L291：数据已过期
- L302：当前版本的最近变更
- L322：版本差异
- L335：取消
- L356：查看差异
- L377：保留为新草稿
- L397：重新加载
- L436：这是协作提醒；提交仍以系统最新数据为准。
- L445：协作状态
- L472：当前无其他协作者
- L484：正在编辑：
- L496：正在查看：

</details>

## components/ui/command.tsx

**节点：** Dialog（行 48）

**建议：** 未列为优先修改项：保留当前结构；调用/装配代码跟随关联弹窗调整。未逐状态实操，不构成运行时验收。

**直接导入方：** 未发现直接静态导入；不得据此断言未使用。

<details><summary>本文件中文字符串索引（包含非弹窗、隐藏说明与动态分支，不等同可见弹窗文案）</summary>


</details>

## components/ui/file-upload.tsx

**节点：** Dialog（行 362）

**建议：** D24

**直接导入方：** [components/business/attachments.tsx](../../erp-client/components/business/attachments.tsx#L23)、[components/form/pdf-upload-field.tsx](../../erp-client/components/form/pdf-upload-field.tsx#L23)、[features/fulfillment-operations/components/forms/fulfillment-service-form.tsx](../../erp-client/features/fulfillment-operations/components/forms/fulfillment-service-form.tsx#L5)、[features/fulfillment-operations/components/forms/fulfillment-electronic-form.tsx](../../erp-client/features/fulfillment-operations/components/forms/fulfillment-electronic-form.tsx#L5)、[features/master-data/components/product/product-editor-media.tsx](../../erp-client/features/master-data/components/product/product-editor-media.tsx#L14)、[features/master-data/components/shared/media-list-field.tsx](../../erp-client/features/master-data/components/shared/media-list-field.tsx#L15)、[features/master-data/components/shared/action-dialog-shared.tsx](../../erp-client/features/master-data/components/shared/action-dialog-shared.tsx#L9)、[features/supplier-payables/components/supplier-payment-detail-body.tsx](../../erp-client/features/supplier-payables/components/supplier-payment-detail-body.tsx#L26)、[features/supplier-payables/components/allocation-fact-form-card.tsx](../../erp-client/features/supplier-payables/components/allocation-fact-form-card.tsx#L24)

<details><summary>本文件中文字符串索引（包含非弹窗、隐藏说明与动态分支，不等同可见弹窗文案）</summary>

- L45：上传文件
- L46：点击选择文件，或将文件拖到此处
- L139：待上传
- L139：已上传
- L199：选择上传文件
- L336：上传文件
- L350：选择文件
- L352：选择
- L354：选择文件
- L372：· 图片预览

</details>

## features/access-audit/components/role-assignment-dialog.tsx

**节点：** Dialog（行 74）

**建议：** D18

**直接导入方：** [features/access-audit/pages/hooks/use-access-audit-page.ts](../../erp-client/features/access-audit/pages/hooks/use-access-audit-page.ts#L14)、[features/access-audit/hooks/use-user-columns.tsx](../../erp-client/features/access-audit/hooks/use-user-columns.tsx#L16)、[features/access-audit/hooks/access-columns-input.ts](../../erp-client/features/access-audit/hooks/access-columns-input.ts#L5)

<details><summary>本文件中文字符串索引（包含非弹窗、隐藏说明与动态分支，不等同可见弹窗文案）</summary>

- L31：至少选择一个角色
- L66：操作失败，请重试。
- L80：调整角色
- L83：）的角色决定其可访问的页面与动作，保存后立即生效。
- L103：角色
- L140：提交失败
- L152：取消
- L157：保存

</details>

## features/access-audit/pages/access-audit-page.tsx

**节点：** AccessChangeDialog（行 255）；DeleteRoleDialog（行 272）

**建议：** 未列为优先修改项：保留当前结构；调用/装配代码跟随关联弹窗调整。未逐状态实操，不构成运行时验收。

**直接导入方：** [app/(workspace)/system/access-audit/page.tsx](../../erp-client/app/(workspace)/system/access-audit/page.tsx#L4)

## features/access-audit/pages/components/access-change-dialog.tsx

**节点：** Dialog（行 53）

**建议：** D18

**直接导入方：** [features/access-audit/pages/access-audit-page.tsx](../../erp-client/features/access-audit/pages/access-audit-page.tsx#L26)

<details><summary>本文件中文字符串索引（包含非弹窗、隐藏说明与动态分支，不等同可见弹窗文案）</summary>

- L65：授权变更影响预览
- L67：提交前先查看变更预览与受影响人员；若数据已被他人更新，需确认后重新提交。
- L90：密钥
- L90：卡密
- L90：完整银行账号
- L105：风险
- L107：高
- L109：中
- L110：低
- L132：配置差异
- L152：变更原因
- L171：安全运维
- L175：紧急止损
- L179：组织调整
- L184：变更原因
- L185：变更原因
- L192：说明（可选，勿填密钥）
- L206：不包含密钥或敏感业务正文
- L212：提交前系统会按最新配置核对版本；若配置已被他人更新，将提示你重新确认。
- L222：取消
- L227：确认提交
- L228：提交中…
- L261：关闭并记录阻断

</details>

## features/admin/components/accounts/account-form-dialog.tsx

**节点：** Dialog（行 134）

**建议：** D18

**直接导入方：** [features/admin/pages/accounts-page.tsx](../../erp-client/features/admin/pages/accounts-page.tsx#L25)、[features/admin/pages/accounts-page.tsx](../../erp-client/features/admin/pages/accounts-page.tsx#L26)、[features/admin/account-form-dialog.ts](../../erp-client/features/admin/account-form-dialog.ts#L6)、[features/admin/account-form-dialog.ts](../../erp-client/features/admin/account-form-dialog.ts#L7)

<details><summary>本文件中文字符串索引（包含非弹窗、隐藏说明与动态分支，不等同可见弹窗文案）</summary>

- L48：账号长度必须在3-32个字符之间
- L49：账号长度必须在3-32个字符之间
- L50：请输入姓名
- L53：密码长度必须在6-32个字符之间
- L54：密码长度必须在6-32个字符之间
- L55：至少选择一个角色
- L60：请输入姓名
- L63：密码长度不能超过32个字符
- L65：密码长度必须在6-32个字符之间
- L67：至少选择一个角色
- L128：操作失败，请重试。
- L141：编辑账号
- L141：新建账号
- L145：修改姓名、角色或密码；密码留空表示不修改。
- L146：创建后台管理员账号并绑定角色；账号创建后不可修改。
- L159：账号
- L173：账号
- L175：登录账号，3-32 个字符
- L186：姓名
- L188：管理员姓名
- L197：新密码
- L197：密码
- L201：留空则不修改
- L201：6-32 个字符
- L224：角色分配
- L231：角色信息待确认
- L232：选择该账号需要使用的角色。
- L263：提交失败
- L275：取消
- L280：保存
- L280：创建

</details>

## features/admin/components/accounts/delete-admin-dialog.tsx

**节点：** AlertDialog（行 33）

**建议：** D18

**直接导入方：** [features/admin/pages/accounts-page.tsx](../../erp-client/features/admin/pages/accounts-page.tsx#L27)、[features/admin/delete-admin-dialog.ts](../../erp-client/features/admin/delete-admin-dialog.ts#L6)

<details><summary>本文件中文字符串索引（包含非弹窗、隐藏说明与动态分支，不等同可见弹窗文案）</summary>

- L37：删除账号
- L40：删除后该账号无法登录后台。系统内置账号会被后端拒绝删除；操作会记录审计日志。
- L45：删除失败
- L54：取消
- L67：删除失败，请重试。
- L72：删除中…
- L72：确认删除

</details>

## features/admin/components/roles/delete-role-dialog.tsx

**节点：** AlertDialog（行 33）

**建议：** D18

**直接导入方：** [features/admin/delete-role-dialog.ts](../../erp-client/features/admin/delete-role-dialog.ts#L6)

<details><summary>本文件中文字符串索引（包含非弹窗、隐藏说明与动态分支，不等同可见弹窗文案）</summary>

- L36：删除角色
- L38：删除后该角色及权限策略一并移除，已绑定该角色的账号将失去对应权限。系统内置角色会被后端拒绝删除。
- L43：删除失败
- L52：取消
- L65：删除失败，请重试。
- L70：删除中…
- L70：确认删除

</details>

## features/admin/pages/accounts-page.tsx

**节点：** AccountFormDialog（行 375）；DeleteAdminDialog（行 391）

**建议：** 未列为优先修改项：保留当前结构；调用/装配代码跟随关联弹窗调整。未逐状态实操，不构成运行时验收。

**直接导入方：** [app/(workspace)/system/accounts/page.tsx](../../erp-client/app/(workspace)/system/accounts/page.tsx#L4)

## features/approval-processes/components/create-draft-dialog.tsx

**节点：** Dialog（行 97）

**建议：** D17

**直接导入方：** [features/approval-processes/pages/approval-processes-page.tsx](../../erp-client/features/approval-processes/pages/approval-processes-page.tsx#L26)、[features/approval-processes/pages/approval-process-detail-page.tsx](../../erp-client/features/approval-processes/pages/approval-process-detail-page.tsx#L24)

<details><summary>本文件中文字符串索引（包含非弹窗、隐藏说明与动态分支，不等同可见弹窗文案）</summary>

- L65：当前没有可复制的已发布版本，请改用空白流程。
- L100：新建草稿
- L102：为
- L107：创建更高版本草稿。必须明确选择来源，不会默认复制历史版本。
- L112：未能创建草稿
- L127：审批流程名称
- L136：草稿来源
- L181：（当前没有已发布版本）
- L192：请选择草稿来源
- L208：取消
- L213：创建草稿

</details>

## features/approval-processes/components/publish-dialog.tsx

**节点：** Dialog（行 89）

**建议：** D17

**直接导入方：** [features/approval-processes/pages/approval-process-detail-page.tsx](../../erp-client/features/approval-processes/pages/approval-process-detail-page.tsx#L27)

<details><summary>本文件中文字符串索引（包含非弹窗、隐藏说明与动态分支，不等同可见弹窗文案）</summary>

- L92：发布审批流程
- L103：尚未配置审批人
- L109：发布后当前已发布版本将退役。已绑定单据和进行中的审批不受影响。
- L114：未能发布
- L126：取消
- L134：正在发布…
- L134：确认发布

</details>

## features/approval-processes/components/retire-dialog.tsx

**节点：** Dialog（行 77）

**建议：** D17

**直接导入方：** [features/approval-processes/pages/approval-process-detail-page.tsx](../../erp-client/features/approval-processes/pages/approval-process-detail-page.tsx#L28)

<details><summary>本文件中文字符串索引（包含非弹窗、隐藏说明与动态分支，不等同可见弹窗文案）</summary>

- L80：退役审批流程
- L90：退役只影响新单据绑定。已有单据和进行中的审批仍使用原版本。                     退役后若没有新的已发布版本，该单据类型将无法创建新单据。
- L95：未能退役
- L107：取消
- L116：正在退役…
- L116：确认退役

</details>

## features/approval-processes/pages/approval-process-detail-page.tsx

**节点：** CreateDraftDialog（行 492）；PublishDialog（行 503）；RetireDialog（行 541）

**建议：** 未列为优先修改项：保留当前结构；调用/装配代码跟随关联弹窗调整。未逐状态实操，不构成运行时验收。

**直接导入方：** [app/(workspace)/system/approval-processes/[documentType]/page.tsx](../../erp-client/app/(workspace)/system/approval-processes/[documentType]/page.tsx#L4)

## features/approval-processes/pages/approval-processes-page.tsx

**节点：** CreateDraftDialog（行 437）

**建议：** 未列为优先修改项：保留当前结构；调用/装配代码跟随关联弹窗调整。未逐状态实操，不构成运行时验收。

**直接导入方：** [app/(workspace)/system/approval-processes/page.tsx](../../erp-client/app/(workspace)/system/approval-processes/page.tsx#L4)

## features/approval-workflow/components/approval-action-bar.tsx

**节点：** DecisionDialog（行 187）；ResumeApproverDialog（行 200）；CancelApprovalDialog（行 212）；UpgradeBindingDialog（行 234）

**建议：** 未列为优先修改项：保留当前结构；调用/装配代码跟随关联弹窗调整。未逐状态实操，不构成运行时验收。

**直接导入方：** [features/sales-orders/components/sales-change-order-approval-area.tsx](../../erp-client/features/sales-orders/components/sales-change-order-approval-area.tsx#L3)、[features/sales-orders/components/voucher-sales-order-approval-area.tsx](../../erp-client/features/sales-orders/components/voucher-sales-order-approval-area.tsx#L3)、[features/sales-orders/components/sales-order-approval-area.tsx](../../erp-client/features/sales-orders/components/sales-order-approval-area.tsx#L3)、[features/inventory/components/adjustment-approval-area.tsx](../../erp-client/features/inventory/components/adjustment-approval-area.tsx#L3)、[features/workspace/components/workspace-task-detail.tsx](../../erp-client/features/workspace/components/workspace-task-detail.tsx#L9)、[features/purchase-orders/components/purchase-order-approval-area.tsx](../../erp-client/features/purchase-orders/components/purchase-order-approval-area.tsx#L3)、[features/purchase-orders/components/purchase-change-order-approval-area.tsx](../../erp-client/features/purchase-orders/components/purchase-change-order-approval-area.tsx#L3)、[features/supplier-payables/components/supplier-refund-approval-area.tsx](../../erp-client/features/supplier-payables/components/supplier-refund-approval-area.tsx#L3)、[features/supplier-payables/components/payment-reversal-approval-area.tsx](../../erp-client/features/supplier-payables/components/payment-reversal-approval-area.tsx#L3)、[features/customer-receivables/components/customer-refund-approval-area.tsx](../../erp-client/features/customer-receivables/components/customer-refund-approval-area.tsx#L3)、[features/customer-receivables/components/customer-receipt-approval-area.tsx](../../erp-client/features/customer-receivables/components/customer-receipt-approval-area.tsx#L3)、[features/customer-receivables/components/receipt-reversal-approval-area.tsx](../../erp-client/features/customer-receivables/components/receipt-reversal-approval-area.tsx#L3)

## features/approval-workflow/components/cancel-approval-dialog.tsx

**节点：** Dialog（行 131）

**建议：** D16

**直接导入方：** [features/approval-workflow/components/approval-action-bar.tsx](../../erp-client/features/approval-workflow/components/approval-action-bar.tsx#L15)

<details><summary>本文件中文字符串索引（包含非弹窗、隐藏说明与动态分支，不等同可见弹窗文案）</summary>

- L126：应急撤回审批
- L127：撤回审批
- L128：取消受阻审批
- L136：当前节点：
- L136：。撤回后单据将回到
- L139：此操作不可恢复，不会改派或继续推进。
- L142：你正在代原提交人撤回，系统会记录应急代办身份。
- L158：原因
- L177：取消
- L184：确认撤回
- L185：确认取消

</details>

## features/approval-workflow/components/decision-dialog.tsx

**节点：** Dialog（行 123）

**建议：** D16

**直接导入方：** [features/approval-workflow/components/approval-action-bar.tsx](../../erp-client/features/approval-workflow/components/approval-action-bar.tsx#L16)

<details><summary>本文件中文字符串索引（包含非弹窗、隐藏说明与动态分支，不等同可见弹窗文案）</summary>

- L136：确认驳回
- L136：确认通过
- L140：请核对当前任务。确认后将驳回，并从第一节点开始下一轮审批。
- L141：请核对单据、金额、当前节点和结果影响。只有确认后才会提交审批决定。
- L153：单据
- L155：当前任务
- L157：金额
- L159：未提供金额
- L161：当前节点
- L164：当前节点待加载
- L166：结果影响
- L170：驳回后从首节点进入下一轮审批。
- L171：通过后进入下一审批节点；如无后续节点，则完成本轮审批。
- L205：驳回原因
- L206：原因（可选）
- L232：取消
- L239：确认驳回
- L240：确认通过

</details>

## features/approval-workflow/components/resume-approver-dialog.tsx

**节点：** Dialog（行 77）

**建议：** D16

**直接导入方：** [features/approval-workflow/components/approval-action-bar.tsx](../../erp-client/features/approval-workflow/components/approval-action-bar.tsx#L17)

<details><summary>本文件中文字符串索引（包含非弹窗、隐藏说明与动态分支，不等同可见弹窗文案）</summary>

- L80：恢复当前审批人
- L82：将为原审批人创建新的办理记录和新的待办，不会重开旧任务或重放原决定。
- L98：取消
- L106：提交中
- L106：确认恢复

</details>

## features/approval-workflow/components/upgrade-binding-dialog.tsx

**节点：** Dialog（行 110）

**建议：** D16

**直接导入方：** [features/approval-workflow/components/approval-action-bar.tsx](../../erp-client/features/approval-workflow/components/approval-action-bar.tsx#L18)

<details><summary>本文件中文字符串索引（包含非弹窗、隐藏说明与动态分支，不等同可见弹窗文案）</summary>

- L75：单据或流程版本已变化，请刷新后重新确认
- L116：更新审批流程版本
- L118：当前绑定
- L118：，将更新到
- L118：。                         不会改动单据内容，只替换尚未提交的审批路线。
- L123：当前路线：
- L124：更新后路线：
- L138：更新原因
- L157：取消
- L162：确认更新

</details>

## features/contracts/components/contract-paper-dialog.tsx

**节点：** Dialog（行 36）

**建议：** D24

**直接导入方：** [features/contracts/pages/contracts-list-page.tsx](../../erp-client/features/contracts/pages/contracts-list-page.tsx#L12)、[features/contracts/pages/contract-detail-page.tsx](../../erp-client/features/contracts/pages/contract-detail-page.tsx#L18)、[features/contracts/contract-paper-dialog.ts](../../erp-client/features/contracts/contract-paper-dialog.ts#L2)

<details><summary>本文件中文字符串索引（包含非弹窗、隐藏说明与动态分支，不等同可见弹窗文案）</summary>

- L42：合同纸质预览
- L44：系统已确认修订的打印件；条款、状态与签章位以系统记录为准。
- L67：关闭
- L76：打印
- L91：付款条件
- L96：开票要求
- L101：条款摘要
- L106：有效期
- L114：销售合同
- L122：甲方（销售方）
- L124：结算主体
- L128：业务负责人
- L135：乙方（客户）
- L141：结算主体
- L146：签订日
- L156：有效期
- L162：生效时间
- L168：付款条件
- L173：开票类型
- L177：合同条款打印件
- L181：条款
- L186：内容
- L199：金额说明
- L200：金额以各销售单含税金额为准，不设合同级金额。
- L206：历史销售单按签订时的合同版本履约与结算。
- L208：（签章位以系统打印版式为准）
- L209：公章

</details>

## features/contracts/components/contract-upload-dialog.tsx

**节点：** Dialog（行 52）

**建议：** D19

**直接导入方：** [features/contracts/contract-upload-dialog.ts](../../erp-client/features/contracts/contract-upload-dialog.ts#L2)、[features/contracts/contract-upload-dialog.ts](../../erp-client/features/contracts/contract-upload-dialog.ts#L3)、[features/contracts/pages/contracts-list-page.tsx](../../erp-client/features/contracts/pages/contracts-list-page.tsx#L14)

<details><summary>本文件中文字符串索引（包含非弹窗、隐藏说明与动态分支，不等同可见弹窗文案）</summary>

- L64：上传合同 PDF
- L66：系统不新建或编辑合同正文；上传已签署电子档并补充检索信息后，形成可引用的合同版本。
- L80：合同 PDF 未归档
- L94：合同电子档
- L106：合同编号
- L126：客户
- L156：搜索客户编号或名称
- L183：结算主体
- L210：搜索结算主体
- L226：付款条件
- L228：用于销售单快速带出；完整条款以 PDF 为准。
- L242：签订日期
- L251：有效期起
- L260：有效期止
- L276：取消
- L283：上传中…
- L284：上传并归档

</details>

## features/contracts/pages/contract-detail-page.tsx

**节点：** ContractPaperDialog（行 169）

**建议：** 未列为优先修改项：保留当前结构；调用/装配代码跟随关联弹窗调整。未逐状态实操，不构成运行时验收。

**直接导入方：** [app/(workspace)/sales/contracts/[contractId]/page.tsx](../../erp-client/app/(workspace)/sales/contracts/[contractId]/page.tsx#L4)

## features/contracts/pages/contracts-list-page.tsx

**节点：** ContractPaperDialog（行 137）；ContractUploadDialog（行 145）

**建议：** 未列为优先修改项：保留当前结构；调用/装配代码跟随关联弹窗调整。未逐状态实操，不构成运行时验收。

**直接导入方：** [app/(workspace)/sales/contracts/page.tsx](../../erp-client/app/(workspace)/sales/contracts/page.tsx#L3)

## features/customer-quality/components/business-tag-dialog.tsx

**节点：** Dialog（行 24）

**建议：** D24

**直接导入方：** [features/customer-quality/components/customer-quality-main-view.tsx](../../erp-client/features/customer-quality/components/customer-quality-main-view.tsx#L24)

<details><summary>本文件中文字符串索引（包含非弹窗、隐藏说明与动态分支，不等同可见弹窗文案）</summary>

- L29：经营标签说明
- L32：标签由系统固定规则生成，页面不提供人工修改入口。
- L44：规则版本
- L48：规模
- L50：利润贡献
- L51：回款风险
- L59：卡券收入进入规模和回款分析，但不进入利润贡献标签与实际盈亏。

</details>

## features/customer-quality/components/customer-quality-main-view.tsx

**节点：** BusinessTagDialog（行 238）

**建议：** 未列为优先修改项：保留当前结构；调用/装配代码跟随关联弹窗调整。未逐状态实操，不构成运行时验收。

**直接导入方：** [features/customer-quality/pages/customer-quality-page.tsx](../../erp-client/features/customer-quality/pages/customer-quality-page.tsx#L28)

## features/customer-receivables/components/allocation-session-panel.tsx

**节点：** DiscardConfirmDialog（行 380）；SessionRemoveLineDialog（行 395）；CustomerReceiptSubmitConfirmDialog（行 404）；FormalActionConfirmDialog（行 413）

**建议：** D04、D20

**直接导入方：** [features/customer-receivables/pages/components/allocation-session-screen.tsx](../../erp-client/features/customer-receivables/pages/components/allocation-session-screen.tsx#L7)

<details><summary>本文件中文字符串索引（包含非弹窗、隐藏说明与动态分支，不等同可见弹窗文案）</summary>

- L46：返回列表
- L147：查询中…
- L148：查询最终结果
- L158：返回销售单
- L177：操作未成功
- L211：本次分配
- L212：拟分配金额仅供参考，以提交后结果为准。
- L227：拟
- L238：拟未分配
- L248：从池中选择
- L249：请从左侧同主体池加入目标
- L254：目标
- L266：开放余额
- L278：分配金额
- L306：填满
- L314：分配校验
- L348：保存中…
- L348：保存草稿
- L372：提交中…
- L373：确认登记并核销
- L384：本次核销尚未保存草稿，确定离开？
- L385：记录表单与分配金额尚未保存，离开后将丢失；可先「保存草稿」再离开。
- L386：放弃并离开
- L387：继续编辑
- L417：确认登记销项发票并分配
- L418：提交
- L419：确认提交
- L420：本次草稿
- L422：已登记发票
- L426：往来主体
- L427：记录编号（提交后）
- L428：既有分配行
- L431：形成发票记录与追加式分配明细
- L432：同步更新应收开放余额与净分配（系统）
- L433：未分配余额按系统策略保留并可见
- L434：重复提交不会重复生成记录
- L436：财务

</details>

## features/customer-receivables/components/customer-receipt-submit-confirm-dialog.tsx

**节点：** FormalActionConfirmDialog（行 30）

**建议：** D01

**直接导入方：** [features/customer-receivables/components/allocation-session-panel.tsx](../../erp-client/features/customer-receivables/components/allocation-session-panel.tsx#L22)

<details><summary>本文件中文字符串索引（包含非弹窗、隐藏说明与动态分支，不等同可见弹窗文案）</summary>

- L34：提交回款
- L35：确认提交
- L36：草稿
- L37：审批中
- L40：确认后启动审批。任一层驳回后将从第一节点开始下一轮。
- L47：往来主体
- L47：到账金额
- L47：已绑定的审批流程
- L49：内容锁定并进入审批
- L50：按已绑定的审批流程办理
- L51：全部节点通过后过账并核销
- L53：形成提交并进入审批

</details>

## features/customer-receivables/components/customer-receivables-workspace.tsx

**节点：** CustomerRefundRequestDialog（行 596）；CustomerRefundSubmitConfirmDialog（行 616）；ReceiptReversalRequestDialog（行 633）；ReceiptReversalSubmitConfirmDialog（行 653）

**建议：** D03

**直接导入方：** [features/customer-receivables/pages/customer-receivables-page.tsx](../../erp-client/features/customer-receivables/pages/customer-receivables-page.tsx#L3)

## features/customer-receivables/components/customer-refund-request-dialog.tsx

**节点：** Dialog（行 58）

**建议：** D03

**直接导入方：** [features/customer-receivables/components/customer-receivables-workspace.tsx](../../erp-client/features/customer-receivables/components/customer-receivables-workspace.tsx#L19)

<details><summary>本文件中文字符串索引（包含非弹窗、隐藏说明与动态分支，不等同可见弹窗文案）</summary>

- L20：请填写原因说明
- L61：发起客户退款
- L63：不编辑、不删除已确认记录与分配；仅追加退款记录。原单
- L64：。退款表示向客户退回资金。
- L75：将按原单全额追加退款
- L82：，原记录保留。
- L89：原因说明
- L90：业务依据与说明
- L109：取消
- L114：下一步

</details>

## features/customer-receivables/components/customer-refund-submit-confirm-dialog.tsx

**节点：** FormalActionConfirmDialog（行 30）

**建议：** D01、D03

**直接导入方：** [features/customer-receivables/components/customer-receivables-workspace.tsx](../../erp-client/features/customer-receivables/components/customer-receivables-workspace.tsx#L20)

<details><summary>本文件中文字符串索引（包含非弹窗、隐藏说明与动态分支，不等同可见弹窗文案）</summary>

- L34：提交退款
- L35：确认提交
- L36：草稿
- L37：审批中
- L40：确认后启动审批。任一层驳回后将从第一节点开始下一轮。
- L47：往来主体
- L47：退款金额
- L47：已绑定的审批流程
- L49：内容锁定并进入审批
- L50：按已绑定的审批流程办理
- L51：全部节点通过后过账并出账
- L53：形成提交并进入审批

</details>

## features/customer-receivables/components/receipt-reversal-request-dialog.tsx

**节点：** Dialog（行 58）

**建议：** D03

**直接导入方：** [features/customer-receivables/components/customer-receivables-workspace.tsx](../../erp-client/features/customer-receivables/components/customer-receivables-workspace.tsx#L21)

<details><summary>本文件中文字符串索引（包含非弹窗、隐藏说明与动态分支，不等同可见弹窗文案）</summary>

- L20：请填写原因说明
- L61：发起回款冲正
- L63：不编辑、不删除已确认记录与分配；仅追加冲正记录。原单
- L64：。冲正表示撤销本次回款记录。
- L75：将按原单全额追加冲正
- L82：，原记录保留。
- L89：原因说明
- L90：业务依据与说明
- L109：取消
- L114：下一步

</details>

## features/customer-receivables/components/receipt-reversal-submit-confirm-dialog.tsx

**节点：** FormalActionConfirmDialog（行 30）

**建议：** D01、D03

**直接导入方：** [features/customer-receivables/components/customer-receivables-workspace.tsx](../../erp-client/features/customer-receivables/components/customer-receivables-workspace.tsx#L22)

<details><summary>本文件中文字符串索引（包含非弹窗、隐藏说明与动态分支，不等同可见弹窗文案）</summary>

- L34：提交冲正
- L35：确认提交
- L36：草稿
- L37：审批中
- L40：确认后启动审批。任一层驳回后将从第一节点开始下一轮。
- L47：往来主体
- L47：冲正金额
- L47：已绑定的审批流程
- L49：内容锁定并进入审批
- L50：按已绑定的审批流程办理
- L51：全部节点通过后过账并冲减原回款
- L53：形成提交并进入审批

</details>

## features/customer-receivables/components/receivable-action-dialogs.tsx

**节点：** Dialog（行 69）；Dialog（行 137）

**建议：** D20

**直接导入方：** [features/customer-receivables/components/customer-receivables-workspace.tsx](../../erp-client/features/customer-receivables/components/customer-receivables-workspace.tsx#L23)

<details><summary>本文件中文字符串索引（包含非弹窗、隐藏说明与动态分支，不等同可见弹窗文案）</summary>

- L77：登记回款 — 选择往来主体
- L78：登记销项发票 — 选择往来主体
- L81：本次核销创建后锁定往来主体，中途不可更换。                             经营客户与结算主体可能不同。
- L87：往来主体
- L97：往来主体
- L98：请选择往来主体
- L109：取消
- L131：创建中…
- L131：打开核销工作区
- L145：发起销项红票
- L147：发起客户退款
- L148：发起回款冲正
- L151：不编辑、不删除已确认记录与分配；仅追加反向记录。原单
- L154：冲正表示撤销本次回款记录。
- L156：退款表示向客户退回资金。
- L157：红票表示冲减原票的分配。
- L164：红票金额
- L179：默认按原票有效净已分配全额；可输入部分金额。
- L184：将按原单全额追加反向记录
- L196：，原记录保留。
- L201：原因说明
- L209：业务依据与说明
- L221：取消
- L240：提交中…
- L240：确认追加反向记录

</details>

## features/customer-receivables/components/session-remove-line-dialog.tsx

**节点：** Dialog（行 23）

**建议：** D04

**直接导入方：** [features/customer-receivables/components/allocation-session-panel.tsx](../../erp-client/features/customer-receivables/components/allocation-session-panel.tsx#L20)

<details><summary>本文件中文字符串索引（包含非弹窗、隐藏说明与动态分支，不等同可见弹窗文案）</summary>

- L26：移除该分配行？
- L28：该行金额将不再分配，需重新输入或从池中再次加入。
- L38：取消
- L49：确认移除

</details>

## features/customers/components/customer-assignment-dialog.tsx

**节点：** Dialog（行 97）

**建议：** D19

**直接导入方：** [features/customers/pages/customer-detail-page.tsx](../../erp-client/features/customers/pages/customer-detail-page.tsx#L21)

<details><summary>本文件中文字符串索引（包含非弹窗、隐藏说明与动态分支，不等同可见弹窗文案）</summary>

- L32：归属调整失败，请重试。
- L90：当前归属调整尚未提交，刷新后将丢失。
- L101：结束协作归属
- L101：调整客户归属
- L105：结束日期当日起不再计入协作范围，历史责任关系保留。
- L106：换任负责人会结束重叠的旧负责人归属；新增协作不会改变负责人。
- L118：· 协作销售 ·
- L119：起
- L128：责任角色
- L143：负责销售
- L146：协作销售
- L162：销售人员
- L193：生效日期
- L222：结束日期（可选）
- L260：结束日期
- L290：调整原因
- L293：说明换任、协作或结束原因
- L310：取消
- L315：确认结束
- L315：确认调整

</details>

## features/customers/components/customer-create-dialog.tsx

**节点：** Dialog（行 27）

**建议：** D19

**直接导入方：** [features/customers/pages/customer-center-page.tsx](../../erp-client/features/customers/pages/customer-center-page.tsx#L20)

<details><summary>本文件中文字符串索引（包含非弹窗、隐藏说明与动态分支，不等同可见弹窗文案）</summary>

- L33：新建客户
- L35：创建客户主体与首版资料；名称相似只提示候选，不自动合并。
- L44：客户已创建
- L47：客户资料已生效，可在客户列表继续查看。

</details>

## features/customers/components/customer-form.tsx

**节点：** ConflictResolutionDialog（行 354）；DiscardConfirmDialog（行 410）

**建议：** 未列为优先修改项：保留当前结构；调用/装配代码跟随关联弹窗调整。未逐状态实操，不构成运行时验收。

**直接导入方：** [features/customers/pages/customer-detail-overview.tsx](../../erp-client/features/customers/pages/customer-detail-overview.tsx#L15)、[features/customers/components/customer-create-dialog.tsx](../../erp-client/features/customers/components/customer-create-dialog.tsx#L11)

## features/customers/pages/customer-center-page.tsx

**节点：** CustomerCreateDialog（行 244）

**建议：** 未列为优先修改项：保留当前结构；调用/装配代码跟随关联弹窗调整。未逐状态实操，不构成运行时验收。

**直接导入方：** [app/(workspace)/sales/customers/page.tsx](../../erp-client/app/(workspace)/sales/customers/page.tsx#L4)

## features/customers/pages/customer-detail-page.tsx

**节点：** CustomerAssignmentDialog（行 278）；DiscardConfirmDialog（行 287）

**建议：** 未列为优先修改项：保留当前结构；调用/装配代码跟随关联弹窗调整。未逐状态实操，不构成运行时验收。

**直接导入方：** [app/(workspace)/sales/customers/[customerId]/page.tsx](../../erp-client/app/(workspace)/sales/customers/[customerId]/page.tsx#L4)

## features/finance-responsibilities/components/finance-responsibility-rules-page.tsx

**节点：** Dialog（行 165）；RuleDialog（行 562）

**建议：** D23

**直接导入方：** [app/(workspace)/finance/responsibilities/page.tsx](../../erp-client/app/(workspace)/finance/responsibilities/page.tsx#L3)

<details><summary>本文件中文字符串索引（包含非弹窗、隐藏说明与动态分支，不等同可见弹窗文案）</summary>

- L68：请选择负责人
- L78：请选择供应商
- L79：请选择客户
- L146：财务责任规则已更新
- L146：财务责任规则已新增
- L148：新形成的付款或开票任务使用最新规则；已有任务负责人保持不变。
- L172：编辑财务责任规则
- L172：新增财务责任规则
- L175：指定往来方规则优先于同业务的默认负责人。保存只影响新任务；已有任务请在工作台转交。
- L190：业务操作
- L208：匹配层级
- L213：每项业务最多启用一条默认规则；指定往来方可覆盖默认负责人。
- L237：供应商
- L238：客户
- L260：搜索供应商编号或名称
- L279：搜索客户编号或名称
- L290：负责人
- L295：选择具备完整执行权限的账号
- L296：没有符合资格的负责人，请先配置角色权限
- L310：启用规则
- L313：停用后不再参与新任务负责人解析。
- L332：取消
- L349：保存中…
- L349：保存规则
- L378：财务
- L378：财务责任配置
- L389：财务
- L389：财务责任配置
- L392：权限信息加载失败
- L395：暂时无法核对财务责任配置权限。
- L404：重试
- L414：财务
- L414：财务责任配置
- L417：权限不足
- L418：当前账号不能查看财务责任配置。
- L439：财务
- L440：财务责任配置
- L441：为供应商付款和客户销项开票指定具体负责人；指定往来方优先，默认规则兜底。
- L453：新增规则
- L460：负责人候选加载失败
- L463：暂时无法读取具备付款或开票权限的账号，当前不能编辑规则。
- L472：重试
- L478：负责人候选加载中
- L480：加载完成后可新增或编辑财务责任规则。
- L489：财务责任规则列表
- L507：规则加载失败
- L510：暂时无法读取财务责任规则。
- L521：重试
- L534：还没有财务责任规则
- L535：请先分别配置供应商付款和销项开票的默认负责人。
- L545：新增规则

</details>

## features/fulfillment-operations/pages/components/fulfillment-operations-workspace.tsx

**节点：** FormalActionConfirmDialog（行 205）

**建议：** D24

**直接导入方：** [features/fulfillment-operations/pages/fulfillment-operations-page.tsx](../../erp-client/features/fulfillment-operations/pages/fulfillment-operations-page.tsx#L29)、[features/workspace/components/workspace-fulfillment-task.tsx](../../erp-client/features/workspace/components/workspace-fulfillment-task.tsx#L21)

<details><summary>本文件中文字符串索引（包含非弹窗、隐藏说明与动态分支，不等同可见弹窗文案）</summary>

- L90：没有生效
- L142：有未保存修改，请先保存或放弃后再切换
- L214：确认？
- L219：确认后不能改。
- L226：确认
- L233：确认
- L235：待确认
- L241：已完成

</details>

## features/import-opening/components/confirm-section.tsx

**节点：** Dialog（行 69）；FormalActionConfirmDialog（行 312）；ReturnForFixDialog（行 333）

**建议：** D21

**直接导入方：** [features/import-opening/components/batch-detail-sections.tsx](../../erp-client/features/import-opening/components/batch-detail-sections.tsx#L9)

<details><summary>本文件中文字符串索引（包含非弹窗、隐藏说明与动态分支，不等同可见弹窗文案）</summary>

- L40：试算数据与业务事实不一致
- L41：导入口径或规则不一致
- L42：缺少必要核对依据
- L43：其它需修复问题
- L47：请选择退回原因
- L48：请填写至少 3 个字的修复说明
- L81：退回
- L81：修复
- L84：本次试算会形成已退回结论并完成当前任务；修复并重新试算后，系统才会创建新任务。
- L99：退回原因
- L111：修复说明
- L113：说明需要修复的数据、口径或依据
- L129：返回核对
- L134：确认退回修复
- L135：正在提交
- L172：责任确认任务不完整
- L173：当前试算缺少已登记的责任确认任务，不能提交确认或退回。请联系管理员重新生成确认任务。
- L178：责任确认未完成
- L209：已确认
- L212：已退回
- L215：已失效
- L216：待确认
- L231：试算版本
- L236：· 当前待处理入口
- L238：· 由本人负责
- L239：· 只读
- L245：确认人
- L268：确认本范围
- L280：退回修复
- L288：当前确认任务不完整，入口已阻断
- L290：本范围已有正式结论或已失效
- L292：当前范围不由本人处理
- L319：确认本范围
- L320：系统将记录本范围正式确认事实，并在同一操作中完成当前任务。
- L321：待确认
- L322：已确认
- L323：记录责任范围确认结论
- L323：完成当前处理任务
- L325：结论写入审计，试算变化后由新任务重新确认

</details>

## features/import-opening/components/execution-actions.tsx

**节点：** Dialog（行 62）；FormalActionConfirmDialog（行 214）；FormalActionConfirmDialog（行 233）；CancelPendingDialog（行 255）

**建议：** D21

**直接导入方：** [features/import-opening/components/batch-detail-sections.tsx](../../erp-client/features/import-opening/components/batch-detail-sections.tsx#L10)

<details><summary>本文件中文字符串索引（包含非弹窗、隐藏说明与动态分支，不等同可见弹窗文案）</summary>

- L35：操作人终止本批未应用项
- L36：导入数据范围已变化
- L37：业务执行窗口已关闭
- L38：其它终止原因
- L42：请选择取消原因
- L43：操作说明不能超过 1024 个字符
- L73：取消尚未应用项
- L75：系统只停止本批尚未应用的项；已成功、已跳过及已形成的业务事实保持不变。
- L90：取消原因
- L102：操作说明（可选）
- L104：补充取消范围或业务窗口信息
- L119：返回
- L124：确认取消未应用项
- L125：正在取消
- L163：导入执行
- L165：责任确认只形成待应用状态；只有“提交应用”会启动后台任务。取消和失败项重试均保留已形成的业务事实。
- L171：导入操作未完成
- L184：提交应用
- L196：取消未应用项
- L207：重新准备失败项
- L220：提交导入应用
- L221：确认提交应用
- L222：系统将再次核验批次和试算版本，随后把批次推进为导入中并启动后台任务。
- L223：待应用
- L224：导入中
- L225：启动关联后台任务
- L225：只处理当前仍待应用的项
- L226：已形成的业务对象不会由本批自动回滚
- L239：重新准备失败项
- L240：确认重新准备
- L241：系统只把上一轮失败行重新准备为待应用，不会在本动作中启动后台任务。
- L242：失败结果
- L243：待应用
- L245：保留已成功与已跳过结果
- L246：仅清理失败行的上次失败诊断
- L248：准备完成后仍需再次点击“提交应用”

</details>

## features/integration-errors/components/terminal-action-dialog.tsx

**节点：** FormalActionConfirmDialog（行 34）；FormalActionConfirmDialog（行 58）；FormalActionConfirmDialog（行 85）

**建议：** D22

**直接导入方：** [features/integration-errors/pages/hooks/use-integration-actions.ts](../../erp-client/features/integration-errors/pages/hooks/use-integration-actions.ts#L7)、[features/integration-errors/pages/components/integration-action-zone.tsx](../../erp-client/features/integration-errors/pages/components/integration-action-zone.tsx#L16)、[features/integration-errors/pages/components/integration-terminal-confirmation.tsx](../../erp-client/features/integration-errors/pages/components/integration-terminal-confirmation.tsx#L1)、[features/integration-errors/pages/components/integration-terminal-confirmation.tsx](../../erp-client/features/integration-errors/pages/components/integration-terminal-confirmation.tsx#L2)、[features/integration-errors/pages/components/integration-direct-reconciliation.tsx](../../erp-client/features/integration-errors/pages/components/integration-direct-reconciliation.tsx#L5)

<details><summary>本文件中文字符串索引（包含非弹窗、隐藏说明与动态分支，不等同可见弹窗文案）</summary>

- L44：关闭重复
- L44：关闭误派
- L45：确认关闭重复任务
- L45：确认关闭误派任务
- L46：仅关闭当前处理任务；不写业务解决结论，不影响业务记录。
- L48：已关闭
- L49：任务退出待处理队列
- L49：不改变业务记录
- L50：关闭后不再出现在待处理列表
- L64：标记已解决
- L65：确认标记已解决
- L66：处理完成要求证据齐备；系统将按证据策略登记处理凭证。
- L68：已完成
- L74：系统将登记本次处理凭证
- L75：任务完成并退出待处理队列
- L77：处理结论写入审计，不可自动撤回
- L95：确认无误
- L95：确认有效差异
- L96：确认差异无误
- L96：确认差异为有效差异
- L97：按已选注册原因追加对账处理记录；本操作不涉及任务关闭。
- L99：已确认
- L100：按注册原因追加对账处理记录
- L100：不改变两侧业务数据
- L101：对账结论写入审计，不可自动撤回

</details>

## features/integration-errors/pages/components/integration-terminal-confirmation.tsx

**节点：** TerminalActionDialog（行 19）

**建议：** 未列为优先修改项：保留当前结构；调用/装配代码跟随关联弹窗调整。未逐状态实操，不构成运行时验收。

**直接导入方：** [features/integration-errors/pages/components/integration-error-workspace.tsx](../../erp-client/features/integration-errors/pages/components/integration-error-workspace.tsx#L17)

## features/inventory/components/adjustment-approval-area.tsx

**节点：** CancelAdjustmentApprovalDialog（行 153）

**建议：** 未列为优先修改项：保留当前结构；调用/装配代码跟随关联弹窗调整。未逐状态实操，不构成运行时验收。

**直接导入方：** [features/inventory/pages/components/adjustment-dialog.tsx](../../erp-client/features/inventory/pages/components/adjustment-dialog.tsx#L18)、[features/inventory/pages/components/adjustment-confirm-dialog.tsx](../../erp-client/features/inventory/pages/components/adjustment-confirm-dialog.tsx#L4)、[features/inventory/pages/components/adjustment-detail-sheet.tsx](../../erp-client/features/inventory/pages/components/adjustment-detail-sheet.tsx#L10)、[features/inventory/pages/components/adjustment-detail-sheet.tsx](../../erp-client/features/inventory/pages/components/adjustment-detail-sheet.tsx#L11)

## features/inventory/components/cancel-adjustment-approval-dialog.tsx

**节点：** Dialog（行 96）

**建议：** 未列为优先修改项：保留当前结构；调用/装配代码跟随关联弹窗调整。未逐状态实操，不构成运行时验收。

**直接导入方：** [features/inventory/components/adjustment-approval-area.tsx](../../erp-client/features/inventory/components/adjustment-approval-area.tsx#L19)

<details><summary>本文件中文字符串索引（包含非弹窗、隐藏说明与动态分支，不等同可见弹窗文案）</summary>

- L94：撤回审批
- L99：撤回审批
- L101：当前节点：
- L102：。撤回后库存调整单将回到草稿。
- L117：原因
- L140：取消
- L145：确认撤回

</details>

## features/inventory/pages/components/adjustment-confirm-dialog.tsx

**节点：** FormalActionConfirmDialog（行 23）

**建议：** D01、D05

**直接导入方：** [features/inventory/pages/inventory-ledger-page.tsx](../../erp-client/features/inventory/pages/inventory-ledger-page.tsx#L21)

<details><summary>本文件中文字符串索引（包含非弹窗、隐藏说明与动态分支，不等同可见弹窗文案）</summary>

- L27：提交库存调整
- L28：确认提交
- L29：草稿
- L30：审批中
- L33：确认后启动审批。余额在审批通过前不会变化。
- L41：当前余额
- L42：已按当前数据版本核对
- L45：创建审批中的库存调整单
- L46：不立即修改账面、预占和可用数量
- L47：经办人不得自行审批本单
- L49：形成调整单号并进入审批

</details>

## features/inventory/pages/components/adjustment-dialog.tsx

**节点：** Dialog（行 41）

**建议：** D05

**直接导入方：** [features/inventory/pages/inventory-ledger-page.tsx](../../erp-client/features/inventory/pages/inventory-ledger-page.tsx#L23)

<details><summary>本文件中文字符串索引（包含非弹窗、隐藏说明与动态分支，不等同可见弹窗文案）</summary>

- L52：发起库存调整
- L54：从当前余额上下文创建调整单草稿。提交后进入审批，不会立即改库存。
- L70：账面现存
- L76：可用
- L82：草稿号
- L88：数据版本
- L90：已按最新核对
- L98：岗位分离
- L113：原因类型
- L135：增加
- L136：减少
- L142：原因类型
- L143：原因类型
- L165：业务发生时间
- L199：原因说明
- L214：提交约束
- L217：不会直接修改账面或可用数量
- L218：经办与审批岗位分离，提交后进入审批
- L220：按当前数据版本提交；若已被他人修改，将提示冲突并保留你的输入。
- L232：取消
- L237：提交审批

</details>

## features/inventory/pages/inventory-ledger-page.tsx

**节点：** AdjustmentDialog（行 459）；AdjustmentConfirmDialog（行 466）

**建议：** 未列为优先修改项：保留当前结构；调用/装配代码跟随关联弹窗调整。未逐状态实操，不构成运行时验收。

**直接导入方：** [app/(workspace)/inventory/page.tsx](../../erp-client/app/(workspace)/inventory/page.tsx#L4)

## features/master-data/components/brand/brand-form-dialog-frame.tsx

**节点：** Dialog（行 80）

**建议：** D11

**直接导入方：** [features/master-data/components/brand/brand-revise-dialog.tsx](../../erp-client/features/master-data/components/brand/brand-revise-dialog.tsx#L6)、[features/master-data/components/brand/brand-create-dialog.tsx](../../erp-client/features/master-data/components/brand/brand-create-dialog.tsx#L6)

<details><summary>本文件中文字符串索引（包含非弹窗、隐藏说明与动态分支，不等同可见弹窗文案）</summary>

- L116：名称
- L142：Logo · 1:1 · 已选择
- L170：关闭
- L176：提交中…
- L178：提交中…

</details>

## features/master-data/components/category/category-form-dialog-frame.tsx

**节点：** Dialog（行 111）

**建议：** D11

**直接导入方：** [features/master-data/components/category/category-form-dialogs.tsx](../../erp-client/features/master-data/components/category/category-form-dialogs.tsx#L6)

<details><summary>本文件中文字符串索引（包含非弹窗、隐藏说明与动态分支，不等同可见弹窗文案）</summary>

- L28：实物
- L28：虚拟
- L28：服务
- L28：卡券
- L146：名称
- L178：可选上级；空为根分类
- L179：没有可选上级分类
- L183：留空表示根分类；不可选择自身或下级。
- L201：未填写
- L226：关闭
- L232：提交中…
- L234：提交中…

</details>

## features/master-data/components/list/voucher-category-form-dialog.tsx

**节点：** Dialog（行 209）

**建议：** 未列为优先修改项：保留当前结构；调用/装配代码跟随关联弹窗调整。未逐状态实操，不构成运行时验收。

**直接导入方：** [features/master-data/pages/voucher-categories-list-page.tsx](../../erp-client/features/master-data/pages/voucher-categories-list-page.tsx#L17)

<details><summary>本文件中文字符串索引（包含非弹窗、隐藏说明与动态分支，不等同可见弹窗文案）</summary>

- L42：请填写卡券类目编号
- L43：请填写卡券类目名称
- L44：请填写卡券类目描述
- L64：说明
- L119：更新
- L142：新建
- L197：卡券类目
- L199：修改名称与描述。编号不可改；分类 / 品牌 / 单位沿用创建时默认。
- L200：只需填写编号、名称与描述。分类挂到共用「卡券」根分类，品牌固定「福尚云」，单位固定「张」。
- L221：资料编号
- L236：说明
- L252：当前版本
- L272：卡券类目编号
- L273：全局唯一，同时作为商品与 SKU 编号
- L284：卡券类目名称
- L310：关闭
- L316：提交中…

</details>

## features/master-data/components/list/voucher-category-status-dialog.tsx

**节点：** Dialog（行 126）

**建议：** 未列为优先修改项：保留当前结构；调用/装配代码跟随关联弹窗调整。未逐状态实操，不构成运行时验收。

**直接导入方：** [features/master-data/pages/voucher-categories-list-page.tsx](../../erp-client/features/master-data/pages/voucher-categories-list-page.tsx#L5)、[features/master-data/components/list/voucher-category-preview-sheet.tsx](../../erp-client/features/master-data/components/list/voucher-category-preview-sheet.tsx#L6)、[features/master-data/hooks/use-dictionary-list-columns.tsx](../../erp-client/features/master-data/hooks/use-dictionary-list-columns.tsx#L18)

<details><summary>本文件中文字符串索引（包含非弹窗、隐藏说明与动态分支，不等同可见弹窗文案）</summary>

- L39：停用
- L39：启用
- L48：当前账号没有卡券类目更新权限
- L73：停用
- L73：启用
- L99：说明
- L109：资料已更新，请关闭弹窗后重新操作。
- L141：卡券类目
- L144：停用后，该类目及关联商品、SKU 将停用，SKU 同时下架。历史版本和既有单据保留。
- L145：启用后，该类目及关联商品、SKU 恢复启用。SKU 保持下架，需要销售时请在商品资料中重新上架。
- L168：取消
- L185：提交中…

</details>

## features/master-data/components/product/product-detail-dialogs.tsx

**节点：** ProductDisableDialog（行 60）；RegisterSupplyForSkuDialog（行 66）；DiscardConfirmDialog（行 82）

**建议：** 未列为优先修改项：保留当前结构；调用/装配代码跟随关联弹窗调整。未逐状态实操，不构成运行时验收。

**直接导入方：** [features/master-data/pages/product-detail-page.tsx](../../erp-client/features/master-data/pages/product-detail-page.tsx#L17)

## features/master-data/components/product/product-editor-media.tsx

**节点：** Dialog（行 334）

**建议：** D24

**直接导入方：** [features/master-data/components/product/product-media-section.tsx](../../erp-client/features/master-data/components/product/product-media-section.tsx#L4)、[features/master-data/components/product/product-sku-table.tsx](../../erp-client/features/master-data/components/product/product-sku-table.tsx#L26)

<details><summary>本文件中文字符串索引（包含非弹窗、隐藏说明与动态分支，不等同可见弹窗文案）</summary>

- L224：首图
- L295：支持多选，首张作为首图
- L296：支持多选，按顺序展示
- L346：图片预览
- L348：图片预览

</details>

## features/master-data/components/product/product-save-dialog.tsx

**节点：** Dialog（行 41）

**建议：** D12

**直接导入方：** [features/master-data/pages/product-detail-page.tsx](../../erp-client/features/master-data/pages/product-detail-page.tsx#L22)

<details><summary>本文件中文字符串索引（包含非弹窗、隐藏说明与动态分支，不等同可见弹窗文案）</summary>

- L54：创建商品
- L54：保存更新
- L57：确认本次内容及生效时间，保存后形成新的商品资料版本。
- L70：本次修改摘要
- L73：新建商品资料
- L73：本次修改
- L80：基本资料、规格与 SKU
- L81：商品资料内容未改变，将按本次原因生成新版本
- L103：继续编辑
- L110：保存中…
- L110：确认保存

</details>

## features/master-data/components/product/product-sku-table.tsx

**节点：** Dialog（行 161）

**建议：** D13、D14

**直接导入方：** [features/master-data/components/product/product-sku-section.tsx](../../erp-client/features/master-data/components/product/product-sku-section.tsx#L10)

**原生确认位置：** 352

<details><summary>本文件中文字符串索引（包含非弹窗、隐藏说明与动态分支，不等同可见弹窗文案）</summary>

- L82：未填写 SKU 名称
- L87：未填写
- L90：默认规格
- L91：待填写编码
- L157：编辑资料
- L157：查看资料
- L167：SKU 资料
- L170：修改后返回商品页统一保存；关闭此窗口会保留本次编辑内容。
- L171：查看 SKU 编码、名称和条码。
- L179：SKU 编码
- L194：系统默认生成，可手动覆盖
- L203：SKU 名称
- L216：请输入 SKU 名称
- L221：可与商品名称不同
- L230：条码
- L253：完成编辑
- L253：关闭
- L310：查看库存
- L314：保存后可查看
- L323：上架状态
- L333：已上架
- L334：已下架
- L342：SKU 启用
- L353：停用该 SKU 后，新的业务单据将选不到它；历史单据不受影响。确定停用？
- L368：启用
- L369：停用
- L424：SKU 信息
- L425：销售价
- L426：市场价
- L427：供给
- L430：库存
- L433：状态

</details>

## features/master-data/components/product/product-supply-dialog.tsx

**节点：** Dialog（行 80）

**建议：** D15

**直接导入方：** [features/master-data/pages/products-list-page.tsx](../../erp-client/features/master-data/pages/products-list-page.tsx#L21)

<details><summary>本文件中文字符串索引（包含非弹窗、隐藏说明与动态分支，不等同可见弹窗文案）</summary>

- L36：未提供
- L84：商品供给
- L89：按 SKU 查看当前启用的供应商与含税供给价。
- L95：SKU 信息加载失败
- L101：正在读取 SKU…
- L106：该商品没有启用中的 SKU
- L109：请先进入商品详情新增或启用 SKU，再添加供给。
- L116：供给信息加载失败
- L151：无供给
- L156：· 销售价
- L173：添加供给
- L179：正在读取供给…
- L183：当前无法判断该 SKU 是否存在供给。
- L187：该 SKU                                             暂无启用中的供给关系，可点击“添加供给”登记。
- L195：供应商
- L198：供应商 SKU
- L201：一件代发价
- L204：集采价
- L207：当前可供
- L221：供应商名称未返回
- L261：未更新
- L264：数量
- L266：未提供
- L291：关闭

</details>

## features/master-data/components/shared/disable-action-dialog.tsx

**节点：** Dialog（行 105）；DisableActionDialog（行 272）；FixedResourceDisableDialog（行 290）；FixedResourceDisableDialog（行 299）；FixedResourceDisableDialog（行 308）；FixedResourceDisableDialog（行 317）；FixedResourceDisableDialog（行 326）

**建议：** 未列为优先修改项：保留当前结构；调用/装配代码跟随关联弹窗调整。未逐状态实操，不构成运行时验收。

**直接导入方：** [features/master-data/pages/category-tree-page.tsx](../../erp-client/features/master-data/pages/category-tree-page.tsx#L30)、[features/master-data/pages/category-object-page.tsx](../../erp-client/features/master-data/pages/category-object-page.tsx#L6)、[features/master-data/pages/suppliers-list-page.tsx](../../erp-client/features/master-data/pages/suppliers-list-page.tsx#L21)、[features/master-data/pages/brands-list-page.tsx](../../erp-client/features/master-data/pages/brands-list-page.tsx#L24)、[features/master-data/pages/products-list-page.tsx](../../erp-client/features/master-data/pages/products-list-page.tsx#L22)、[features/master-data/pages/unit-of-measures-list-page.tsx](../../erp-client/features/master-data/pages/unit-of-measures-list-page.tsx#L17)、[features/master-data/components/warehouse/warehouse-action-dialogs.tsx](../../erp-client/features/master-data/components/warehouse/warehouse-action-dialogs.tsx#L20)、[features/master-data/components/supplier/supplier-editor-dialogs.tsx](../../erp-client/features/master-data/components/supplier/supplier-editor-dialogs.tsx#L6)、[features/master-data/components/product/product-detail-dialogs.tsx](../../erp-client/features/master-data/components/product/product-detail-dialogs.tsx#L6)

<details><summary>本文件中文字符串索引（包含非弹窗、隐藏说明与动态分支，不等同可见弹窗文案）</summary>

- L117：资料编号
- L133：说明
- L138：库存台账
- L144：打开库存台账
- L163：当前版本
- L230：关闭
- L236：提交中…

</details>

## features/master-data/components/shared/media-list-field.tsx

**节点：** Dialog（行 254）

**建议：** D24

**直接导入方：** [features/master-data/components/supplier/supplier-editor-contract-section.tsx](../../erp-client/features/master-data/components/supplier/supplier-editor-contract-section.tsx#L16)

<details><summary>本文件中文字符串索引（包含非弹窗、隐藏说明与动态分支，不等同可见弹窗文案）</summary>

- L117：文件预览失败
- L118：请稍后重试
- L196：正在打开预览…
- L265：图片预览
- L267：仅在当前登录会话中查看。

</details>

## features/master-data/components/supplier/supplier-editor-dialogs.tsx

**节点：** SupplierDisableDialog（行 50）；SupplierSaveReasonDialog（行 57）；DiscardConfirmDialog（行 74）

**建议：** D12

**直接导入方：** [features/master-data/components/supplier/supplier-editor-form.tsx](../../erp-client/features/master-data/components/supplier/supplier-editor-form.tsx#L14)

## features/master-data/components/supplier/supplier-save-reason-dialog.tsx

**节点：** Dialog（行 39）

**建议：** D12

**直接导入方：** [features/master-data/components/supplier/supplier-editor-dialogs.tsx](../../erp-client/features/master-data/components/supplier/supplier-editor-dialogs.tsx#L7)

<details><summary>本文件中文字符串索引（包含非弹窗、隐藏说明与动态分支，不等同可见弹窗文案）</summary>

- L46：确认创建
- L46：确认保存
- L50：创建后生成供应商档案；请填写创建说明。
- L51：保存将生成新版本；变更原因必填。
- L66：新建原因
- L67：说明本次修改内容，保存后形成新版本
- L87：取消
- L97：提交中…

</details>

## features/master-data/components/unit-of-measure/unit-of-measure-form-dialogs.tsx

**节点：** Dialog（行 290）

**建议：** D11

**直接导入方：** [features/master-data/pages/unit-of-measures-list-page.tsx](../../erp-client/features/master-data/pages/unit-of-measures-list-page.tsx#L13)

<details><summary>本文件中文字符串索引（包含非弹窗、隐藏说明与动态分支，不等同可见弹窗文案）</summary>

- L41：请填写名称
- L42：请填写单位代码
- L43：请填写单位符号
- L45：请填写变更原因
- L122：计量单位
- L213：资料编号
- L326：名称
- L389：关闭
- L395：提交中…
- L397：提交中…

</details>

## features/master-data/components/warehouse/warehouse-action-dialogs.tsx

**节点：** FixedResourceDisableDialog（行 52）；Dialog（行 165）

**建议：** D23

**直接导入方：** [features/master-data/pages/warehouse-object-page.tsx](../../erp-client/features/master-data/pages/warehouse-object-page.tsx#L6)、[features/master-data/pages/warehouses-list-page.tsx](../../erp-client/features/master-data/pages/warehouses-list-page.tsx#L14)

<details><summary>本文件中文字符串索引（包含非弹窗、隐藏说明与动态分支，不等同可见弹窗文案）</summary>

- L32：请选择入库经办人
- L33：请选择仓发经办人
- L131：请分别选择当前仍具备完整操作权限的入库与仓发经办人
- L143：收发责任未更新，请稍后重试。
- L168：配置收发责任
- L170：仓库
- L171：；配置保存后只用于新建的入库与仓发任务。
- L184：经办人选项加载失败
- L188：请稍后重试
- L195：没有保存
- L204：入库经办人
- L206：选择具备入库确认权限的账号
- L207：负责该仓库的采购收货与入库过账。
- L219：仓发经办人
- L221：选择具备发货确认权限的账号
- L222：负责该仓库的现货发货与入库后发货；可与入库经办人为同一人。
- L241：取消
- L255：保存配置
- L256：保存中…

</details>

## features/master-data/hooks/use-product-list-state.ts

**节点：** 原生确认 / 弹窗导入

**建议：** D13

**直接导入方：** [features/master-data/pages/products-list-page.tsx](../../erp-client/features/master-data/pages/products-list-page.tsx#L25)、[features/master-data/components/list/product-list-toolbar.tsx](../../erp-client/features/master-data/components/list/product-list-toolbar.tsx#L19)

**原生确认位置：** 317

<details><summary>本文件中文字符串索引（包含非弹窗、隐藏说明与动态分支，不等同可见弹窗文案）</summary>

- L198：不限
- L199：不限
- L331：上架状态更新失败，请刷新后重试。

</details>

## features/master-data/lib/product-form-bindings.ts

**节点：** 原生确认 / 弹窗导入

**建议：** D13

**直接导入方：** [features/master-data/pages/product-detail-page.tsx](../../erp-client/features/master-data/pages/product-detail-page.tsx#L23)

**原生确认位置：** 101、142

<details><summary>本文件中文字符串索引（包含非弹窗、隐藏说明与动态分支，不等同可见弹窗文案）</summary>

- L34：商品详情
- L53：仅实物商品适用公司自有库存台账
- L55：选择实物商品类型并保存 SKU 后可查看正式库存

</details>

## features/master-data/pages/brands-list-page.tsx

**节点：** BrandCreateDialog（行 226）；BrandReviseDialog（行 230）；BrandDisableDialog（行 237）

**建议：** 未列为优先修改项：保留当前结构；调用/装配代码跟随关联弹窗调整。未逐状态实操，不构成运行时验收。

**直接导入方：** [app/(workspace)/master-data/brands/page.tsx](../../erp-client/app/(workspace)/master-data/brands/page.tsx#L4)

## features/master-data/pages/category-object-page.tsx

**节点：** CategoryReviseDialog（行 52）；CategoryDisableDialog（行 57）

**建议：** 未列为优先修改项：保留当前结构；调用/装配代码跟随关联弹窗调整。未逐状态实操，不构成运行时验收。

**直接导入方：** [app/(workspace)/master-data/categories/[stableId]/page.tsx](../../erp-client/app/(workspace)/master-data/categories/[stableId]/page.tsx#L4)

## features/master-data/pages/category-tree-page.tsx

**节点：** CategoryCreateDialog（行 215）；CategoryReviseDialog（行 221）；CategoryDisableDialog（行 228）

**建议：** 未列为优先修改项：保留当前结构；调用/装配代码跟随关联弹窗调整。未逐状态实操，不构成运行时验收。

**直接导入方：** [app/(workspace)/master-data/categories/page.tsx](../../erp-client/app/(workspace)/master-data/categories/page.tsx#L3)

## features/master-data/pages/product-detail-page.tsx

**节点：** ProductSaveDialog（行 381）；DiscardConfirmDialog（行 400）

**建议：** 未列为优先修改项：保留当前结构；调用/装配代码跟随关联弹窗调整。未逐状态实操，不构成运行时验收。

**直接导入方：** [app/(workspace)/master-data/products/[stableId]/page.tsx](../../erp-client/app/(workspace)/master-data/products/[stableId]/page.tsx#L4)

## features/master-data/pages/products-list-page.tsx

**节点：** ProductSupplyDialog（行 233）；RegisterSupplyForSkuDialog（行 270）；ProductDisableDialog（行 279）

**建议：** 未列为优先修改项：保留当前结构；调用/装配代码跟随关联弹窗调整。未逐状态实操，不构成运行时验收。

**直接导入方：** [app/(workspace)/master-data/products/page.tsx](../../erp-client/app/(workspace)/master-data/products/page.tsx#L4)

## features/master-data/pages/suppliers-list-page.tsx

**节点：** SupplierDisableDialog（行 213）

**建议：** 未列为优先修改项：保留当前结构；调用/装配代码跟随关联弹窗调整。未逐状态实操，不构成运行时验收。

**直接导入方：** [app/(workspace)/master-data/suppliers/page.tsx](../../erp-client/app/(workspace)/master-data/suppliers/page.tsx#L4)

## features/master-data/pages/unit-of-measures-list-page.tsx

**节点：** UnitOfMeasureCreateDialog（行 165）；UnitOfMeasureReviseDialog（行 170）；UnitOfMeasureDisableDialog（行 178）

**建议：** 未列为优先修改项：保留当前结构；调用/装配代码跟随关联弹窗调整。未逐状态实操，不构成运行时验收。

**直接导入方：** [app/(workspace)/master-data/unit-of-measures/page.tsx](../../erp-client/app/(workspace)/master-data/unit-of-measures/page.tsx#L4)

## features/master-data/pages/voucher-categories-list-page.tsx

**节点：** VoucherCategoryStatusDialog（行 179）；VoucherCategoryFormDialog（行 183）；VoucherCategoryFormDialog（行 188）

**建议：** 未列为优先修改项：保留当前结构；调用/装配代码跟随关联弹窗调整。未逐状态实操，不构成运行时验收。

**直接导入方：** [app/(workspace)/master-data/voucher-categories/page.tsx](../../erp-client/app/(workspace)/master-data/voucher-categories/page.tsx#L4)

## features/master-data/pages/warehouse-object-page.tsx

**节点：** WarehouseReviseDialog（行 54）；WarehouseDisableDialog（行 59）

**建议：** 未列为优先修改项：保留当前结构；调用/装配代码跟随关联弹窗调整。未逐状态实操，不构成运行时验收。

**直接导入方：** [app/(workspace)/master-data/warehouses/[stableId]/page.tsx](../../erp-client/app/(workspace)/master-data/warehouses/[stableId]/page.tsx#L4)

## features/master-data/pages/warehouses-list-page.tsx

**节点：** WarehouseReviseDialog（行 166）；WarehouseDisableDialog（行 173）

**建议：** 未列为优先修改项：保留当前结构；调用/装配代码跟随关联弹窗调整。未逐状态实操，不构成运行时验收。

**直接导入方：** [app/(workspace)/master-data/warehouses/page.tsx](../../erp-client/app/(workspace)/master-data/warehouses/page.tsx#L4)

## features/procurement-responsibilities/components/procurement-responsibility-rules-page.tsx

**节点：** Dialog（行 190）；RuleDialog（行 641）

**建议：** D23

**直接导入方：** [app/(workspace)/master-data/procurement-responsibilities/page.tsx](../../erp-client/app/(workspace)/master-data/procurement-responsibilities/page.tsx#L3)

<details><summary>本文件中文字符串索引（包含非弹窗、隐藏说明与动态分支，不等同可见弹窗文案）</summary>

- L76：请选择采购负责人
- L84：请选择 SKU
- L95：请选择商品分类
- L105：请输入服务区域
- L115：请选择商品类型
- L172：采购责任规则已更新
- L172：采购责任规则已新增
- L173：后续销售责任预览将按最新启用规则解析。
- L197：编辑采购责任规则
- L197：新增采购责任规则
- L200：按从具体到通用的层级维护负责人。销售人员只能查看解析结果，不能在销售单上改负责人。
- L215：规则类型
- L237：公司 SKU
- L256：公司 SKU
- L257：搜索 SKU 或商品名称
- L269：商品分类
- L298：服务区域
- L300：例如：华东、上海市
- L310：商品类型
- L332：采购负责人
- L335：选择现有账号
- L345：启用规则
- L348：停用后不再参与销售行负责人解析。
- L372：取消
- L389：保存中…
- L389：保存规则
- L443：基础资料
- L443：采购责任规则
- L455：基础资料
- L455：采购责任规则
- L458：权限信息加载失败
- L461：暂时无法核对采购责任规则权限。
- L471：重试
- L482：基础资料
- L482：采购责任规则
- L485：权限不足
- L486：当前账号不能查看采购责任规则。
- L508：基础资料
- L509：采购责任规则
- L510：维护销售实物行到采购负责人的分配规则；越具体的规则优先命中。
- L521：正在加载负责人和分类选项
- L523：负责人或分类选项加载失败，请先重试
- L529：新增规则
- L536：规则编辑依赖加载失败
- L539：暂时无法读取采购负责人或商品分类，当前不能新增或编辑规则。
- L551：重试依赖数据
- L557：选项加载中
- L559：正在加载采购负责人和商品分类，完成后可编辑规则。
- L568：采购责任规则列表
- L587：规则加载失败
- L590：暂时无法读取采购责任规则。
- L601：重试
- L614：还没有采购责任规则
- L615：请先新增默认调度人，再逐步补充更具体的规则。
- L628：新增规则

</details>

## features/purchase-orders/components/purchase-change-order-approval-section.tsx

**节点：** PurchaseChangeOrderSubmitConfirmDialog（行 168）

**建议：** 未列为优先修改项：保留当前结构；调用/装配代码跟随关联弹窗调整。未逐状态实操，不构成运行时验收。

**直接导入方：** [features/purchase-orders/pages/purchase-order-detail-page.tsx](../../erp-client/features/purchase-orders/pages/purchase-order-detail-page.tsx#L17)、[features/purchase-orders/components/purchase-order-detail-changes-section.tsx](../../erp-client/features/purchase-orders/components/purchase-order-detail-changes-section.tsx#L11)

## features/purchase-orders/components/purchase-change-order-submit-confirm-dialog.tsx

**节点：** FormalActionConfirmDialog（行 30）

**建议：** D01

**直接导入方：** [features/purchase-orders/components/purchase-change-order-approval-section.tsx](../../erp-client/features/purchase-orders/components/purchase-change-order-approval-section.tsx#L9)

<details><summary>本文件中文字符串索引（包含非弹窗、隐藏说明与动态分支，不等同可见弹窗文案）</summary>

- L35：提交改单
- L36：确认提交
- L37：草稿
- L38：审批中
- L41：确认后启动审批。任一层驳回后将从第一节点开始下一轮。
- L48：原采购单当前版本
- L48：改单内容
- L48：已绑定的审批流程
- L50：内容锁定并进入审批
- L51：按已绑定的审批流程办理
- L52：全部节点通过后生成新的采购版本
- L54：形成提交并进入审批

</details>

## features/purchase-orders/components/purchase-order-cancel-approval-button.tsx

**节点：** AlertDialog（行 94）

**建议：** 未列为优先修改项：保留当前结构；调用/装配代码跟随关联弹窗调整。未逐状态实操，不构成运行时验收。

**直接导入方：** [features/purchase-orders/components/purchase-order-detail-header.tsx](../../erp-client/features/purchase-orders/components/purchase-order-detail-header.tsx#L21)

<details><summary>本文件中文字符串索引（包含非弹窗、隐藏说明与动态分支，不等同可见弹窗文案）</summary>

- L52：暂时无法核对权限，请刷新后重试。
- L56：正在核对权限，请稍候。
- L62：暂时无法核对权限，请刷新后重试。
- L71：当前账号没有撤回采购单审批权限
- L92：撤回审批
- L97：撤回审批
- L99：撤回后，采购单将回到草稿。
- L107：撤回原因
- L113：请输入撤回原因
- L131：取消
- L153：审批已撤回
- L155：已撤回当前审批，单据回到可编辑草稿。
- L162：撤回审批未完成，请刷新后重试。
- L181：撤回中
- L181：确认撤回

</details>

## features/purchase-orders/components/purchase-order-create-preview.tsx

**节点：** Dialog（行 88）

**建议：** D02

**直接导入方：** [features/purchase-orders/pages/purchase-order-create-page.tsx](../../erp-client/features/purchase-orders/pages/purchase-order-create-page.tsx#L39)

<details><summary>本文件中文字符串索引（包含非弹窗、隐藏说明与动态分支，不等同可见弹窗文案）</summary>

- L78：确认库存分配
- L94：预览供给分配
- L96：确认后由一个后端事务统一处理。
- L103：现有库存分配
- L135：将创建的采购单
- L157：本次全部由现有库存满足，不会创建采购单。
- L170：返回编辑
- L183：提交中…
- L207：采购单
- L208：提交预览
- L217：供应商
- L222：付款条件
- L227：采购类型
- L236：来源销售单
- L242：合同
- L243：无合同
- L248：负责销售
- L257：履约责任
- L266：采购入库目标仓
- L276：明细行数
- L282：预计交期
- L288：含税合计
- L292：采购明细
- L296：采购项目
- L310：数量
- L322：含税成本
- L329：进项税率
- L345：含税金额
- L351：含税金额
- L357：预计交期
- L368：不含税金额
- L373：税额
- L378：含税合计
- L383：本预览按当前选源结果拆单，确认后创建采购单并提交审批，金额以系统计算为准。

</details>

## features/purchase-orders/components/purchase-order-create-source-panel.tsx

**节点：** PurchaseOrderCreateSourcePickerDialog（行 200）

**建议：** 未列为优先修改项：保留当前结构；调用/装配代码跟随关联弹窗调整。未逐状态实操，不构成运行时验收。

**直接导入方：** [features/purchase-orders/pages/purchase-order-create-page.tsx](../../erp-client/features/purchase-orders/pages/purchase-order-create-page.tsx#L42)

## features/purchase-orders/components/purchase-order-create-source-picker-dialog.tsx

**节点：** Dialog（行 67）

**建议：** 未列为优先修改项：保留当前结构；调用/装配代码跟随关联弹窗调整。未逐状态实操，不构成运行时验收。

**直接导入方：** [features/purchase-orders/components/purchase-order-create-source-panel.tsx](../../erp-client/features/purchase-orders/components/purchase-order-create-source-panel.tsx#L29)

<details><summary>本文件中文字符串索引（包含非弹窗、隐藏说明与动态分支，不等同可见弹窗文案）</summary>

- L73：选择来源销售单
- L75：预览待分配供给的销售单，选定后作为本次供给分配来源。
- L82：待分配供给销售单
- L100：当前没有可预览的销售单。
- L113：取消
- L126：使用该销售单
- L152：销售单
- L153：供给分配来源
- L158：客户
- L160：无合同
- L164：合同
- L165：无合同
- L172：负责销售
- L177：待分配明细
- L183：可选采购供应商
- L193：已供给覆盖
- L199：采购类型
- L204：履约责任
- L209：付款条件
- L216：经营类目
- L222：待分配明细
- L226：销售项目
- L240：销售数量
- L252：已覆盖
- L264：剩余数量
- L276：可用供给
- L287：待分配明细
- L292：推荐采购含税估算
- L295：扣除推荐库存分配后，按采购推荐方案估算
- L298：本预览展示该销售单当前待分配供给的明细；库存优先，含税金额仅估算推荐采购缺口。

</details>

## features/purchase-orders/components/purchase-order-detail-dialogs.tsx

**节点：** PurchaseOrderSubmitConfirmDialog（行 64）；FormalActionConfirmDialog（行 73）；FormalActionConfirmDialog（行 95）；Dialog（行 119）

**建议：** 未列为优先修改项：保留当前结构；调用/装配代码跟随关联弹窗调整。未逐状态实操，不构成运行时验收。

**直接导入方：** [features/purchase-orders/pages/purchase-order-detail-page.tsx](../../erp-client/features/purchase-orders/pages/purchase-order-detail-page.tsx#L20)

<details><summary>本文件中文字符串索引（包含非弹窗、隐藏说明与动态分支，不等同可见弹窗文案）</summary>

- L77：作废采购草稿
- L78：作废
- L79：确认作废
- L80：草稿
- L81：已作废
- L87：释放本草稿占用的销售待分配供给数量
- L88：同步更新供给分配任务和可选依据
- L90：作废后的采购草稿不能恢复或再次提交
- L99：发起采购变更
- L100：创建变更
- L101：创建工作副本
- L106：变更工作副本
- L109：已发生入库/发货/付款/发票记录不回退
- L112：创建采购变更工作副本（同对象页签）
- L113：不得在原版本表单直接覆写
- L125：有未保存的修改
- L127：当前编辑内容尚未保存，离开后修改将丢失。建议先保存草稿。
- L140：继续编辑
- L156：保存中…
- L156：保存并离开
- L165：放弃修改并离开

</details>

## features/purchase-orders/components/purchase-order-paper-dialog.tsx

**节点：** Dialog（行 32）

**建议：** D24

**直接导入方：** [features/sales-orders/components/sales-order-detail-purchase-panel.tsx](../../erp-client/features/sales-orders/components/sales-order-detail-purchase-panel.tsx#L14)

<details><summary>本文件中文字符串索引（包含非弹窗、隐藏说明与动态分支，不等同可见弹窗文案）</summary>

- L37：采购单纸质预览
- L39：系统业务数据的打印件；金额与状态以系统记录为准。按 Esc                     或点击遮罩关闭。
- L56：关闭预览
- L102：正在读取采购单
- L103：正在读取采购单…
- L119：采购单读取失败
- L129：重试
- L140：未找到这张采购单，可能已删除或当前角色无权查看。

</details>

## features/purchase-orders/components/purchase-order-submit-confirm-dialog.tsx

**节点：** FormalActionConfirmDialog（行 30）

**建议：** D01

**直接导入方：** [features/purchase-orders/components/purchase-order-detail-dialogs.tsx](../../erp-client/features/purchase-orders/components/purchase-order-detail-dialogs.tsx#L17)

<details><summary>本文件中文字符串索引（包含非弹窗、隐藏说明与动态分支，不等同可见弹窗文案）</summary>

- L35：提交采购单
- L36：确认提交
- L37：草稿
- L38：审批中
- L41：确认后启动审批。任一层驳回后将从第一节点开始下一轮。
- L49：供应商 / 采购类型 / 履约责任 / 付款条件
- L50：商品行（二次确认分行）与物流费用
- L51：已绑定的审批流程
- L54：内容锁定并进入审批
- L55：按已绑定的审批流程办理
- L56：全部节点通过后形成采购生效版本
- L58：形成提交并进入审批

</details>

## features/purchase-orders/pages/purchase-order-create-page.tsx

**节点：** PurchaseOrderCreatePreviewDialog（行 814）；AlertDialog（行 828）

**建议：** D02

**直接导入方：** [app/(workspace)/procurement/orders/purchase-order-create-client.tsx](../../erp-client/app/(workspace)/procurement/orders/purchase-order-create-client.tsx#L3)、[features/workspace/components/workspace-procurement-task.tsx](../../erp-client/features/workspace/components/workspace-procurement-task.tsx#L13)

<details><summary>本文件中文字符串索引（包含非弹窗、隐藏说明与动态分支，不等同可见弹窗文案）</summary>

- L197：供给分配已完成
- L198：本次供给分配已保存
- L200：当前责任范围仍有未分配数量，任务继续保留在工作台，请完成剩余供给分配。
- L207：已创建 1 张采购单并提交审批。
- L226：供给分配失败
- L235：供给分配结果待确认
- L363：已批量指定履约方案
- L363：没有可应用的明细
- L367：选中行不支持该履约方案。
- L382：已重新分配供给
- L382：没有可匹配的供给方案
- L385：已优先分配现有库存，并为剩余缺口推荐采购方案。
- L386：当前明细没有可用库存或合格采购供给。
- L417：没有更多履约方案
- L418：该销售明细的可选履约方案都已添加。
- L474：请检查本次分配数量和供给方案后重试。
- L476：无法预览供给分配
- L480：无法预览供给分配
- L497：供给分配
- L498：正在加载库存与采购供给…
- L504：加载中
- L523：供给分配
- L524：供给依据加载失败
- L537：重新加载
- L552：返回列表
- L575：将创建采购单
- L580：将建立库存预留
- L585：采购缺口明细
- L590：采购含税合计
- L602：预览供给分配
- L613：供给分配
- L614：系统优先推荐现有库存，不足部分再推荐采购；确认后一次完成库存预留和采购缺口建单。
- L620：返回列表
- L658：当前没有待分配供给
- L661：该销售单可能尚未生效、供给已覆盖，或既无可用库存也无合格采购供给。
- L662：当前没有待分配供给。请检查已生效销售单、库存余额和供应商供给。
- L676：返回列表
- L730：销售明细与供给方案
- L733：行
- L762：页面已自动优先分配现有库存；库存不足时，再按可覆盖数量、成本和交期推荐采购。可调整或拆分，同一采购维度会合并为一张采购单。
- L843：确认供给分配
- L846：库存和采购会在同一次提交中生效。
- L854：返回预览
- L866：提交中…
- L866：确认提交

</details>

## features/sales-orders/components/acceptance-dialogs.tsx

**节点：** FormalActionConfirmDialog（行 47）；FormalActionConfirmDialog（行 71）

**建议：** D07

**直接导入方：** [features/sales-orders/components/acceptance-workspace.tsx](../../erp-client/features/sales-orders/components/acceptance-workspace.tsx#L40)

<details><summary>本文件中文字符串索引（包含非弹窗、隐藏说明与动态分支，不等同可见弹窗文案）</summary>

- L51：确认客户验收
- L52：确认本次验收
- L53：确认本次验收
- L54：待登记
- L78：冲正这条验收记录？
- L80：原记录会保留，并增加一条冲正记录；对应批次重新变为待验。
- L81：冲正
- L82：确认冲正
- L83：取消
- L84：已确认
- L85：已冲正
- L89：冲正理由
- L99：说明误录原因
- L117：本次验收批次

</details>

## features/sales-orders/components/acceptance-register-dialog.tsx

**节点：** Dialog（行 91）

**建议：** D07

**直接导入方：** [features/sales-orders/components/acceptance-workspace.tsx](../../erp-client/features/sales-orders/components/acceptance-workspace.tsx#L44)

<details><summary>本文件中文字符串索引（包含非弹窗、隐藏说明与动态分支，不等同可见弹窗文案）</summary>

- L85：全部通过并确认
- L85：确认本次验收
- L87：· 含短少、拒收或不通过
- L98：登记客户验收
- L100：商品选通过、短少或拒收；服务选通过或不通过。打开时默认全部通过，也可把某批改成这次不验。
- L106：当前没有待验收的交付记录。
- L122：客户验收时间
- L157：由
- L157：负责销售
- L157：登记
- L160：只有本单负责销售可以确认客户验收。
- L169：明细
- L173：销售
- L183：批待验
- L229：内部备注
- L230：可不填
- L242：已选
- L242：批
- L254：取消
- L261：提交中…
- L262：提交中…

</details>

## features/sales-orders/components/acceptance-workspace.tsx

**节点：** AcceptanceRegisterDialog（行 385）

**建议：** D07

**直接导入方：** [features/sales-orders/components/sales-order-detail-acceptance-panel.tsx](../../erp-client/features/sales-orders/components/sales-order-detail-acceptance-panel.tsx#L3)、[features/workspace/components/workspace-acceptance-task.tsx](../../erp-client/features/workspace/components/workspace-acceptance-task.tsx#L17)

## features/sales-orders/components/sales-change-order-approval-section.tsx

**节点：** SalesChangeOrderSubmitConfirmDialog（行 168）

**建议：** 未列为优先修改项：保留当前结构；调用/装配代码跟随关联弹窗调整。未逐状态实操，不构成运行时验收。

**直接导入方：** [features/sales-orders/pages/sales-order-detail-page.tsx](../../erp-client/features/sales-orders/pages/sales-order-detail-page.tsx#L17)、[features/sales-orders/components/sales-order-detail-finance-panels.tsx](../../erp-client/features/sales-orders/components/sales-order-detail-finance-panels.tsx#L3)

## features/sales-orders/components/sales-change-order-submit-confirm-dialog.tsx

**节点：** FormalActionConfirmDialog（行 26）

**建议：** D01

**直接导入方：** [features/sales-orders/components/sales-change-order-approval-section.tsx](../../erp-client/features/sales-orders/components/sales-change-order-approval-section.tsx#L9)

<details><summary>本文件中文字符串索引（包含非弹窗、隐藏说明与动态分支，不等同可见弹窗文案）</summary>

- L30：提交改单
- L31：确认提交
- L32：草稿
- L33：审批中
- L36：确认后启动审批。任一层驳回后将从第一节点开始下一轮。
- L43：原销售单当前版本
- L43：改单内容
- L43：已绑定的审批流程
- L45：内容锁定并进入审批
- L46：按已绑定的审批流程办理
- L47：全部节点通过后生成新的销售版本
- L49：形成提交并进入审批

</details>

## features/sales-orders/components/sales-change-review-panel.tsx

**节点：** FormalActionConfirmDialog（行 105）；FormalActionConfirmDialog（行 130）

**建议：** 未列为优先修改项：保留当前结构；调用/装配代码跟随关联弹窗调整。未逐状态实操，不构成运行时验收。

**直接导入方：** 未发现直接静态导入；不得据此断言未使用。

<details><summary>本文件中文字符串索引（包含非弹窗、隐藏说明与动态分支，不等同可见弹窗文案）</summary>

- L53：正在加载销售变更复核任务…
- L60：销售变更复核任务不可执行
- L62：任务、销售单或当前改单关系已变化，请返回任务队列刷新后重试。
- L73：销售变更履约影响复核
- L74：销售变更财务复核
- L77：当前改单
- L77：；任务版本
- L78：；提交版本
- L86：通过复核
- L102：驳回复核
- L109：通过销售变更复核
- L110：待复核
- L114：待财务复核
- L115：变更已生效
- L123：复核意见（可选）
- L126：写入正式复核结论
- L126：完成当前任务
- L134：驳回销售变更复核
- L135：待复核
- L136：退回改单
- L142：驳回原因（必填）
- L145：写入正式驳回结论
- L145：完成当前任务

</details>

## features/sales-orders/components/sales-order-cancel-approval-button.tsx

**节点：** AlertDialog（行 93）

**建议：** 未列为优先修改项：保留当前结构；调用/装配代码跟随关联弹窗调整。未逐状态实操，不构成运行时验收。

**直接导入方：** [features/sales-orders/components/sales-order-detail-command-dialogs.tsx](../../erp-client/features/sales-orders/components/sales-order-detail-command-dialogs.tsx#L19)

<details><summary>本文件中文字符串索引（包含非弹窗、隐藏说明与动态分支，不等同可见弹窗文案）</summary>

- L55：暂时无法核对权限，请刷新后重试。
- L59：正在核对权限，请稍候。
- L65：暂时无法核对权限，请刷新后重试。
- L91：撤回审批
- L96：撤回审批
- L98：撤回后，销售单将回到草稿。
- L106：撤回原因
- L112：请输入撤回原因
- L130：取消
- L151：审批已撤回
- L153：已撤回当前审批，单据回到可编辑草稿。
- L160：撤回审批未完成，请刷新后重试。
- L179：撤回中
- L179：确认撤回

</details>

## features/sales-orders/components/sales-order-create-form.tsx

**节点：** ContractUploadDialog（行 370）；DiscardConfirmDialog（行 380）；VoucherSalesOrderSubmitConfirmDialog（行 397）；SalesOrderSubmitConfirmDialog（行 409）

**建议：** 未列为优先修改项：保留当前结构；调用/装配代码跟随关联弹窗调整。未逐状态实操，不构成运行时验收。

**直接导入方：** [features/sales-orders/pages/sales-order-create-page.tsx](../../erp-client/features/sales-orders/pages/sales-order-create-page.tsx#L11)、[features/sales-orders/components/sales-order-detail-editable-center.tsx](../../erp-client/features/sales-orders/components/sales-order-detail-editable-center.tsx#L15)

## features/sales-orders/components/sales-order-create-line-items-section.tsx

**节点：** SellableSkuSelectDialog（行 108）

**建议：** 未列为优先修改项：保留当前结构；调用/装配代码跟随关联弹窗调整。未逐状态实操，不构成运行时验收。

**直接导入方：** [features/sales-orders/components/sales-order-create-form.tsx](../../erp-client/features/sales-orders/components/sales-order-create-form.tsx#L43)

## features/sales-orders/components/sales-order-detail-command-dialogs.tsx

**节点：** AlertDialog（行 109）

**建议：** 未列为优先修改项：保留当前结构；调用/装配代码跟随关联弹窗调整。未逐状态实操，不构成运行时验收。

**直接导入方：** [features/sales-orders/pages/sales-order-detail-page.tsx](../../erp-client/features/sales-orders/pages/sales-order-detail-page.tsx#L18)

<details><summary>本文件中文字符串索引（包含非弹窗、隐藏说明与动态分支，不等同可见弹窗文案）</summary>

- L45：当前不能改单
- L64：发起改单
- L86：尚无生效版本
- L102：改单未创建，请稍后重试。
- L112：发起改单
- L114：创建改单草稿，不改现行版本。交付、回款、开票都保留。
- L126：变更为
- L129：改单草稿
- L142：取消
- L157：创建中
- L157：确认创建

</details>

## features/sales-orders/components/sales-order-detail-purchase-panel.tsx

**节点：** PurchaseOrderPaperDialog（行 222）

**建议：** 未列为优先修改项：保留当前结构；调用/装配代码跟随关联弹窗调整。未逐状态实操，不构成运行时验收。

**直接导入方：** [features/sales-orders/components/sales-order-detail-panels.tsx](../../erp-client/features/sales-orders/components/sales-order-detail-panels.tsx#L13)

## features/sales-orders/components/sales-order-paper-dialog.tsx

**节点：** Dialog（行 36）

**建议：** D24

**直接导入方：** [features/sales-orders/components/sales-order-paper-preview-dialog.tsx](../../erp-client/features/sales-orders/components/sales-order-paper-preview-dialog.tsx#L13)、[features/workspace/components/workspace-document-paper-dialog.tsx](../../erp-client/features/workspace/components/workspace-document-paper-dialog.tsx#L16)

<details><summary>本文件中文字符串索引（包含非弹窗、隐藏说明与动态分支，不等同可见弹窗文案）</summary>

- L44：销售单纸质预览
- L47：系统业务数据的打印件；金额与状态以系统记录为准。按 Esc                     或点击遮罩关闭。
- L64：关闭预览
- L89：销售单
- L95：尚未生效
- L101：销售方
- L103：结算主体
- L107：业务负责人
- L112：提交时间
- L120：客户
- L126：结算主体
- L131：联系人
- L140：付款条件
- L145：卡券履约期限
- L145：客户承诺期限摘要
- L151：福利场景
- L156：来源
- L160：卡券明细（唯一）
- L160：销售明细
- L166：卡券类目
- L180：面额
- L192：数量
- L204：形态
- L209：配赠率
- L219：成交金额（含税）
- L230：项目
- L244：承诺交付日
- L250：数量
- L262：单价（含税）
- L271：小计（含税）
- L285：不含税金额
- L290：税额
- L295：成交金额（含税）
- L301：已回款（含税）
- L307：已开票
- L315：卡券履约在福利商城执行；本单据仅展示系统内的销售数据。
- L321：业务负责人
- L327：日期
- L336：公司签章
- L338：签章位

</details>

## features/sales-orders/components/sales-order-paper-preview-dialog.tsx

**节点：** Dialog（行 34）

**建议：** D24

**直接导入方：** [app/(workspace)/procurement/orders/purchase-order-create-client.tsx](../../erp-client/app/(workspace)/procurement/orders/purchase-order-create-client.tsx#L4)、[features/workspace/components/workspace-procurement-task.tsx](../../erp-client/features/workspace/components/workspace-procurement-task.tsx#L14)

<details><summary>本文件中文字符串索引（包含非弹窗、隐藏说明与动态分支，不等同可见弹窗文案）</summary>

- L40：销售单纸质预览
- L43：系统业务数据的打印件；金额与状态以系统记录为准。按 Esc                     或点击遮罩关闭。版本、附件和关联单据仍在对应工作面查看。
- L59：关闭预览
- L86：正在读取销售单
- L87：正在读取销售单…
- L97：销售单读取失败
- L107：重试
- L118：未找到这张销售单，可能已删除或当前角色无权查看。

</details>

## features/sales-orders/components/sales-order-submit-confirm-dialog.tsx

**节点：** Dialog（行 38）

**建议：** 未列为优先修改项：保留当前结构；调用/装配代码跟随关联弹窗调整。未逐状态实操，不构成运行时验收。

**直接导入方：** [features/sales-orders/components/voucher-sales-order-submit-confirm-dialog.tsx](../../erp-client/features/sales-orders/components/voucher-sales-order-submit-confirm-dialog.tsx#L3)、[features/sales-orders/components/sales-order-create-form.tsx](../../erp-client/features/sales-orders/components/sales-order-create-form.tsx#L47)

<details><summary>本文件中文字符串索引（包含非弹窗、隐藏说明与动态分支，不等同可见弹窗文案）</summary>

- L26：提交后进入审批；任一层驳回后将从第一节点开始下一轮。
- L46：提交销售单
- L50：草稿
- L52：审批中
- L71：返回修改
- L81：提交中…
- L81：确认提交

</details>

## features/sales-orders/components/sellable-sku-select-dialog.tsx

**节点：** Dialog（行 254）

**建议：** 未列为优先修改项：保留当前结构；调用/装配代码跟随关联弹窗调整。未逐状态实操，不构成运行时验收。

**直接导入方：** [features/sales-orders/components/sales-order-create-line-items-section.tsx](../../erp-client/features/sales-orders/components/sales-order-create-line-items-section.tsx#L7)

<details><summary>本文件中文字符串索引（包含非弹窗、隐藏说明与动态分支，不等同可见弹窗文案）</summary>

- L68：选择商品
- L69：用分类、品牌、供应商、区域和售价从公司商品池筛选，不必只靠关键字。
- L184：不限
- L185：不限
- L251：使用该商品
- L325：可组合多个条件，点击「查询」后统一生效。
- L377：当前筛选无结果
- L378：还没有可销售的 SKU
- L382：没有记录符合当前筛选条件，可清除筛选后重试。
- L383：商品需要已上架、资料有效且存在有效供给，才会出现在这里。
- L396：清除筛选
- L409：已选
- L409：个
- L430：取消

</details>

## features/sales-orders/components/voucher-sales-order-submit-confirm-dialog.tsx

**节点：** SalesOrderSubmitConfirmDialog（行 23）

**建议：** 未列为优先修改项：保留当前结构；调用/装配代码跟随关联弹窗调整。未逐状态实操，不构成运行时验收。

**直接导入方：** [features/sales-orders/components/sales-order-create-form.tsx](../../erp-client/features/sales-orders/components/sales-order-create-form.tsx#L49)

## features/supplier-api-connections/components/connection-center.tsx

**节点：** DisableConnectionDialog（行 277）；ReferenceBindDialog（行 295）；ReferenceBindDialog（行 319）；RunHealthCheckDialog（行 343）；EnableConnectionDialog（行 361）；CapConfigDialog（行 378）

**建议：** 未列为优先修改项：保留当前结构；调用/装配代码跟随关联弹窗调整。未逐状态实操，不构成运行时验收。

**直接导入方：** [features/supplier-api-connections/pages/supplier-api-connections-page.tsx](../../erp-client/features/supplier-api-connections/pages/supplier-api-connections-page.tsx#L6)

## features/supplier-api-connections/components/connection-create-dialog.tsx

**节点：** Dialog（行 98）

**建议：** D09

**直接导入方：** [features/supplier-api-connections/components/connection-list.tsx](../../erp-client/features/supplier-api-connections/components/connection-list.tsx#L20)

<details><summary>本文件中文字符串索引（包含非弹窗、隐藏说明与动态分支，不等同可见弹窗文案）</summary>

- L28：请填写连接代码
- L29：请选择供应商
- L30：请选择供应商
- L85：打开连接详情
- L101：新建连接身份
- L103：连接代码全局唯一，不可与环境组合复用。创建成功后可在结果中打开连接详情完成配置。
- L118：连接代码
- L136：供应商
- L153：搜索供应商名称或编码
- L167：环境
- L182：生产
- L184：测试
- L187：开发
- L197：正在创建生产环境连接身份
- L211：取消
- L216：创建

</details>

## features/supplier-api-connections/components/connection-list.tsx

**节点：** ConnectionCreateDialog（行 246）

**建议：** 未列为优先修改项：保留当前结构；调用/装配代码跟随关联弹窗调整。未逐状态实操，不构成运行时验收。

**直接导入方：** [features/supplier-api-connections/pages/supplier-api-connections-page.tsx](../../erp-client/features/supplier-api-connections/pages/supplier-api-connections-page.tsx#L7)

## features/supplier-api-connections/components/dialogs/cap-config-dialog.tsx

**节点：** Dialog（行 107）

**建议：** D09

**直接导入方：** [features/supplier-api-connections/components/connection-center-sections.tsx](../../erp-client/features/supplier-api-connections/components/connection-center-sections.tsx#L13)

<details><summary>本文件中文字符串索引（包含非弹窗、隐藏说明与动态分支，不等同可见弹窗文案）</summary>

- L110：配置连接能力
- L112：由系统管理员统一配置，配置后能力需重新验证；不复用采购确认写入口。
- L145：停用
- L146：启用
- L162：取消
- L178：提交中…
- L179：提交能力配置

</details>

## features/supplier-api-connections/components/dialogs/disable-connection-dialog.tsx

**节点：** Dialog（行 36）

**建议：** D10

**直接导入方：** [features/supplier-api-connections/components/connection-center.tsx](../../erp-client/features/supplier-api-connections/components/connection-center.tsx#L28)

<details><summary>本文件中文字符串索引（包含非弹窗、隐藏说明与动态分支，不等同可见弹窗文案）</summary>

- L43：停用生产环境连接
- L43：停用连接
- L46：停用改变治理状态，不删除连接、版本和历史业务记录。
- L50：停用影响预览
- L51：请核对发布、待处理订单与同步任务影响。
- L59：受影响发布/订单/任务
- L61：连接
- L64：密钥配置
- L64：签名材料
- L70：生效发布
- L78：待处理订单
- L86：同步任务
- L94：历史版本与业务记录保留，不会删除任何数据。
- L96：替代方案：
- L102：供应商供给
- L109：供应商订单
- L116：接口错误中心
- L128：取消
- L143：停用中…
- L143：确认停用

</details>

## features/supplier-api-connections/components/dialogs/enable-connection-dialog.tsx

**节点：** Dialog（行 31）

**建议：** D10

**直接导入方：** [features/supplier-api-connections/components/connection-center.tsx](../../erp-client/features/supplier-api-connections/components/connection-center.tsx#L29)

<details><summary>本文件中文字符串索引（包含非弹窗、隐藏说明与动态分支，不等同可见弹窗文案）</summary>

- L38：启用生产环境连接
- L38：启用连接
- L41：启用后连接将恢复对外接口可用，后续下单、查询等业务请求将按能力声明放行。
- L42：生产环境操作需谨慎核对。
- L53：取消
- L67：启用中…
- L67：确认启用

</details>

## features/supplier-api-connections/components/dialogs/reference-bind-dialog.tsx

**节点：** Dialog（行 62）

**建议：** D09

**直接导入方：** [features/supplier-api-connections/components/connection-center.tsx](../../erp-client/features/supplier-api-connections/components/connection-center.tsx#L30)

<details><summary>本文件中文字符串索引（包含非弹窗、隐藏说明与动态分支，不等同可见弹窗文案）</summary>

- L48：密钥引用
- L48：地址引用
- L59：无法取得密钥管理引用列表，请重试后再选择。
- L60：无法取得地址配置引用列表，请重试后再选择。
- L74：只能从密钥管理系统选择不透明引用。无明文密钥输入框；页面、URL 与结果均不返回正文。
- L75：只能从系统提供的地址配置引用中选择，不能自由输入地址。
- L81：引用选项加载失败
- L89：密钥管理引用
- L90：地址配置引用
- L101：选择不透明引用
- L102：选择地址配置引用
- L107：当前状态：
- L120：取消
- L140：绑定中…
- L142：确认绑定引用
- L143：确认绑定地址

</details>

## features/supplier-api-connections/components/dialogs/run-health-check-dialog.tsx

**节点：** Dialog（行 33）

**建议：** D10

**直接导入方：** [features/supplier-api-connections/components/connection-center.tsx](../../erp-client/features/supplier-api-connections/components/connection-center.tsx#L31)

<details><summary>本文件中文字符串索引（包含非弹窗、隐藏说明与动态分支，不等同可见弹窗文案）</summary>

- L39：执行健康检查
- L41：将对全能力执行健康检查并记录结果。
- L43：生产环境检查不会创建真实业务订单。
- L44：结果可随时在本页健康记录中查看。
- L55：取消
- L74：执行中…
- L74：确认执行

</details>

## features/supplier-offerings/components/dialogs/change-offering-status-dialog.tsx

**节点：** Dialog（行 73）

**建议：** D15

**直接导入方：** [features/supplier-offerings/pages/supplier-offerings-page.tsx](../../erp-client/features/supplier-offerings/pages/supplier-offerings-page.tsx#L17)

<details><summary>本文件中文字符串索引（包含非弹窗、隐藏说明与动态分支，不等同可见弹窗文案）</summary>

- L59：当前条款不完整或看不到价格，无法保存新版本。请改用「修订条款」。
- L67：供给条款保存失败
- L81：将追加一版商业条款。价格、起订量、区域和有效期保持当前内容，条款号从                         v
- L82：变为 v
- L82：，关系状态变为
- L88：保存失败
- L103：变更原因
- L119：取消

</details>

## features/supplier-offerings/components/dialogs/register-supply-for-sku-dialog.tsx

**节点：** Dialog（行 118）

**建议：** D15

**直接导入方：** [features/supplier-offerings/pages/supplier-offerings-page.tsx](../../erp-client/features/supplier-offerings/pages/supplier-offerings-page.tsx#L18)、[features/supplier-offerings/offering-dialogs.ts](../../erp-client/features/supplier-offerings/offering-dialogs.ts#L3)

<details><summary>本文件中文字符串索引（包含非弹窗、隐藏说明与动态分支，不等同可见弹窗文案）</summary>

- L68：新增供应商供给
- L104：供给已添加
- L106：公司 SKU 与供应商之间的供给关系、首版条款和初始可供状态已同时生效。
- L112：供给登记失败，请稍后重试
- L124：添加供给
- L126：供给直接连接公司 SKU                         与供应商；供应商订货编码、商业条款和当前可供情况在此维护。
- L143：保存失败
- L159：基础信息
- L167：公司 SKU
- L185：选择公司 SKU
- L196：供应商
- L212：选择已启用供应商
- L222：供应商 SKU 编码
- L224：用于下单、对账和履约快照
- L232：供应商商品编码
- L242：价格与条款
- L249：一件代发供给价（含税）
- L258：集采供给价（含税）
- L267：集采起订量
- L276：进项税率（%）
- L278：例如 13 表示 13%
- L287：供应范围与时效
- L295：可供区域
- L297：多个区域使用逗号分隔
- L306：生效日期
- L315：失效日期
- L326：物流与费用
- L333：一件代发快递说明
- L341：运费
- L349：服务费
- L358：可供状态与登记说明
- L365：初始可供状态
- L403：当前可供数量
- L404：留空表示供应商未提供数量上限
- L413：登记原因
- L436：关闭
- L441：保存供给

</details>

## features/supplier-offerings/components/dialogs/revise-offering-dialog.tsx

**节点：** Dialog（行 90）

**建议：** D15

**直接导入方：** [features/supplier-offerings/pages/supplier-offerings-page.tsx](../../erp-client/features/supplier-offerings/pages/supplier-offerings-page.tsx#L19)、[features/supplier-offerings/offering-dialogs.ts](../../erp-client/features/supplier-offerings/offering-dialogs.ts#L4)

<details><summary>本文件中文字符串索引（包含非弹窗、隐藏说明与动态分支，不等同可见弹窗文案）</summary>

- L52：调整供给条款
- L84：供给条款保存失败
- L96：修订供给条款
- L106：保存失败
- L121：价格与条款
- L128：一件代发供给价（含税）
- L137：集采供给价（含税）
- L146：集采起订量
- L155：进项税率（%）
- L157：例如 13 表示 13%
- L166：供应范围与时效
- L174：可供区域
- L176：多个区域使用逗号分隔
- L185：生效日期
- L194：失效日期
- L205：物流与费用
- L212：一件代发快递说明
- L220：运费
- L228：服务费
- L237：状态与说明
- L244：供给关系状态
- L260：变更原因
- L280：取消
- L285：保存新版本

</details>

## features/supplier-offerings/components/dialogs/update-availability-dialog.tsx

**节点：** Dialog（行 69）

**建议：** D15

**直接导入方：** [features/supplier-offerings/pages/supplier-offerings-page.tsx](../../erp-client/features/supplier-offerings/pages/supplier-offerings-page.tsx#L20)、[features/supplier-offerings/offering-dialogs.ts](../../erp-client/features/supplier-offerings/offering-dialogs.ts#L5)

<details><summary>本文件中文字符串索引（包含非弹窗、隐藏说明与动态分支，不等同可见弹窗文案）</summary>

- L44：更新当前可供情况
- L63：可供情况保存失败
- L88：更新当前可供情况
- L90：该信息独立于商业条款版本，可由人工或供应商接口高频更新。
- L95：保存失败
- L110：可供状态
- L138：当前可供数量
- L139：留空表示供应商未提供数量上限
- L147：变更原因
- L163：取消
- L168：保存可供情况

</details>

## features/supplier-offerings/components/supply-exception-task-panel.tsx

**节点：** FormalActionConfirmDialog（行 394）

**建议：** D22

**直接导入方：** [features/supplier-offerings/pages/supplier-offerings-page.tsx](../../erp-client/features/supplier-offerings/pages/supplier-offerings-page.tsx#L24)、[features/workspace/components/workspace-supply-exception-task.tsx](../../erp-client/features/workspace/components/workspace-supply-exception-task.tsx#L4)

<details><summary>本文件中文字符串索引（包含非弹窗、隐藏说明与动态分支，不等同可见弹窗文案）</summary>

- L38：可查看
- L39：可核对
- L40：可转交
- L45：责任人信息不可用
- L66：请填写处置证据引用
- L67：证据引用不能超过 256 个字符
- L71：请填写至少 3 个字的核对结论
- L72：核对结论不能超过 500 个字符
- L141：核对结论未提交，请刷新任务版本后重试
- L151：正在核对供应停止任务
- L159：供应停止任务已阻止
- L160：当前责任、任务版本或供给对象未通过校验。本页不会提供供给写入动作。
- L171：重试校验
- L180：返回待办队列
- L200：供应停止核对
- L201：待核对
- L204：核对停供来源和已固定的暂停影响，登记处置证据后完成当前责任；不选定替代供给，不发起恢复发布。
- L215：返回待办队列
- L223：安全暂停不会随任务完成而解除
- L225：本动作只确认人工核对已经完成。供应商供给、暂停修订和商品发布暂停状态全部保持不变。
- L232：来源供给
- L240：当前责任
- L252：任务版本
- L260：来源版本
- L271：已固定影响
- L275：原因：
- L276：。影响只使用任务记录，不在页面重新计算。
- L280：当前列表行只用于识别供给对象，不覆盖任务冻结的来源版本。
- L281：当前分页未加载完整供给行；不在页面推断来源记录。
- L290：核对边界
- L293：核对停供来源和来源版本。
- L294：确认安全暂停影响已由系统固定。
- L295：登记可审计的处置证据与核对结论。
- L302：当前允许动作
- L312：当前只读
- L328：转交只改变当前责任人；“确认已核对”完成任务，但不恢复发布。
- L334：核对结论未提交
- L347：完成核对
- L349：证据引用和核对结论进入审计；提交后当前任务完成，安全暂停继续生效。
- L357：处置证据引用
- L358：例如：供应商停供函、替代采购事项或内部工单编号
- L368：核对结论
- L370：说明已核对的停供来源、受影响发布及后续安排
- L378：确认已核对
- L379：正在提交
- L387：当前账号不是责任人，或任务已被阻断；请刷新或由有权人员处理。
- L398：确认已核对并完成任务
- L399：确认完成供应停止核对
- L400：本动作只完成人工核对责任，不恢复供应商供给，也不恢复任何商品发布。
- L401：待核对
- L402：任务已完成
- L404：记录处置证据引用与核对结论
- L405：完成当前 W21 工作项
- L406：安全暂停与暂停修订继续生效
- L408：核对结论进入审计记录

</details>

## features/supplier-offerings/pages/supplier-offerings-page.tsx

**节点：** RegisterSupplyForSkuDialog（行 284）；ReviseOfferingDialog（行 307）；UpdateAvailabilityDialog（行 316）；ChangeOfferingStatusDialog（行 325）

**建议：** 未列为优先修改项：保留当前结构；调用/装配代码跟随关联弹窗调整。未逐状态实操，不构成运行时验收。

**直接导入方：** [app/(workspace)/procurement/supplier-offerings/page.tsx](../../erp-client/app/(workspace)/procurement/supplier-offerings/page.tsx#L4)

## features/supplier-orders/components/supplier-order-preview-center-dialogs.tsx

**节点：** FormalActionConfirmDialog（行 47）；FormalActionConfirmDialog（行 70）；FormalActionConfirmDialog（行 92）

**建议：** D08、D22

**直接导入方：** [features/supplier-orders/pages/supplier-order-center-page.tsx](../../erp-client/features/supplier-orders/pages/supplier-order-center-page.tsx#L31)

<details><summary>本文件中文字符串索引（包含非弹窗、隐藏说明与动态分支，不等同可见弹窗文案）</summary>

- L51：安全重发
- L52：确认沿用原任务号重新提交
- L53：仅在确认无结果且系统判定可安全重试时允许。重发不会新建业务订单。
- L58：重发后待确认
- L62：沿用原下单任务号
- L63：任务保持待处理，不会自动完成
- L65：将再次向供应商发起下单
- L74：完成正式任务
- L75：确认处理结果并完成任务
- L76：提交时将重新核对供应商动作结果、订单数据和当前处理权；任一不一致都保持原任务待处理。
- L81：任务已完成
- L87：保存已核实的业务结果
- L87：一并完成当前任务
- L104：提交取消
- L105：提交退款
- L109：确认向供应商提交取消
- L110：确认向供应商提交退款
- L116：取消
- L117：退款
- L122：当前状态
- L128：取消处理中
- L129：退款处理中
- L134：重复提交返回原结果，不会重复发起
- L138：取消
- L138：退款

</details>

## features/supplier-payables/components/allocation-workspace.tsx

**节点：** SupplierPaymentSubmitConfirmDialog（行 198）；FormalActionConfirmDialog（行 208）

**建议：** D20

**直接导入方：** [features/supplier-payables/pages/supplier-allocation-session-page.tsx](../../erp-client/features/supplier-payables/pages/supplier-allocation-session-page.tsx#L14)、[features/workspace/components/workspace-payment-task.tsx](../../erp-client/features/workspace/components/workspace-payment-task.tsx#L23)

<details><summary>本文件中文字符串索引（包含非弹窗、隐藏说明与动态分支，不等同可见弹窗文案）</summary>

- L98：应付优先级策略不可用
- L101：混合自动分配已禁用；请显式勾选目标并填写金额。
- L108：来源上下文
- L116：。完成后请返回来源页，将重新校验先款条件；未核销付款不满足先款要求。
- L127：继续处理
- L127：回到列表
- L212：登记进项发票并核销
- L213：确认登记进项发票并核销
- L214：提交后形成不可编辑记录；纠错须追加红票。提交时系统将校验供应商、余额与混合来源规则。
- L215：确认提交
- L216：本次草稿
- L217：已确认
- L224：形成进项发票与有效分配
- L225：同步更新应付开放余额
- L226：未分配余额保留在待核销视图
- L227：来源页须重新校验先款条件，未核销付款不满足
- L230：已确认记录不可编辑删除，纠错追加反向记录

</details>

## features/supplier-payables/components/payment-reversal-request-dialog.tsx

**节点：** Dialog（行 58）

**建议：** D03

**直接导入方：** [features/supplier-payables/pages/supplier-accounts-page.tsx](../../erp-client/features/supplier-payables/pages/supplier-accounts-page.tsx#L5)

<details><summary>本文件中文字符串索引（包含非弹窗、隐藏说明与动态分支，不等同可见弹窗文案）</summary>

- L20：请填写原因说明
- L61：发起付款冲正
- L63：不编辑、不删除已确认记录与分配；仅追加冲正记录。原单
- L64：。冲正表示撤销本次付款记录。
- L75：将按原单全额追加冲正
- L82：，原记录保留。
- L89：原因说明
- L91：业务依据与说明
- L110：取消
- L115：下一步

</details>

## features/supplier-payables/components/payment-reversal-submit-confirm-dialog.tsx

**节点：** FormalActionConfirmDialog（行 30）

**建议：** D01、D03

**直接导入方：** [features/supplier-payables/pages/supplier-accounts-page.tsx](../../erp-client/features/supplier-payables/pages/supplier-accounts-page.tsx#L6)

<details><summary>本文件中文字符串索引（包含非弹窗、隐藏说明与动态分支，不等同可见弹窗文案）</summary>

- L35：提交冲正
- L36：确认提交
- L37：草稿
- L38：审批中
- L41：确认后启动审批。任一层驳回后将从第一节点开始下一轮。
- L48：往来主体
- L48：冲正金额
- L48：已绑定的审批流程
- L50：内容锁定并进入审批
- L51：按已绑定的审批流程办理
- L52：全部节点通过后过账并冲减原付款
- L54：形成提交并进入审批

</details>

## features/supplier-payables/components/supplier-payment-detail-dialog.tsx

**节点：** Dialog（行 42）

**建议：** D20

**直接导入方：** [features/supplier-payables/pages/components/supplier-accounts-preview.tsx](../../erp-client/features/supplier-payables/pages/components/supplier-accounts-preview.tsx#L13)

<details><summary>本文件中文字符串索引（包含非弹窗、隐藏说明与动态分支，不等同可见弹窗文案）</summary>

- L49：付款详情
- L59：查看付款记录、收款信息、银行回单与核销明细。
- L76：付款详情加载失败，请重试。
- L86：重试
- L91：未找到付款详情
- L102：取消

</details>

## features/supplier-payables/components/supplier-payment-submit-confirm-dialog.tsx

**节点：** FormalActionConfirmDialog（行 33）

**建议：** D08

**直接导入方：** [features/supplier-payables/components/allocation-workspace.tsx](../../erp-client/features/supplier-payables/components/allocation-workspace.tsx#L14)

<details><summary>本文件中文字符串索引（包含非弹窗、隐藏说明与动态分支，不等同可见弹窗文案）</summary>

- L29：未填写
- L30：未加载
- L38：付款
- L39：确认付款
- L40：确认付款
- L41：待付款
- L42：已过账
- L43：确认后立即过账并核销。
- L45：未加载
- L47：未加载
- L50：纠错须走付款冲正或供应商退款

</details>

## features/supplier-payables/components/supplier-refund-request-dialog.tsx

**节点：** Dialog（行 58）

**建议：** D03

**直接导入方：** [features/supplier-payables/pages/supplier-accounts-page.tsx](../../erp-client/features/supplier-payables/pages/supplier-accounts-page.tsx#L7)

<details><summary>本文件中文字符串索引（包含非弹窗、隐藏说明与动态分支，不等同可见弹窗文案）</summary>

- L20：请填写原因说明
- L61：发起供应商退款
- L63：不编辑、不删除已确认记录与分配；仅追加退款记录。原单
- L64：。退款表示供应商退回资金。
- L75：将按原单全额追加退款
- L82：，原记录保留。
- L89：原因说明
- L91：业务依据与说明
- L110：取消
- L115：下一步

</details>

## features/supplier-payables/components/supplier-refund-submit-confirm-dialog.tsx

**节点：** FormalActionConfirmDialog（行 30）

**建议：** D01、D03

**直接导入方：** [features/supplier-payables/pages/supplier-accounts-page.tsx](../../erp-client/features/supplier-payables/pages/supplier-accounts-page.tsx#L8)

<details><summary>本文件中文字符串索引（包含非弹窗、隐藏说明与动态分支，不等同可见弹窗文案）</summary>

- L35：提交退款
- L36：确认提交
- L37：草稿
- L38：审批中
- L41：确认后启动审批。任一层驳回后将从第一节点开始下一轮。
- L48：供应商
- L48：退款金额
- L48：已绑定的审批流程
- L50：内容锁定并进入审批
- L51：按已绑定的审批流程办理
- L52：全部节点通过后过账并入账
- L54：形成提交并进入审批

</details>

## features/supplier-payables/pages/components/pick-supplier-dialog.tsx

**节点：** Dialog（行 32）

**建议：** D20

**直接导入方：** [features/supplier-payables/pages/supplier-accounts-page.tsx](../../erp-client/features/supplier-payables/pages/supplier-accounts-page.tsx#L29)

<details><summary>本文件中文字符串索引（包含非弹窗、隐藏说明与动态分支，不等同可见弹窗文案）</summary>

- L42：选择供应商 · 登记付款
- L43：选择供应商 · 登记进项发票
- L46：本次核销创建后锁定供应商；不同供应商目标不会进入同一核销池。
- L50：供应商
- L56：供应商
- L57：选择供应商
- L67：取消
- L78：进入本次核销

</details>

## features/supplier-payables/pages/components/reverse-dialog.tsx

**节点：** Dialog（行 40）

**建议：** D20

**直接导入方：** [features/supplier-payables/pages/supplier-accounts-page.tsx](../../erp-client/features/supplier-payables/pages/supplier-accounts-page.tsx#L30)

<details><summary>本文件中文字符串索引（包含非弹窗、隐藏说明与动态分支，不等同可见弹窗文案）</summary>

- L49：付款冲正
- L49：进项红票
- L52：原单
- L52：将保留；请填写业务原因。
- L58：原因
- L64：至少 2 个字
- L70：红票号码
- L86：红票号码必填；红票将作为独立记录登记。
- L100：取消
- L119：提交中…
- L119：确认追加反向记录

</details>

## features/supplier-payables/pages/components/supplier-accounts-preview.tsx

**节点：** SupplierPaymentDetailDialog（行 219）

**建议：** 未列为优先修改项：保留当前结构；调用/装配代码跟随关联弹窗调整。未逐状态实操，不构成运行时验收。

**直接导入方：** [features/supplier-payables/pages/supplier-accounts-page.tsx](../../erp-client/features/supplier-payables/pages/supplier-accounts-page.tsx#L26)

## features/supplier-payables/pages/supplier-accounts-page.tsx

**节点：** PickSupplierDialog（行 398）；SupplierRefundRequestDialog（行 416）；SupplierRefundSubmitConfirmDialog（行 429）；PaymentReversalRequestDialog（行 440）；PaymentReversalSubmitConfirmDialog（行 476）；ReverseDialog（行 488）

**建议：** D03

**直接导入方：** [app/(workspace)/finance/supplier-accounts/page.tsx](../../erp-client/app/(workspace)/finance/supplier-accounts/page.tsx#L5)

## features/supplier-settlements/components/create-draft-dialog.tsx

**节点：** Dialog（行 63）

**建议：** 未列为优先修改项：保留当前结构；调用/装配代码跟随关联弹窗调整。未逐状态实操，不构成运行时验收。

**直接导入方：** [features/supplier-settlements/components/settlement-list.tsx](../../erp-client/features/supplier-settlements/components/settlement-list.tsx#L25)

<details><summary>本文件中文字符串索引（包含非弹窗、隐藏说明与动态分支，不等同可见弹窗文案）</summary>

- L23：请选择供应商
- L24：请选择期间起
- L25：请选择期间止
- L66：新建结算草稿
- L68：选择供应商与结算期间，创建后进入待对账。
- L83：供应商
- L92：请选择供应商
- L102：期间起
- L121：期间止
- L143：取消
- L148：确认创建草稿

</details>

## features/supplier-settlements/components/settlement-center-dialogs.tsx

**节点：** Dialog（行 46）；Dialog（行 161）；Dialog（行 237）；FormalActionConfirmDialog（行 317）；FormalActionConfirmDialog（行 377）

**建议：** D06、D08、D22

**直接导入方：** [features/supplier-settlements/components/settlement-center.tsx](../../erp-client/features/supplier-settlements/components/settlement-center.tsx#L18)

<details><summary>本文件中文字符串索引（包含非弹窗、隐藏说明与动态分支，不等同可见弹窗文案）</summary>

- L49：登记差异处理结论
- L51：财务经办追加式结论；不修改左右证据原值或历史成本。结论一经登记不可撤回，将写入审计并改变待确认成本差额。
- L57：受控结论
- L81：原因码
- L92：账单已对齐
- L96：接受供应商账单
- L100：无需业务调整
- L104：已另行补偿
- L119：取消
- L133：提交中…
- L133：提交结论
- L164：追加采购协同证据
- L166：只追加供应商证据或业务意见和审计，不改变差异结论、试算金额或成本基线。
- L171：正式证据引用
- L177：例如 ticket://T-123 或 attachment://...
- L182：业务说明
- L199：取消
- L213：保存中…
- L213：保存证据
- L240：驳回复核
- L242：原因必填，退回经办并保留记录。
- L247：原因码
- L254：请选择
- L257：证据不足
- L261：金额仍不一致
- L263：其他
- L265：请选择
- L277：取消
- L291：提交中…
- L291：确认驳回
- L321：提交复核
- L322：将冻结来源更新时间、明细与差异结论，并创建唯一复核待办。
- L323：提交复核
- L324：确认提交
- L329：待复核
- L332：来源数据、明细与差异结论已锁定
- L334：冻结来源数据与差异结论
- L334：创建结算复核待办
- L338：复核人用户 ID
- L347：请输入明确的复核人用户 ID
- L350：系统将把复核待办直接分派给该用户。
- L381：确认结算（不可逆）
- L382：同一次提交追加成本差额、形成唯一应付并锁定处理结果。经办人不可确认本单。
- L383：确认结算
- L384：确认结算
- L389：已确认
- L397：追加成本差额记录
- L398：形成唯一供应商结算应付
- L399：锁定处理结果，不可撤回确认
- L401：确认后付款/进项发票/核销进入供应商往来
- L402：供应商往来

</details>

## features/supplier-settlements/components/settlement-center.tsx

**节点：** SettlementResolveDialog（行 265）；SettlementEvidenceDialog（行 276）；SettlementSubmitReviewDialog（行 287）；SettlementConfirmSettlementDialog（行 299）；SettlementRejectDialog（行 310）

**建议：** 未列为优先修改项：保留当前结构；调用/装配代码跟随关联弹窗调整。未逐状态实操，不构成运行时验收。

**直接导入方：** [features/workspace/components/workspace-settlement-task.tsx](../../erp-client/features/workspace/components/workspace-settlement-task.tsx#L6)、[features/supplier-settlements/pages/supplier-settlements-page.tsx](../../erp-client/features/supplier-settlements/pages/supplier-settlements-page.tsx#L6)

## features/supplier-settlements/components/settlement-list.tsx

**节点：** CreateDraftDialog（行 374）

**建议：** 未列为优先修改项：保留当前结构；调用/装配代码跟随关联弹窗调整。未逐状态实操，不构成运行时验收。

**直接导入方：** [features/supplier-settlements/pages/supplier-settlements-page.tsx](../../erp-client/features/supplier-settlements/pages/supplier-settlements-page.tsx#L7)

## features/workspace/components/workspace-acceptance-task.tsx

**节点：** WorkspaceDocumentPaperDialog（行 132）

**建议：** 未列为优先修改项：保留当前结构；调用/装配代码跟随关联弹窗调整。未逐状态实操，不构成运行时验收。

**直接导入方：** [features/workspace/components/workspace-task-detail.tsx](../../erp-client/features/workspace/components/workspace-task-detail.tsx#L55)

## features/workspace/components/workspace-document-paper-dialog.tsx

**节点：** Dialog（行 43）

**建议：** D24

**直接导入方：** [features/workspace/components/workspace-acceptance-task.tsx](../../erp-client/features/workspace/components/workspace-acceptance-task.tsx#L29)、[features/workspace/components/workspace-payment-task.tsx](../../erp-client/features/workspace/components/workspace-payment-task.tsx#L47)、[features/workspace/components/workspace-task-detail.tsx](../../erp-client/features/workspace/components/workspace-task-detail.tsx#L70)

<details><summary>本文件中文字符串索引（包含非弹窗、隐藏说明与动态分支，不等同可见弹窗文案）</summary>

- L51：单据纸质预览
- L54：系统业务数据的打印件；金额与状态以系统记录为准。按 Esc                     或点击遮罩关闭。版本、附件和关联单据仍在对应工作面查看。
- L72：关闭预览
- L141：正在读取单据
- L142：正在读取单据…
- L161：单据读取失败
- L172：未找到这张单据，可能已删除或当前角色无权查看。

</details>

## features/workspace/components/workspace-fulfillment-task.tsx

**节点：** WorkspaceFulfillmentReassignDialog（行 164）；Dialog（行 261）；Dialog（行 385）

**建议：** D23

**直接导入方：** [features/workspace/components/workspace-task-detail.tsx](../../erp-client/features/workspace/components/workspace-task-detail.tsx#L56)

<details><summary>本文件中文字符串索引（包含非弹窗、隐藏说明与动态分支，不等同可见弹窗文案）</summary>

- L45：请选择新责任人
- L48：转交原因最多 150 个字符
- L49：请填写转交原因
- L104：当前履约任务
- L108：任务责任与履约对象不一致
- L110：请联系管理员核对责任人、对象类型与任务原因后重试。
- L121：履约处理
- L128：当前任务
- L161：转交责任
- L220：请选择当前合格人员，并填写转交原因
- L248：任务责任未转交，请刷新后重试
- L268：转交履约责任
- L270：当前责任人：
- L271：。采购单责任转交会同步更新该采购单全部开放交付任务；历史任务不变。
- L283：候选人员加载失败
- L287：请关闭后重试
- L294：没有转交
- L303：新责任人
- L305：选择合格人员
- L306：没有同时满足当前责任约束的人员
- L307：最终提交时会再次校验账号状态、完整操作权限与全部开放任务。
- L319：转交原因
- L320：说明本次责任调整依据
- L338：取消
- L347：确认转交
- L348：转交中…
- L383：处理履约
- L400：处理履约
- L402：填写本次履约信息并确认提交。

</details>

## features/workspace/components/workspace-payment-task.tsx

**节点：** WorkspaceDocumentPaperDialog（行 219）

**建议：** 未列为优先修改项：保留当前结构；调用/装配代码跟随关联弹窗调整。未逐状态实操，不构成运行时验收。

**直接导入方：** [features/workspace/components/workspace-task-detail.tsx](../../erp-client/features/workspace/components/workspace-task-detail.tsx#L63)

## features/workspace/components/workspace-procurement-task.tsx

**节点：** SalesOrderPaperPreviewDialog（行 115）

**建议：** 未列为优先修改项：保留当前结构；调用/装配代码跟随关联弹窗调整。未逐状态实操，不构成运行时验收。

**直接导入方：** [features/workspace/components/workspace-task-detail.tsx](../../erp-client/features/workspace/components/workspace-task-detail.tsx#L64)

## features/workspace/components/workspace-task-detail.tsx

**节点：** WorkspaceDocumentPaperDialog（行 680）

**建议：** 未列为优先修改项：保留当前结构；调用/装配代码跟随关联弹窗调整。未逐状态实操，不构成运行时验收。

**直接导入方：** [features/workspace/pages/workspace-home-page.tsx](../../erp-client/features/workspace/pages/workspace-home-page.tsx#L40)

