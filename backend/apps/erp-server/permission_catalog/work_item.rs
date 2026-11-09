//! work_item 的接口权限目录。
use super::{PermissionDef, groups};

/// 查询财务责任规则。
pub const FINANCE_RESPONSIBILITY_FINANCE_RESPONSIBILITY_RULE_LIST: PermissionDef = PermissionDef {
    code: "finance_responsibility:list",
    group: groups::WORK_ITEM,
    description: "查询财务责任规则",
};

/// 创建财务责任规则。
pub const FINANCE_RESPONSIBILITY_FINANCE_RESPONSIBILITY_RULE_CREATE: PermissionDef = PermissionDef {
    code: "finance_responsibility:manage",
    group: groups::WORK_ITEM,
    description: "创建财务责任规则",
};

/// 更新财务责任规则。
pub const FINANCE_RESPONSIBILITY_FINANCE_RESPONSIBILITY_RULE_UPDATE: PermissionDef =
    FINANCE_RESPONSIBILITY_FINANCE_RESPONSIBILITY_RULE_CREATE
        .described(groups::WORK_ITEM, "更新财务责任规则");

/// 查询财务负责人候选。
pub const FINANCE_RESPONSIBILITY_FINANCE_RESPONSIBILITY_OWNER_OPTIONS: PermissionDef =
    FINANCE_RESPONSIBILITY_FINANCE_RESPONSIBILITY_RULE_LIST
        .described(groups::WORK_ITEM, "查询财务负责人候选");

/// 查询本人授权范围内的待办。
pub const LIST: PermissionDef = PermissionDef {
    code: "work_item:list",
    group: groups::FINANCIAL_FILES,
    description: "查询本人授权范围内的待办",
};

/// 查询本人授权范围内的履约责任队列。
pub const FULFILLMENT_QUEUE_LIST: PermissionDef =
    LIST.described(groups::FINANCIAL_FILES, "查询本人授权范围内的履约责任队列");

/// 查询本人授权范围内的待办统计。
pub const STATS: PermissionDef = LIST.described(groups::FINANCIAL_FILES, "查询本人授权范围内的待办统计");

/// 查询本人有权查看的待办详情。
pub const DETAIL: PermissionDef = PermissionDef {
    code: "work_item:detail",
    group: groups::FINANCIAL_FILES,
    description: "查询本人有权查看的待办详情",
};

/// 查询非审批任务可转交人员。
pub const REASSIGN_CANDIDATES: PermissionDef =
    REASSIGN.described(groups::FINANCIAL_FILES, "查询非审批任务可转交人员");

/// 在授权范围内受控转交非审批任务。
pub const MANAGE: PermissionDef = PermissionDef {
    code: "work_item:manage",
    group: groups::FINANCIAL_FILES,
    description: "在授权范围内操作待办",
};

/// 在任务数据范围内转交。
pub const REASSIGN: PermissionDef = MANAGE.described(groups::FINANCIAL_FILES, "转交待办");

/// 在任务数据范围内关闭。
pub const CLOSE: PermissionDef = MANAGE.described(groups::FINANCIAL_FILES, "关闭待办");

/// 查询本人有权查看的待办详情。
pub const TIMELINE_TASK_TIMELINE: PermissionDef = DETAIL;

/// 原提交人放弃本人驳回处理任务；服务层再次校验责任身份与版本。
pub const ABANDON_REWORK: PermissionDef =
    MANAGE.described(groups::FINANCIAL_FILES, "原提交人放弃本人驳回处理任务");

/// 本模块可供路由引用的显式权限定义。
pub const ENTRIES: &[(&str, PermissionDef)] = &[
    (
        "FINANCE_RESPONSIBILITY_FINANCE_RESPONSIBILITY_RULE_LIST",
        FINANCE_RESPONSIBILITY_FINANCE_RESPONSIBILITY_RULE_LIST,
    ),
    (
        "FINANCE_RESPONSIBILITY_FINANCE_RESPONSIBILITY_RULE_CREATE",
        FINANCE_RESPONSIBILITY_FINANCE_RESPONSIBILITY_RULE_CREATE,
    ),
    (
        "FINANCE_RESPONSIBILITY_FINANCE_RESPONSIBILITY_RULE_UPDATE",
        FINANCE_RESPONSIBILITY_FINANCE_RESPONSIBILITY_RULE_UPDATE,
    ),
    (
        "FINANCE_RESPONSIBILITY_FINANCE_RESPONSIBILITY_OWNER_OPTIONS",
        FINANCE_RESPONSIBILITY_FINANCE_RESPONSIBILITY_OWNER_OPTIONS,
    ),
    ("LIST", LIST),
    ("MANAGE", MANAGE),
    ("FULFILLMENT_QUEUE_LIST", FULFILLMENT_QUEUE_LIST),
    ("STATS", STATS),
    ("DETAIL", DETAIL),
    ("REASSIGN_CANDIDATES", REASSIGN_CANDIDATES),
    ("REASSIGN", REASSIGN),
    ("CLOSE", CLOSE),
    ("ABANDON_REWORK", ABANDON_REWORK),
    ("TIMELINE_TASK_TIMELINE", TIMELINE_TASK_TIMELINE),
];
