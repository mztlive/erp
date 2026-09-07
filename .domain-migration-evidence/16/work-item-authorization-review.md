# 阶段16：正式任务窄授权读取实施记录

- 输入：`ed8015e28d36e5b9323b87e1a1fd5f63c122eb70`。
- 源码绑定：`72a0c79a2261d33699b869329376e534edfb1ef4`。
- 状态：源码已实施并完成定向格式与静态核验；Cargo/纯库测试由唯一集成人执行。
- 本次未运行 Cargo、真实数据库、外部服务或历史 tests。真实数据库运行未验证。

## 公共出口与行为合同

- 唯一消费端 WorkItemAuthorizationReadPort: Send+Sync，async_trait object-safe，authorize(&self,id,&AuditActor)->erp_workflow::Result<AuthorizedTaskFact>；不接 Executor，保留原授权读取快照边界。
- AuthorizedTaskFact 仅 work_item_type、business_object_type、business_object_id、subject_version、allowed_actions 五字段；没有 serde、WorkItem 实体、RBAC、processing_state 或 blocker 泄漏。
- 唯一生产 WorkItemAuthorizationAdapter::new(Database,SharedRbacService) 仅调用原 work_item_service 装配；authorize 原位调用 service.authorize_work_item().await?，直接返回原 typed workflow 错误，再显式映射五字段。
- W13 在原末参 rbac 后追加 task_auth:&dyn WorkItemAuthorizationReadPort；HTTP 由 root 注入。W26 由 G 消费同一个Port，本分片不改W26/HTTP。
- 实际 load_review_task helper 顺序：detail future先 await → work_item_id trim/空则返回原view且Port零调用 → authorize → CardFundsReview/Delta类型 → receivable_account类型/id/current sales revision及原||false → active review type → Process能力。
- 缺 Process 仍创建同五条 CURRENT_RESPONSIBILITY_REQUIRED blocker 后返回，不调用raw仓储；有 Process 才调用真实closure按 NoTransaction 再读 WorkItem，NotFound原文案，Open/当前owner不满足同样阻断。
- 新helper只把原W13读取/guard段移出；raw callback实际仓储方法/错误映射不改。返回原raw实体，保留其真实base.version，不能用第一次授权的事实省掉重读。
- 之后账户读取、snapshot、复核状态、岗位分离、领域动作投影和 customer_receipt:create → invoice:create 两次RBAC检查整段字节相同。rbac保留，不由Port.allowed_actions代替。
- 本轮不迁移旧workflow_compose/work_item/workbench事实reader；authority来源单一化留17。三个自有文件不改领域DTO或HTTP wire，不增加数据库/网关执行。

## 实际源码与哈希

| 文件 | SHA256 |
| --- | --- |
| `crates/erp-read-models/src/ports/work_item_authorization.rs` | `48d3ddb207620b5d4c66383c2d86cd8af980e6e5564bf895c4b8969ef12091d5` |
| `crates/erp-processes/src/adapters/workflow/work_item_authorization.rs` | `59ce765ce0c1821b4233f8e01c9a59c7f1c09b6343e2920917226389ac6fe53d` |
| `crates/erp-read-models/src/finance/receivable/account.rs` | `5c44822631e9578220876cee9e1fc62801d899e82cc85c2bd6678fb88df8970a` |

- 每个原函数和新增实际函数的行号、主体hash见同名JSON。原W13剩余账户/快照/岗位分离/登记权限段逐字节一致：`bc2f0b9d85fc57bc25e0217364a4bb676c33e432610ab33000cbd3d51c0c80a7`。
- 原工作流factory及authorize_work_item两个实际provider函数主体token与输入一致；不把装配合同当运行结果。

## 新增纯测试定义

- `empty_task_id_keeps_detail_and_skips_authority_and_raw_reload`。
- `detail_failure_precedes_authority`。
- `authority_error_precedes_formal_binding_checks_and_preserves_type`。
- `formal_type_object_identity_and_subject_checks_precede_raw_reload`。
- `missing_process_action_blocks_all_five_actions_without_raw_reload`。
- `raw_reload_failure_is_preserved_after_authorization`。
- `raw_reload_rechecks_open_status_and_current_owner`。
- `authorized_review_preserves_opening_and_delta_type_and_raw_task`。

- 新增8项均调用真实生产load_review_task，不复制授权/判断算法。尚未执行，不能登记为通过。
- root接W13追加的Port末参；G接W26同一Port；root维护注册和HTTP，当前分片无源删除。
