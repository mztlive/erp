//! 履约草稿与 W01 执行任务的原子责任编排。
//!
//! 本模块只接受已经持久化于调用方事务中的履约对象；责任来源固定为采购单当前
//! 责任人或仓库分操作经办人。任务创建、活动记录和完成必须复用调用方事务，
//! 禁止责任池、创建人或任意默认仓库回退。

use database::{AccessControlExt, Executor, PurchaseOrderExt, SalesOrderExt, WarehouseExt, WorkItemExt};
use entities::fulfillment::{
    Delivery, DeliveryType, ElectronicDelivery, PurchaseReceipt, ServiceFulfillment,
};
use entities::warehouse::WarehouseFulfillmentOperation;
use entities::work_item::{
    AssignmentSource, AvailableWorkItemAccount, WorkItem, WorkItemData, WorkItemPriority, WorkItemType,
};
use entities::{Permission, PermissionSet};
use id_generator::next_id;

use crate::errors::{Error, Result};
use crate::iam::SharedRbacService;

const PURCHASE_ORDER_OWNER_ROLE: &str = "purchase_order_owner";
const WAREHOUSE_INBOUND_ROLE: &str = "warehouse_inbound_handler";
const WAREHOUSE_OUTBOUND_ROLE: &str = "warehouse_outbound_handler";

/// 可形成履约执行任务的草稿对象。
pub(crate) enum FulfillmentTaskObject<'a> {
    /// 采购到货入库。
    PurchaseReceipt(&'a PurchaseReceipt),
    /// 仓发或供应商直发。
    Delivery(&'a Delivery),
    /// 电子交付。
    ElectronicDelivery(&'a ElectronicDelivery),
    /// 线下服务履约。
    ServiceFulfillment(&'a ServiceFulfillment),
}

impl FulfillmentTaskObject<'_> {
    fn business_object_type(&self) -> &'static str {
        match self {
            Self::PurchaseReceipt(_) => "purchase_receipt",
            Self::Delivery(_) => "delivery",
            Self::ElectronicDelivery(_) => "electronic_delivery",
            Self::ServiceFulfillment(_) => "service_fulfillment",
        }
    }

    fn business_object_id(&self) -> &str {
        match self {
            Self::PurchaseReceipt(receipt) => &receipt.base.id,
            Self::Delivery(delivery) => &delivery.base.id,
            Self::ElectronicDelivery(delivery) => &delivery.base.id,
            Self::ServiceFulfillment(fulfillment) => &fulfillment.base.id,
        }
    }

    fn subject_version(&self) -> String {
        match self {
            Self::PurchaseReceipt(receipt) => receipt.base.version.to_string(),
            Self::Delivery(delivery) => delivery.base.version.to_string(),
            Self::ElectronicDelivery(delivery) => delivery.base.version.to_string(),
            Self::ServiceFulfillment(fulfillment) => fulfillment.base.version.to_string(),
        }
    }

    fn reason_code(&self) -> &'static str {
        match self {
            Self::PurchaseReceipt(_) => "PURCHASE_RECEIPT_READY",
            Self::Delivery(delivery) if delivery.delivery_type == DeliveryType::WarehouseShip => {
                "WAREHOUSE_DELIVERY_READY"
            }
            Self::Delivery(_) => "SUPPLIER_DIRECT_DELIVERY_READY",
            Self::ElectronicDelivery(_) => "ELECTRONIC_DELIVERY_READY",
            Self::ServiceFulfillment(_) => "SERVICE_FULFILLMENT_READY",
        }
    }

    fn impact_summary(&self) -> String {
        match self {
            Self::PurchaseReceipt(_) => "采购入库待确认".to_string(),
            Self::Delivery(delivery) => format!("{}待发货", delivery.delivery_type.label()),
            Self::ElectronicDelivery(_) => "电子交付待确认".to_string(),
            Self::ServiceFulfillment(_) => "服务履约待确认".to_string(),
        }
    }

    /// 返回由履约对象冻结的责任角色与责任键，不读取当前配置作为回退。
    fn frozen_identity(&self) -> Result<FrozenFulfillmentIdentity> {
        let (owner_role, responsibility_key) = match self {
            Self::PurchaseReceipt(receipt) => (
                WAREHOUSE_INBOUND_ROLE,
                format!("warehouse:{}:receipt", receipt.warehouse_id),
            ),
            Self::Delivery(delivery) if delivery.delivery_type == DeliveryType::WarehouseShip => {
                let warehouse_id = delivery.warehouse_id.as_ref().ok_or_else(|| {
                    Error::BusinessLogicError("仓发单缺少发货仓库，无法校验责任任务".to_string())
                })?;
                (
                    WAREHOUSE_OUTBOUND_ROLE,
                    format!("warehouse:{warehouse_id}:warehouse_ship"),
                )
            }
            Self::Delivery(delivery) => {
                let purchase_order_id = delivery.purchase_order_id.as_ref().ok_or_else(|| {
                    Error::BusinessLogicError("供应商直发单缺少采购单，无法校验责任任务".to_string())
                })?;
                (
                    PURCHASE_ORDER_OWNER_ROLE,
                    format!("purchase_order:{purchase_order_id}"),
                )
            }
            Self::ElectronicDelivery(delivery) => (
                PURCHASE_ORDER_OWNER_ROLE,
                format!("purchase_order:{}", delivery.purchase_order_id),
            ),
            Self::ServiceFulfillment(fulfillment) => (
                PURCHASE_ORDER_OWNER_ROLE,
                format!("purchase_order:{}", fulfillment.purchase_order_id),
            ),
        };
        Ok(FrozenFulfillmentIdentity {
            owner_role,
            responsibility_key,
        })
    }
}

