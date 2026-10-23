"use client"

import {
    ChevronDownIcon,
    ChevronUpIcon,
    GripVerticalIcon,
    Trash2Icon,
} from "lucide-react"
import { OptionCombobox } from "@/components/business/option-combobox"
import { Button } from "@/components/ui/button"
import { Input } from "@/components/ui/input"
import { Label } from "@/components/ui/label"
import { toAutomationIdSegment } from "@/lib/automation-id"
import type { AssigneeSource, EditorNode } from "../types"

/** 在流程图选中节点内编辑，沿用父级表单的节点状态。 */
export function NodeInlineEditor({
    nodes,
    readOnly,
    onChange,
    node,
    id = "governance-approval-processes-detail-editor-nodes",
}: {
    nodes: EditorNode[]
    readOnly: boolean
    onChange: (nodes: EditorNode[]) => void
    node: EditorNode
    id?: string
}) {
    const index = nodes.findIndex((item) => item.client_id === node.client_id)
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
        <div
            role="group"
            aria-label={`第 ${index + 1} 个节点设置`}
            data-testid={`approval-node-${index}`}
        >
            <div
                id={`${id}-node-${segment}-drag`}
                className="approval-node-drag flex cursor-grab items-center justify-between gap-2 border-b border-border px-4 py-3 active:cursor-grabbing"
            >
                <h3 className="flex items-center gap-3 text-sm font-medium">
                    <GripVerticalIcon className="size-4 shrink-0 text-muted-foreground" />
                    <span className="num flex size-7 shrink-0 items-center justify-center rounded-md bg-primary text-xs text-primary-foreground">
                        {String(index + 1).padStart(2, "0")}
                    </span>
                    编辑节点
                </h3>
                <div className="nodrag flex items-center gap-1">
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
            <div className="nodrag nowheel space-y-5 p-4">
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
                {node.assignee_source !== "SPECIFIED" && (
                    <p className="text-xs leading-5 text-muted-foreground">
                        提交时按申请人所属组织确定负责人；请先在“组织与人员”中配置负责人。审批开始后人员保持不变。
                    </p>
                )}
            </div>
        </div>
    )
}
