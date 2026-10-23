"use client"

import { GripVerticalIcon, PencilIcon } from "lucide-react"
import { Button } from "@/components/ui/button"
import { toAutomationIdSegment } from "@/lib/automation-id"
import type { CanvasNode } from "./flow-canvas-node"
import { nodePeople, personNodeId } from "./flow-canvas-model"
import { PersonPicker } from "./node-assignee-group"

/** 单人、组织负责人和或签共用标题栏；节点内容统一在右侧面板编辑。 */
export function FlowStepHeader({ data }: { data: CanvasNode["data"] }) {
    const node = data.node
    if (!node) return null
    const people = nodePeople(node)
    const specified = node.assignee_source === "SPECIFIED"
    const modeCopy = !specified
        ? "组织负责人"
        : people.length === 0
          ? "待添加审批人"
          : people.length > 1
            ? "或签 · 任一人通过"
            : "单人审批"
    return (
        <div
            id={`${data.domId}-drag`}
            className="approval-node-drag flex h-16 cursor-grab items-center gap-2.5 rounded-t-xl border-b border-info-border bg-info-soft px-4 active:cursor-grabbing"
        >
            <GripVerticalIcon className="size-4 shrink-0 text-info-soft-foreground" />
            <span className="num text-xs text-muted-foreground">
                {String(data.order ?? 0).padStart(2, "0")}
            </span>
            <span className="min-w-0 flex-1 truncate text-sm font-medium">
                {node.node_name.trim() || "未命名节点"}
            </span>
            <span
                className="shrink-0 rounded-full bg-card/80 px-2 py-0.5 text-xs font-medium text-info-soft-foreground ring-1 ring-info-border"
                title={
                    specified
                        ? people.length > 1
                            ? "任一人通过后进入下一步"
                            : "由指定人员审批"
                        : "按申请人所属组织确定负责人"
                }
            >
                {modeCopy}
            </span>
            <div className="nodrag nopan flex shrink-0 items-center gap-0.5 text-muted-foreground">
                {specified && (
                    <PersonPicker
                        id={`${data.domId}-add`}
                        documentType={data.documentType}
                        people={people}
                        compact
                        disabled={data.disabled || people.length >= 50}
                        onChoose={(person) => {
                            const next = [...people, person]
                            data.onChange({
                                assignee_user_id: next[0].user_id,
                                assignee_name: next[0].name,
                                additional_assignees: next.slice(1),
                            })
                            const focusId =
                                next.length > 1
                                    ? `${data.canvasId}-node-${toAutomationIdSegment(personNodeId(node.client_id, person.user_id))}-person-trigger`
                                    : `${data.domId}-assignees-person-${toAutomationIdSegment(person.user_id)}-trigger`
                            requestAnimationFrame(() =>
                                document
                                    .getElementById(focusId)
                                    ?.focus({ preventScroll: true }),
                            )
                        }}
                    />
                )}
                <Button
                    id={`${data.domId}-edit`}
                    type="button"
                    size="icon-xs"
                    variant="ghost"
                    aria-label="编辑节点"
                    title="在右侧编辑节点"
                    onClick={() => data.onEdit?.()}
                >
                    <PencilIcon />
                </Button>
            </div>
        </div>
    )
}
