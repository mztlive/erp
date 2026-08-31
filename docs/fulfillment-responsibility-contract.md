# 履约责任与 W01/W06 作业合同

状态：生效  
适用范围：采购入库、仓库发货、供应商直发、电子交付、线下服务交付、销售客户验收
适用工作面：W01 我的工作台、W06 客户验收

## 1. 强制规则

1. 每个开放履约对象必须且只能存在一个 `FULFILLMENT_OPERATION` 开放任务。
2. 履约对象的正式命令与对应工作项完成事实必须在同一后端事务提交。
3. 客户端不得根据岗位、创建人、默认仓库或本地推荐结果补写责任事实。
4. 责任人缺失、账号不可用、权限不足、目标仓缺失或责任配置不唯一时，命令必须失败关闭。
5. W01 每次只允许处理当前工作项绑定的一个履约对象；不得在作业面内建立第二套履约队列。
6. `/fulfillment` 仅作为旧地址兼容入口，必须跳转至 W01 履约任务范围。
7. 发货或交付确认与 W06 客户验收责任形成必须在同一后端事务提交。
8. 客户验收过账、销售单履约进度和验收任务活动或完成事实必须在同一后端事务提交。

开放唯一性必须由部分唯一索引 `uk_work_items_open_fulfillment_object` 强制；应用层先查后建只用于幂等提示，不得替代数据库并发约束。

## 2. 责任映射

| 履约操作     | 责任事实来源                     | 工作项责任角色               | 必需执行权限                               |
| ------------ | -------------------------------- | ---------------------------- | ------------------------------------------ |
| 采购入库     | 采购单冻结的目标仓库之入库经办人 | `warehouse_inbound_handler`  | `purchase_receipt:list/detail/update/post` |
| 仓库发货     | 发货单指定仓库之仓发经办人       | `warehouse_outbound_handler` | `delivery:list/detail/update/post`         |
| 供应商直发   | 采购单当前责任人                 | `purchase_order_owner`       | `delivery:list/detail/update/post`         |
| 电子交付     | 采购单当前责任人                 | `purchase_order_owner`       | `electronic_delivery:list/confirm`         |
| 线下服务交付 | 采购单当前责任人                 | `purchase_order_owner`       | `service_fulfillment:list/confirm`         |

同一账号可以同时配置为同一仓库的入库经办人与仓发经办人。责任必须指定到具体有效账号，不得只指定角色或部门。

## 3. 采购单责任

1. 新建采购单时，`owner_user_id` 必须形成显式责任事实。
2. 入仓采购必须在采购单创建时写入 `target_warehouse_id`；非入仓采购不得写入该字段。
3. 目标仓库必须处于启用状态且已配置合格的入库经办人，才允许被采购建单界面选择。
4. 供应商直发、电子交付与服务交付任务必须读取采购单当前责任人，不得读取采购单创建人作为运行时回退。
5. 管理员通过开放履约工作项执行转交时，后端必须在同一事务中更新采购单当前责任人以及同一 `purchase_order:{id}` 下的全部开放履约任务。
6. 已完成或已关闭任务属于历史事实，不得随采购单责任变更而改写。
7. 转交候选人必须同时具备该采购单全部开放履约任务的完整执行权限；只满足当前一条任务不得进入候选列表。

## 4. 仓库责任配置

1. 每个启用仓库必须分别维护 `inbound_handler_user_id` 与 `outbound_handler_user_id`。
2. 配置界面只能列出有效管理账号，并分别标明入库资格与仓发资格。
3. 更新命令必须携带仓库当前 `version`，版本冲突时返回刷新重试提示。
4. 配置更新只影响更新后新建的履约任务；既有开放任务不得静默改派。
5. 配置更新必须写入审计日志，动作代码为 `warehouse.fulfillment_handlers.update`。

管理接口：

- `GET /admin/warehouse-fulfillment-handler-options`
- `PUT /admin/warehouses/{warehouse_id}/fulfillment-handlers`

## 5. 工作项合同

履约工作项必须写入下列稳定事实：

- `work_item_type = FULFILLMENT_OPERATION`
- `business_object_type` 为 `purchase_receipt`、`delivery`、`electronic_delivery` 或 `service_fulfillment`
- `business_object_id` 为唯一履约对象 ID
- `owner_user_id` 为映射规则解析出的具体账号
- `owner_role` 为本合同第 2 节规定的固定代码
- `responsibility_key` 为 `purchase_order:{id}`、`warehouse:{id}:receipt` 或 `warehouse:{id}:warehouse_ship`
- `reason_code` 为与对象和责任一致的固定原因码
- `handler_key = fulfillment_operation`
- `destination_workspace_id = W01`

对象类型、责任角色、原因码、处理器或目标工作面任一不一致时，W01 必须拒绝展开正式操作。

责任转交接口：

- `GET /admin/work-items/{work_item_id}/reassign-candidates`
- `POST /admin/work-items/{work_item_id}/reassign`

