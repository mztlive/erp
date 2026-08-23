/**
 * 审计事件的动作与对象文案。
 *
 * 后端 action_type 约定为 `<对象>.<动作>`（如 `user_role.assign`），
 * 界面不展示这种编码：对象名取自权限目录，动作名走下表。
 */

import { resourceLabel } from "@/features/admin/lib/permission-catalog"

/** 审计动作动词中文名。 */
const AUDIT_VERB_LABEL: Record<string, string> = {
    create: "新建",
    update: "修改",
    delete: "删除",
    assign: "授权",
    revoke: "撤权",
    submit: "提交",
    approve: "通过",
    reject: "驳回",
    cancel: "取消",
    close: "关闭",
    post: "记账",
    reverse: "冲销",
    void: "作废",
    export: "导出",
    query: "查询",
    login: "登录",
    reveal: "查看敏感信息",
}

/** 审计动作是否为高风险（列表可据此加重强调）。 */
const RISKY_VERBS = new Set(["delete", "revoke", "reverse", "void", "reveal"])

function splitActionType(actionType: string): {
    object: string
    verb: string
} | null {
    const index = actionType.lastIndexOf(".")
    if (index <= 0 || index === actionType.length - 1) return null
    return {
        object: actionType.slice(0, index),
        verb: actionType.slice(index + 1),
    }
}

/** 审计对象类型 → 中文对象名；未知类型原样返回。 */
export function auditObjectTypeLabel(objectType: string): string {
    return resourceLabel(objectType)
}

/** 审计动作 → 「对象 · 动作」；不符合约定的取值原样展示。 */
export function auditActionLabel(actionType: string): string {
    const parts = splitActionType(actionType)
    if (!parts) return actionType
    const verb = AUDIT_VERB_LABEL[parts.verb] ?? parts.verb
    return `${auditObjectTypeLabel(parts.object)} · ${verb}`
}

/** 是否为高风险动作。 */
export function isRiskyAuditAction(actionType: string): boolean {
    const parts = splitActionType(actionType)
    return parts ? RISKY_VERBS.has(parts.verb) : false
}