struct FrozenFulfillmentIdentity {
    owner_role: &'static str,
    responsibility_key: String,
}

struct FulfillmentTaskOwner {
    owner_user_id: String,
    owner_role: &'static str,
    owner_organization_id: String,
    responsibility_key: String,
}

/// 为履约草稿建立唯一开放执行任务。
///
/// # 参数
/// * `db` - MongoDB 数据库
/// * `object` - 已在当前事务中形成的履约草稿
/// * `executor` - 调用方事务执行器
///
/// # 返回
/// 新任务创建成功或同一对象已存在唯一开放任务时返回成功。
///
/// # 错误
/// 采购单、目标仓库或唯一责任人缺失/不可用，开放任务重复，或仓储写入失败时返回错误。
///
/// # 关键业务约束
/// 采购履约继承采购单当前责任人；入库与仓发分别取仓库入库、仓发经办人。
/// 责任配置只影响新任务，已存在任务不得在幂等重入时被配置变化静默改派。
pub(crate) async fn ensure_fulfillment_task(
    db: &mongodb::Database,
    object: FulfillmentTaskObject<'_>,
    executor: &mut dyn Executor,
) -> Result<()> {
    let existing = open_fulfillment_tasks(
        db,
        object.business_object_type(),
        object.business_object_id(),
        executor,
    )
    .await?;
    match existing.as_slice() {
        [] => {}
        [task] => {
            ensure_task_matches_frozen_identity(task, &object)?;
            return Ok(());
        }
        _ => {
            return Err(Error::BusinessLogicError(
                "履约对象存在多个开放执行任务，请联系管理员处理后重试".to_string(),
            ));
        }
    }

    let owner = resolve_task_owner(db, &object, executor).await?;
    let rbac = crate::iam::shared_rbac_service(db.clone());
    ensure_fulfillment_owner_eligible(
        db,
        &rbac,
        &owner.owner_user_id,
        object.business_object_type(),
        executor,
    )
    .await?;
    let task = WorkItem::new_with_responsibility_key(
        entities::ids::WorkItemId::new(next_id()),
        WorkItemData {
            work_item_type: WorkItemType::FulfillmentOperation,
            business_object_type: object.business_object_type().to_string(),
            business_object_id: object.business_object_id().to_string(),
            subject_version: object.subject_version(),
            owner_role: owner.owner_role.to_string(),
            owner_organization_id: owner.owner_organization_id,
            owner_user_id: owner.owner_user_id,
            assignment_source: AssignmentSource::SystemRule,
            priority: WorkItemPriority::Normal,
            due_at: None,
            reason_code: Some(object.reason_code().to_string()),
            impact_summary: Some(object.impact_summary()),
        },
        owner.responsibility_key,
    )?;
    ensure_task_matches_frozen_identity(&task, &object)?;
    db.work_items().create(&task, executor).await?;
    Ok(())
}