候选接口只提供当前授权快照下的交互选项。提交转交时必须在后端写事务内重新校验任务版本、管理员范围、账号状态、完整执行权限和采购级联范围。

## 6. W01 执行合同

1. W01 左侧使用统一工作项队列，并提供“履约”任务族筛选。
2. 选中履约任务后，右侧按工作项对象 ID 查询唯一强类型履约对象。
3. 右侧不得显示履约子队列、跳过当前单据、自动切换或岗位责任选择器。
4. 正式命令成功后，客户端只刷新统一工作项队列；工作项完成事实由同一后端事务产生，客户端不得另发完成命令。
5. 当前账号必须同时具备任务级 `PROCESS` 动作和对象完整执行权限，缺一即禁止保存或确认。
6. 具有任务级 `REASSIGN` 动作的管理员必须通过候选人员列表转交；客户端不得提交列表之外的本地推断账号。

工作项精确查询必须使用下列接口，不得先拉取分页列表后按 ID 筛选：

| 工作项对象类型        | 查询接口                                |
| --------------------- | --------------------------------------- |
| `purchase_receipt`    | `GET /admin/purchase-receipts/{id}`     |
| `delivery`            | `GET /admin/deliveries/{id}`            |
| `electronic_delivery` | `GET /admin/electronic-deliveries/{id}` |
| `service_fulfillment` | `GET /admin/service-fulfillments/{id}`  |

接口返回的对象 ID、对象类型或草稿状态与工作项冻结事实不一致时，W01 必须失败关闭，不得切换到同类型其他对象。

## 7. 客户验收责任合同

1. 仓库发货或供应商直发过账、电子交付确认、线下服务履约确认时，必须在同一事务建立客户验收任务。
2. 客户验收任务必须按销售单聚合；同一销售单不得按发货单或交付记录并存多条开放任务。
3. 任务责任人固定取销售单负责销售 `stable.created_by`，责任组织固定取销售单 `settlement_party_id`；新任务形成前必须校验账号可用及 W06 完整执行权限。
4. 客户验收任务必须写入下列稳定事实：
   - `work_item_type = CUSTOMER_ACCEPTANCE_REGISTRATION`
   - `business_object_type = sales_order`
   - `business_object_id = sales_order.id`
   - `owner_role = sales_order_owner`
   - `responsibility_key = sales_order:{id}:customer_acceptance`
   - `handler_key = customer_acceptance_registration`
   - `destination_workspace_id = W06`
5. 开放唯一性必须由部分唯一索引 `uk_work_items_open_customer_acceptance_object` 强制；应用层先查后建不得替代数据库并发约束。
6. W06 完整执行权限固定为 `customer_acceptance:list/detail/create/post`，并必须具备 `sales_order:detail` 对象读取权限。缺少任一权限时不得形成新任务、不得显示 `PROCESS`、不得提交验收正式命令。
7. 从统一工作台进入 W06 时，客户端必须携带 `work_item_id` 与 `expected_task_version`；后端必须同时校验任务类型、销售单、当前责任人、开放状态和乐观锁版本。
8. 从销售单直接进入 W06 时，客户端可以不携带任务身份；后端必须解析该销售单唯一开放任务。启用本合同前形成且尚无任务的遗留销售单，必须按本合同原子补建任务后再校验当前责任人，不得绕过任务责任。
9. 验收过账后仍存在可验收交付时，任务必须保持开放并记录活动；短少、拒收或服务不通过不得被本地结果文案误判为任务完成。
10. 验收过账后当前可验收交付清零时，任务必须由同一正式命令完成；客户端不得另发完成任务命令。
11. 已完成验收被冲正，或历史验收任务完成后形成新发货或交付时，必须新建开放任务；历史终态任务不得重开或覆盖。
12. 客户验收保持 `NO_APPROVAL`。同步 W06 人工责任不得绑定审批定义、启动审批实例或创建审批任务。

## 8. 上线数据门禁

上线前必须完成以下核对；任一项未通过时不得开放履约正式命令：

1. 所有仍会参与履约的启用仓库均已显式配置入库经办人与仓发经办人。
2. 所有未完成采购单均已显式填充有效 `owner_user_id`。
3. 所有未完成入仓采购单均已按经审核的业务事实填充唯一 `target_warehouse_id`。
4. 所有存量履约草稿均已形成唯一开放 `FULFILLMENT_OPERATION` 工作项；重复任务必须先关闭并保留审计依据。
5. 存量字段不得以创建人、首个启用仓库或任意默认账号批量回填；迁移清单必须逐对象可追溯。
6. 切换后必须验证 W01 可见性、任务责任、强类型命令、事务完成、管理员转交与审计记录。
7. 必须验证四类发货或交付均形成销售单级 W06 任务，并验证部分验收保持开放、全部验收完成、冲正重建以及统一工作台精确回跳。

存量对象缺少上述事实时，运行时代码必须保持失败关闭；兼容反序列化不构成业务回退授权。
