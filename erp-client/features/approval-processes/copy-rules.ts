import type {
    ApprovalCopyRule,
    CopyTrigger,
    DefinitionDetailView,
    EditorCopyRule,
    EditorNode,
} from "./types"
export const COPY_TRIGGER_LABELS: Record<CopyTrigger, string> = {
    STARTED: "提交审批后",
    NODE_APPROVED: "节点通过后",
    APPROVED: "审批通过后",
    REJECTED: "驳回后",
    CANCELLED: "撤回后",
}
export function seedCopyRules(
    detail: DefinitionDetailView,
    nodes: readonly EditorNode[],
): EditorCopyRule[] {
    return (detail.copy_rules ?? []).map((rule) => ({
        trigger: rule.trigger,
        nodeClientId: rule.node_order
            ? (nodes[rule.node_order - 1]?.client_id ?? null)
            : null,
        recipients: rule.recipient_user_ids.map((user_id) => ({
            user_id,
            name: detail.copy_recipient_names?.[user_id] ?? "待确认人员",
        })),
    }))
}
export function buildCopyRules(
    rules: readonly EditorCopyRule[],
    nodes: readonly EditorNode[],
): ApprovalCopyRule[] {
    return rules
        .map((rule) => {
            const order = rule.nodeClientId
                ? nodes.findIndex((n) => n.client_id === rule.nodeClientId) + 1
                : 0
            if (rule.trigger === "NODE_APPROVED" && !order)
                throw new Error("抄送关联的节点已删除，请重新设置抄送规则")
            if (!rule.recipients.length)
                throw new Error("请选择抄送人，或删除空的抄送规则")
            return {
                trigger: rule.trigger,
                node_order: rule.trigger === "NODE_APPROVED" ? order : null,
                recipient_user_ids: rule.recipients
                    .map((p) => p.user_id)
                    .sort(),
            }
        })
        .sort((a, b) =>
            `${a.trigger}:${a.node_order}`.localeCompare(
                `${b.trigger}:${b.node_order}`,
            ),
        )
}