/// 在领域草稿更新事务内记录当前责任人的处理活动。
///
/// # 参数
/// * `db` - MongoDB 数据库
/// * `object` - 已更新且冻结责任身份的强类型履约对象
/// * `actor_id` - 当前操作人
/// * `executor` - 调用方事务执行器
///
/// # 返回
/// 活动与任务版本写入成功时返回成功。
///
/// # 错误
/// 任务缺失、重复、非当前责任人或仓储写入失败时返回错误。
pub(crate) async fn record_fulfillment_activity(
    db: &mongodb::Database,
    object: FulfillmentTaskObject<'_>,
    actor_id: &str,
    executor: &mut dyn Executor,
) -> Result<()> {
    let mut task = load_single_open_task(
        db,
        object.business_object_type(),
        object.business_object_id(),
        executor,
    )
    .await?;
    ensure_task_matches_frozen_identity(&task, &object)?;
    task.record_activity(actor_id, entities::common::time::Instant::now())?;
    ensure_current_owner_execution_access(db, &task, actor_id, executor).await?;
    db.work_items().update(&mut task, executor).await?;
    Ok(())
}

/// 在强类型履约命令事务内完成对应执行任务。
///
/// # 参数
/// * `db` - MongoDB 数据库
/// * `object` - 已形成正式事实且冻结责任身份的强类型履约对象
/// * `actor_id` - 当前操作人
/// * `executor` - 调用方事务执行器
///
/// # 返回
/// 领域事实与任务可以在调用方同一事务提交时返回成功。
///
/// # 错误
/// 任务缺失、重复、非当前责任人或仓储写入失败时返回错误。
pub(crate) async fn complete_fulfillment_task(
    db: &mongodb::Database,
    object: FulfillmentTaskObject<'_>,
    actor_id: &str,
    executor: &mut dyn Executor,
) -> Result<()> {
    let mut task = load_single_open_task(
        db,
        object.business_object_type(),
        object.business_object_id(),
        executor,
    )
    .await?;
    ensure_task_matches_frozen_identity(&task, &object)?;
    task.complete_by_domain_command(actor_id, entities::common::time::Instant::now())?;
    ensure_current_owner_execution_access(db, &task, actor_id, executor).await?;
    db.work_items().update(&mut task, executor).await?;
    Ok(())
}

/// 校验开放任务与强类型履约对象冻结的责任身份完全一致。
fn ensure_task_matches_frozen_identity(task: &WorkItem, object: &FulfillmentTaskObject<'_>) -> Result<()> {
    let identity = object.frozen_identity()?;
    ensure_frozen_identity_fields(
        task,
        object.business_object_type(),
        object.business_object_id(),
        identity.owner_role,
        &identity.responsibility_key,
        object.reason_code(),
    )
}

/// 校验任务对象、责任角色、责任键与原因码；任一不一致均失败关闭。
fn ensure_frozen_identity_fields(
    task: &WorkItem,
    business_object_type: &str,
    business_object_id: &str,
    owner_role: &str,
    responsibility_key: &str,
    reason_code: &str,
) -> Result<()> {
    let matches = task.work_item_type == WorkItemType::FulfillmentOperation
        && task.business_object_type == business_object_type
        && task.business_object_id == business_object_id
        && task.owner_role == owner_role
        && task.responsibility_key() == Some(responsibility_key)
        && task.reason_code.as_deref() == Some(reason_code);
    if matches {
        return Ok(());
    }
    Err(Error::BusinessLogicError(
        "履约任务责任身份与业务对象不一致，请联系管理员修复后重试".to_string(),
    ))
}

async fn load_single_open_task(
    db: &mongodb::Database,
    business_object_type: &str,
    business_object_id: &str,
    executor: &mut dyn Executor,
) -> Result<WorkItem> {
    let tasks = open_fulfillment_tasks(db, business_object_type, business_object_id, executor).await?;
    match tasks.as_slice() {
        [task] => Ok(task.clone()),
        [] => Err(Error::BusinessLogicError(
            "当前履约单缺少开放执行任务，请联系管理员补齐责任后重试".to_string(),
        )),
        _ => Err(Error::BusinessLogicError(
            "当前履约单存在多个开放执行任务，请联系管理员处理后重试".to_string(),
        )),
    }
}

async fn open_fulfillment_tasks(
    db: &mongodb::Database,
    business_object_type: &str,
    business_object_id: &str,
    executor: &mut dyn Executor,
) -> Result<Vec<WorkItem>> {
    Ok(db
        .work_items()
        .list_active_by_object(business_object_type, business_object_id, executor)
        .await?
        .into_iter()
        .filter(|item| item.work_item_type == WorkItemType::FulfillmentOperation)
        .collect())
}

