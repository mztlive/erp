"use client"

import {
    ChevronDownIcon,
    ChevronUpIcon,
    Trash2Icon,
    UsersRoundIcon,
    XIcon,
} from "lucide-react"
import { OptionCombobox } from "@/components/business/option-combobox"
import { Button } from "@/components/ui/button"
import { Input } from "@/components/ui/input"
import { Label } from "@/components/ui/label"
import { toAutomationIdSegment } from "@/lib/automation-id"
import type { AssigneeSource, DocumentType, EditorNode } from "../types"
import { AssigneeCombobox } from "./assignee-combobox"

/** 一次编辑一个节点，顺序与选择由流程图呈现。 */
export function NodeListEditor({
    documentType,
    nodes,
    readOnly,
    onChange,
    selectedClientId,
    id = "governance-approval-processes-detail-editor-nodes",
}: {
    documentType: DocumentType
    nodes: EditorNode[]
    readOnly: boolean
    onChange: (nodes: EditorNode[]) => void
    selectedClientId?: string | null
    id?: string
}) {
    const index = Math.max(
        0,
        nodes.findIndex((node) => node.client_id === selectedClientId),
    )
    const node = nodes[index]
    if (!node)
        return (
            <aside className="rounded-lg border border-dashed border-border p-6 text-center text-sm text-muted-foreground">
                添加节点后，在这里设置名称和审批人。
            </aside>
        )
    const segment = toAutomationIdSegment(node.client_id)
    const update = (patch: Partial<EditorNode>) =>
        onChange(
            nodes.map((item) =>
                item.client_id === node.client_id
                    ? { ...item, ...patch }
                    : item,
            ),
        )
    const move = (offset: number) => {
        const next = [...nodes]
        const target = index + offset
        if (!next[target]) return
        ;[next[index], next[target]] = [next[target], node]
        onChange(next)
    }
    return (
        <aside
            aria-label="节点设置"
            className="overflow-hidden rounded-lg border border-border bg-card"
            data-testid={`approval-node-${index}`}
        >
            <div className="flex items-center justify-between gap-2 border-b border-border px-4 py-3">
                <h3 className="text-sm font-medium">
                    节点设置{" "}
                    <span className="num ml-2 text-xs text-muted-foreground">
                        {index + 1} / {nodes.length}
                    </span>
                </h3>
                <div className="flex items-center gap-1">
                    <Button
                        id={`${id}-node-${segment}-move-up`}
                        type="button"
                        size="icon-sm"
                        variant="ghost"
                        aria-label="上移节点"
                        title="上移节点"
                        disabled={readOnly || index === 0}
                        onClick={() => move(-1)}
                    >
                        <ChevronUpIcon />
                    </Button>
                    <Button
                        id={`${id}-node-${segment}-move-down`}
                        type="button"
                        size="icon-sm"
                        variant="ghost"
                        aria-label="下移节点"
                        title="下移节点"
                        disabled={readOnly || index === nodes.length - 1}
                        onClick={() => move(1)}
                    >
                        <ChevronDownIcon />
                    </Button>
                    <Button
                        id={`${id}-node-${segment}-remove`}
                        type="button"
                        size="icon-sm"
                        variant="ghost"
                        className="text-muted-foreground hover:text-destructive"
                        aria-label="删除节点"
                        title="删除节点"
                        disabled={readOnly}
                        onClick={() =>
                            onChange(
                                nodes.filter(
                                    (item) => item.client_id !== node.client_id,
                                ),
                            )
                        }
                    >
                        <Trash2Icon />
                    </Button>
                </div>
            </div>
            <div className="space-y-5 p-4">
                <div className="space-y-2">
                    <Label htmlFor={`${id}-node-${segment}-name`}>
                        节点名称 <span className="text-destructive">*</span>
                    </Label>
                    <Input
                        id={`${id}-node-${segment}-name`}
                        value={node.node_name}
                        maxLength={64}
                        disabled={readOnly}
                        placeholder="例如：财务复核"
                        onChange={(event) =>
                            update({ node_name: event.target.value })
                        }
                    />
                </div>
                <div className="space-y-2">
                    <Label htmlFor={`${id}-node-${segment}-source`}>
                        审批人来源
                    </Label>
                    <OptionCombobox
                        id={`${id}-node-${segment}-source`}
                        disabled={readOnly}
                        value={node.assignee_source}
                        allowClear={false}
                        options={[
                            { value: "SPECIFIED", label: "指定人员" },
                            {
                                value: "APPLICANT_TEAM_LEADER",
                                label: "申请人的团队负责人",
                            },
                            {
                                value: "APPLICANT_DEPARTMENT_LEADER",
                                label: "申请人的部门负责人",
                            },
                        ]}
                        onValueChange={(value) => {
                            if (!value || value === node.assignee_source) return
                            const source = value as AssigneeSource
                            update({
                                assignee_source: source,
                                assignee_user_id: "",
                                additional_assignees: [],
                                assignee_name:
                                    source === "APPLICANT_TEAM_LEADER"
                                        ? "申请人的团队负责人"
                                        : source ===
                                            "APPLICANT_DEPARTMENT_LEADER"
                                          ? "申请人的部门负责人"
                                          : "",
                            })
                        }}
                    />
                </div>
                {node.assignee_source === "SPECIFIED" ? (
                    <div className="space-y-2">
                        <Label
                            htmlFor={`${id}-node-${segment}-assignee-trigger`}
                        >
                            审批人 <span className="text-destructive">*</span>
                        </Label>
                        <AssigneeCombobox
                            id={`${id}-node-${segment}-assignee`}
                            documentType={documentType}
                            value={node.assignee_user_id}
                            selectedName={node.assignee_name}
                            excludedIds={node.additional_assignees.map(
                                (person) => person.user_id,
                            )}
                            disabled={readOnly}
                            onChange={(person) =>
                                update({
                                    assignee_user_id: person?.user_id ?? "",
                                    assignee_name: person?.name ?? "",
                                })
                            }
                        />
                        {node.additional_assignees.length > 0 && (
                            <div className="flex flex-wrap gap-2 py-1">
                                {node.additional_assignees.map((person) => (
                                    <span
                                        key={person.user_id}
                                        className="inline-flex max-w-full items-center gap-1 rounded-md border border-border bg-muted/40 py-1 pr-1 pl-2 text-xs"
                                    >
                                        <span className="break-all">
                                            {person.name}
                                        </span>
                                        <Button
                                            id={`${id}-node-${segment}-assignee-${toAutomationIdSegment(person.user_id)}-remove`}
                                            type="button"
                                            size="icon-xs"
                                            variant="ghost"
                                            aria-label={`移除审批人${person.name}`}
                                            disabled={readOnly}
                                            onClick={() =>
                                                update({
                                                    additional_assignees:
                                                        node.additional_assignees.filter(
                                                            (item) =>
                                                                item.user_id !==
                                                                person.user_id,
                                                        ),
                                                })
                                            }
                                        >
                                            <XIcon />
                                        </Button>
                                    </span>
                                ))}
                            </div>
                        )}
                        {!readOnly && (
                            <AssigneeCombobox
                                id={`${id}-node-${segment}-add-assignee`}
                                documentType={documentType}
                                value=""
                                selectedName=""
                                placeholder="添加或签审批人"
                                disabled={
                                    !node.assignee_user_id ||
                                    node.additional_assignees.length >= 49
                                }
                                excludedIds={[
                                    node.assignee_user_id,
                                    ...node.additional_assignees.map(
                                        (person) => person.user_id,
                                    ),
                                ]}
                                onChange={(person) => {
                                    if (person)
                                        update({
                                            additional_assignees: [
                                                ...node.additional_assignees,
                                                person,
                                            ],
                                        })
                                }}
                            />
                        )}
                    </div>
                ) : (
                    <p className="text-xs leading-5 text-muted-foreground">
                        提交时按申请人所属组织确定负责人；请先在“组织与人员”中配置负责人。审批开始后人员保持不变。
                    </p>
                )}
                {node.assignee_source === "SPECIFIED" && (
                    <div className="flex items-start gap-2 rounded-md bg-muted/50 p-3">
                        <UsersRoundIcon className="mt-0.5 size-4 shrink-0 text-muted-foreground" />
                        <div className="space-y-1">
                            <p className="text-xs font-medium">
                                {node.additional_assignees.length
                                    ? `或签 · ${node.additional_assignees.length + 1} 人`
                                    : "单人审批"}
                            </p>
                            <p className="text-xs leading-5 text-muted-foreground">
                                {node.additional_assignees.length
                                    ? "任一人通过即进入下一步，其余人的待办自动关闭。"
                                    : "添加多位审批人后，任一人通过即可。"}
                            </p>
                        </div>
                    </div>
                )}
            </div>
        </aside>
    )
}
