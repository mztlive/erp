//! work_item 的内置权限。
use crate::{Permission, PermissionDef, groups, resources};

/// 查询财务责任规则。
pub const FINANCE_RESPONSIBILITY_FINANCE_RESPONSIBILITY_RULE_LIST: Permission =
    Permission::new(resources::FINANCE_RESPONSIBILITY, "list");

/// 创建财务责任规则。
pub const FINANCE_RESPONSIBILITY_FINANCE_RESPONSIBILITY_RULE_CREATE: Permission =
    Permission::new(resources::FINANCE_RESPONSIBILITY, "manage");

/// 更新财务责任规则。
pub const FINANCE_RESPONSIBILITY_FINANCE_RESPONSIBILITY_RULE_UPDATE: Permission =
    FINANCE_RESPONSIBILITY_FINANCE_RESPONSIBILITY_RULE_CREATE;

/// 查询财务负责人候选。
pub const FINANCE_RESPONSIBILITY_FINANCE_RESPONSIBILITY_OWNER_OPTIONS: Permission =
    FINANCE_RESPONSIBILITY_FINANCE_RESPONSIBILITY_RULE_LIST;

/// 查询本人授权范围内的待办。
pub const LIST: Permission = Permission::new(resources::WORK_ITEM, "list");

/// 查询本人授权范围内的履约责任队列。
pub const FULFILLMENT_QUEUE_LIST: Permission = LIST;

/// 查询本人授权范围内的待办统计。
pub const STATS: Permission = LIST;

/// 查询本人有权查看的待办详情。
pub const DETAIL: Permission = Permission::new(resources::WORK_ITEM, "detail");

/// 查询非审批任务可转交人员。
pub const REASSIGN_CANDIDATES: Permission = REASSIGN;

/// 在授权范围内受控转交非审批任务。
pub const MANAGE: Permission = Permission::new(resources::WORK_ITEM, "manage");

/// 在任务数据范围内转交。
pub const REASSIGN: Permission = MANAGE;

/// 在任务数据范围内关闭。
pub const CLOSE: Permission = MANAGE;

/// 查询本人有权查看的待办详情。
pub const TIMELINE_TASK_TIMELINE: Permission = DETAIL;

/// 原提交人放弃本人驳回处理任务；入口校验待办详情权限，服务层再次校验原提交人身份与版本。
pub const ABANDON_REWORK: Permission = DETAIL;

/// 本模块独立权限及可复用展示说明；接口专属说明由应用提供。
pub const ENTRIES: &[(&str, PermissionDef)] = &[
    (
        "FINANCE_RESPONSIBILITY_FINANCE_RESPONSIBILITY_RULE_LIST",
        PermissionDef {
            permission: &FINANCE_RESPONSIBILITY_FINANCE_RESPONSIBILITY_RULE_LIST,
            group: groups::WORK_ITEM,
            description: "查询财务责任规则",
        },
    ),
    (
        "FINANCE_RESPONSIBILITY_FINANCE_RESPONSIBILITY_RULE_CREATE",
        PermissionDef {
            permission: &FINANCE_RESPONSIBILITY_FINANCE_RESPONSIBILITY_RULE_CREATE,
            group: groups::WORK_ITEM,
            description: "创建财务责任规则",
        },
    ),
    (
        "LIST",
        PermissionDef {
            permission: &LIST,
            group: groups::FINANCIAL_FILES,
            description: "查询本人授权范围内的待办",
        },
    ),
    (
        "DETAIL",
        PermissionDef {
            permission: &DETAIL,
            group: groups::FINANCIAL_FILES,
            description: "查询本人有权查看的待办详情",
        },
    ),
    (
        "MANAGE",
        PermissionDef {
            permission: &MANAGE,
            group: groups::FINANCIAL_FILES,
            description: "在授权范围内操作待办",
        },
    ),
];