async fn resolve_task_owner(
    db: &mongodb::Database,
    object: &FulfillmentTaskObject<'_>,
    executor: &mut dyn Executor,
) -> Result<FulfillmentTaskOwner> {
    match object {
        FulfillmentTaskObject::PurchaseReceipt(receipt) => {
            let order = db
                .purchase_orders()
                .find_by_id(&receipt.purchase_order_id, executor)
                .await?
                .ok_or_else(|| {
                    Error::BusinessLogicError("采购入库单来源采购单不存在，无法形成责任任务".to_string())
                })?;
            if order.target_warehouse_for_receipt()? != &receipt.warehouse_id {
                return Err(Error::BusinessLogicError(
                    "采购入库仓与采购单目标仓不一致，请修正后重试".to_string(),
                ));
            }
            warehouse_owner(
                db,
                &receipt.warehouse_id,
                WarehouseFulfillmentOperation::Receipt,
                executor,
            )
            .await
        }
        FulfillmentTaskObject::Delivery(delivery) => match delivery.delivery_type {
            DeliveryType::WarehouseShip => {
                let warehouse_id = delivery.warehouse_id.as_ref().ok_or_else(|| {
                    Error::BusinessLogicError("仓发单缺少发货仓库，无法形成责任任务".to_string())
                })?;
                warehouse_owner(
                    db,
                    warehouse_id,
                    WarehouseFulfillmentOperation::WarehouseShip,
                    executor,
                )
                .await
            }
            DeliveryType::SupplierDirect => {
                let purchase_order_id = delivery.purchase_order_id.as_ref().ok_or_else(|| {
                    Error::BusinessLogicError("供应商直发单缺少采购单，无法形成责任任务".to_string())
                })?;
                purchase_order_owner(db, purchase_order_id.as_ref(), executor).await
            }
        },
        FulfillmentTaskObject::ElectronicDelivery(delivery) => {
            purchase_order_owner(db, delivery.purchase_order_id.as_ref(), executor).await
        }
        FulfillmentTaskObject::ServiceFulfillment(fulfillment) => {
            purchase_order_owner(db, fulfillment.purchase_order_id.as_ref(), executor).await
        }
    }
}

async fn warehouse_owner(
    db: &mongodb::Database,
    warehouse_id: &entities::ids::WarehouseId,
    operation: WarehouseFulfillmentOperation,
    executor: &mut dyn Executor,
) -> Result<FulfillmentTaskOwner> {
    let warehouse = db
        .warehouses()
        .find_by_id(warehouse_id, executor)
        .await?
        .ok_or_else(|| Error::BusinessLogicError("履约仓库不存在，请重新选择".to_string()))?;
    let owner_user_id = warehouse.fulfillment_handler(operation)?.to_string();
    let (owner_role, suffix) = match operation {
        WarehouseFulfillmentOperation::Receipt => (WAREHOUSE_INBOUND_ROLE, "receipt"),
        WarehouseFulfillmentOperation::WarehouseShip => (WAREHOUSE_OUTBOUND_ROLE, "warehouse_ship"),
    };
    Ok(FulfillmentTaskOwner {
        owner_user_id,
        owner_role,
        owner_organization_id: warehouse.base.id.clone(),
        responsibility_key: format!("warehouse:{}:{suffix}", warehouse.base.id),
    })
}

async fn purchase_order_owner(
    db: &mongodb::Database,
    purchase_order_id: &str,
    executor: &mut dyn Executor,
) -> Result<FulfillmentTaskOwner> {
    let order = db
        .purchase_orders()
        .find_by_id(
            &entities::ids::PurchaseOrderId::new(purchase_order_id.to_string()),
            executor,
        )
        .await?
        .ok_or_else(|| Error::BusinessLogicError("履约来源采购单不存在，无法形成责任任务".to_string()))?;
    let sales_order = db
        .sales_orders()
        .find_by_id(&order.sales_order_id, executor)
        .await?
        .ok_or_else(|| Error::BusinessLogicError("采购单来源销售单不存在，无法形成责任任务".to_string()))?;
    Ok(FulfillmentTaskOwner {
        owner_user_id: order.current_owner_user_id()?.to_string(),
        owner_role: PURCHASE_ORDER_OWNER_ROLE,
        owner_organization_id: sales_order.settlement_party_id.to_string(),
        responsibility_key: format!("purchase_order:{}", order.base.id),
    })
}

