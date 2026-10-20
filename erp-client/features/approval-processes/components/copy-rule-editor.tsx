"use client"
import { Button } from "@/components/ui/button"
import { toAutomationIdSegment } from "@/lib/automation-id"
import { AssigneeCombobox } from "./assignee-combobox"
import { COPY_TRIGGER_LABELS } from "../copy-rules"
import type {
    CopyTrigger,
    DefinitionDetailView,
    DocumentType,
    EditorCopyRule,
    EditorNode,
} from "../types"

export function CopyRuleEditor({
    rules,
    nodes,
    documentType,
    disabled,
    onChange,
}: {
    rules: EditorCopyRule[]
    nodes: EditorNode[]
    documentType: DocumentType
    disabled: boolean
    onChange: (rules: EditorCopyRule[]) => void
}) {
    const slots = [
        ...(
            ["STARTED", "APPROVED", "REJECTED", "CANCELLED"] as CopyTrigger[]
        ).map((trigger) => ({
            trigger,
            nodeClientId: null,
            label: COPY_TRIGGER_LABELS[trigger],
        })),
        ...nodes.map((node) => ({
            trigger: "NODE_APPROVED" as const,
            nodeClientId: node.client_id,
            label: `${node.node_name || "未命名节点"}通过后`,
        })),
    ]
    return (
        <section
            className="space-y-3 rounded-lg border p-4"
            aria-label="抄送设置"
        >
            <div>
                <h3 className="font-medium text-sm">抄送设置</h3>
                <p className="mt-1 text-xs text-muted-foreground">
                    抄送人可查看本次提交资料，无需审批；未读不影响流程推进。
                </p>
            </div>
            {slots.map((slot) => {
                const key = `${slot.trigger}-${slot.nodeClientId ?? "process"}`
                const id = `approval-copy-rule-${toAutomationIdSegment(key)}`
                const rule = rules.find(
                    (r) =>
                        r.trigger === slot.trigger &&
                        r.nodeClientId === slot.nodeClientId,
                )
                const set = (next: EditorCopyRule | null) =>
                    onChange([
                        ...rules.filter((r) => r !== rule),
                        ...(next ? [next] : []),
                    ])
                return (
                    <div
                        key={key}
                        className="grid gap-2 border-t pt-3 sm:grid-cols-[180px_1fr]"
                    >
                        <span className="text-sm">{slot.label}</span>
                        <div className="space-y-2">
                            <div className="flex flex-wrap gap-2">
                                {rule?.recipients.map((person) => (
                                    <Button
                                        key={person.user_id}
                                        id={`${id}-remove-${toAutomationIdSegment(person.user_id)}`}
                                        type="button"
                                        variant="secondary"
                                        size="sm"
                                        disabled={disabled}
                                        onClick={() => {
                                            const recipients =
                                                rule.recipients.filter(
                                                    (p) =>
                                                        p.user_id !==
                                                        person.user_id,
                                                )
                                            set(
                                                recipients.length
                                                    ? { ...rule, recipients }
                                                    : null,
                                            )
                                        }}
                                    >
                                        {person.name} ×
                                    </Button>
                                ))}
                            </div>
                            <AssigneeCombobox
                                id={`${id}-add`}
                                copies
                                documentType={documentType}
                                value=""
                                selectedName=""
                                placeholder="添加抄送人"
                                excludedIds={rule?.recipients.map(
                                    (p) => p.user_id,
                                )}
                                disabled={
                                    disabled ||
                                    (rule?.recipients.length ?? 0) >= 50
                                }
                                onChange={(person) => {
                                    if (person)
                                        set({
                                            trigger: slot.trigger,
                                            nodeClientId: slot.nodeClientId,
                                            recipients: [
                                                ...(rule?.recipients ?? []),
                                                person,
                                            ],
                                        })
                                }}
                            />
                        </div>
                    </div>
                )
            })}
        </section>
    )
}
export function CopyRuleSummary({ detail }: { detail: DefinitionDetailView }) {
    return (
        <section
            className="space-y-2 rounded-lg border p-4"
            aria-label="抄送规则"
        >
            <h3 className="text-sm font-medium">抄送设置</h3>
            {detail.copy_rules?.length ? (
                detail.copy_rules.map((rule) => (
                    <p
                        className="text-sm"
                        key={`${rule.trigger}-${rule.node_order}`}
                    >
                        {rule.node_order
                            ? `${detail.nodes.find((n) => n.display_order === rule.node_order)?.node_name ?? "节点"}通过后`
                            : COPY_TRIGGER_LABELS[rule.trigger]}
                        ：
                        {rule.recipient_user_ids
                            .map(
                                (id) =>
                                    detail.copy_recipient_names?.[id] ??
                                    "待确认人员",
                            )
                            .join("、")}
                    </p>
                ))
            ) : (
                <p className="text-sm text-muted-foreground">未设置抄送</p>
            )}
        </section>
    )
}