/// 校验唯一责任人账号有效并具备指定履约对象的完整执行权限。
///
/// # 参数
/// * `db` - MongoDB 数据库
/// * `rbac` - 与调用事务授权版本一致的 RBAC 服务
/// * `owner_user_id` - 待验证的具体责任账号
/// * `business_object_type` - 已登记权限合同的履约对象类型
/// * `executor` - 当前数据访问执行器
///
/// # 返回
/// 账号可登录且当前角色覆盖完整执行权限时返回成功。
///
/// # 错误
/// 对象权限合同缺失、账号不存在或不可用、权限不足，以及账号或 RBAC 查询失败时返回错误。
pub(crate) async fn ensure_fulfillment_owner_eligible(
    db: &mongodb::Database,
    rbac: &SharedRbacService,
    owner_user_id: &str,
    business_object_type: &str,
    executor: &mut dyn Executor,
) -> Result<()> {
    let required_permissions = WorkItemType::FulfillmentOperation
        .fulfillment_execution_permissions(business_object_type)
        .ok_or_else(|| Error::Internal("履约对象未登记完整执行权限，请联系管理员修复后重试".to_string()))?;
    let account = db
        .accounts()
        .find_work_item_account(owner_user_id, executor)
        .await?
        .ok_or_else(|| {
            Error::BusinessLogicError("履约责任人账号不存在，请先调整采购单或仓库责任配置".to_string())
        })?;
    AvailableWorkItemAccount::from_account(&account).map_err(|_| {
        Error::BusinessLogicError("履约责任人账号不可用，请先调整采购单或仓库责任配置".to_string())
    })?;
    let granted = PermissionSet::new(rbac.permissions(account.kind, owner_user_id).await?);
    let required = PermissionSet::new(
        required_permissions
            .iter()
            .map(|code| Permission::parse(code).expect("固定履约操作权限必须合法")),
    );
    if granted.covers(&required) {
        return Ok(());
    }
    Err(Error::BusinessLogicError(
        "履约责任人缺少完成当前操作所需权限，请先调整角色或责任配置".to_string(),
    ))
}

/// 在写入活动或完成事实前重验当前责任人的账号状态与完整执行权限。
async fn ensure_current_owner_execution_access(
    db: &mongodb::Database,
    task: &WorkItem,
    actor_id: &str,
    executor: &mut dyn Executor,
) -> Result<()> {
    let rbac = crate::iam::shared_rbac_service(db.clone());
    ensure_fulfillment_owner_eligible(db, &rbac, actor_id, &task.business_object_type, executor).await
}

#[cfg(test)]
mod tests {
    use entities::ids::WorkItemId;

    use super::{ensure_frozen_identity_fields, AssignmentSource, WorkItem, WorkItemData};
    use entities::work_item::{WorkItemPriority, WorkItemType};

    fn fulfillment_task() -> WorkItem {
        WorkItem::new_with_responsibility_key(
            WorkItemId::new("work-item-1"),
            WorkItemData {
                work_item_type: WorkItemType::FulfillmentOperation,
                business_object_type: "delivery".to_string(),
                business_object_id: "delivery-1".to_string(),
                subject_version: "1".to_string(),
                owner_role: "purchase_order_owner".to_string(),
                owner_organization_id: "party-1".to_string(),
                owner_user_id: "buyer-1".to_string(),
                assignment_source: AssignmentSource::SystemRule,
                priority: WorkItemPriority::Normal,
                due_at: None,
                reason_code: Some("SUPPLIER_DIRECT_DELIVERY_READY".to_string()),
                impact_summary: None,
            },
            "purchase_order:po-1",
        )
        .expect("测试履约任务应合法")
    }

    #[test]
    fn formal_command_requires_full_frozen_responsibility_identity() {
        let task = fulfillment_task();
        assert!(ensure_frozen_identity_fields(
            &task,
            "delivery",
            "delivery-1",
            "purchase_order_owner",
            "purchase_order:po-1",
            "SUPPLIER_DIRECT_DELIVERY_READY",
        )
        .is_ok());
        assert!(ensure_frozen_identity_fields(
            &task,
            "delivery",
            "delivery-1",
            "warehouse_outbound_handler",
            "purchase_order:po-1",
            "SUPPLIER_DIRECT_DELIVERY_READY",
        )
        .is_err());
        assert!(ensure_frozen_identity_fields(
            &task,
            "delivery",
            "delivery-1",
            "purchase_order_owner",
            "purchase_order:po-2",
            "SUPPLIER_DIRECT_DELIVERY_READY",
        )
        .is_err());
        assert!(ensure_frozen_identity_fields(
            &task,
            "delivery",
            "delivery-1",
            "purchase_order_owner",
            "purchase_order:po-1",
            "WAREHOUSE_DELIVERY_READY",
        )
        .is_err());
    }
}
