"use client"

import { useEffect } from "react"
import { useUpdateNodeInternals, type NodeProps } from "@xyflow/react"
import { GripVerticalIcon, GitForkIcon, XIcon } from "lucide-react"
import { Button } from "@/components/ui/button"
import { cn } from "@/lib/utils"
import { toAutomationIdSegment } from "@/lib/automation-id"
import type { EligibleAssignee } from "../types"
import { nodePeople, personNodeId } from "./flow-canvas-model"
import { FlowPort, type CanvasNode } from "./flow-canvas-node"
import { FlowStepHeader } from "./flow-step-header"
import { PersonPicker } from "./node-assignee-group"

/** 或签分流、人员分支及汇合点均为独立 React Flow 节点。 */
export function FlowBranchNode({
    id,
    data,
    selected,
    isConnectable,
}: NodeProps<CanvasNode>) {
    const updateInternals = useUpdateNodeInternals()
    useEffect(() => {
        updateInternals(id)
    }, [id, data.direction, updateInternals])
    const node = data.node
    if (!node) return null
    const people = nodePeople(node)
    const person = people.find((item) => item.user_id === data.personId)
    const update = (
        next: EligibleAssignee[],
        focusPerson = next[0]?.user_id,
    ) => {
        if (data.disabled) return
        data.onChange({
            assignee_user_id: next[0]?.user_id ?? "",
            assignee_name: next[0]?.name ?? "",
            additional_assignees: next.slice(1),
        })
        const ownerDomId = `${data.canvasId}-node-${toAutomationIdSegment(node.client_id)}`
        const focusId =
            next.length > 1 && focusPerson
                ? `${data.canvasId}-node-${toAutomationIdSegment(personNodeId(node.client_id, focusPerson))}-person-trigger`
                : focusPerson
                  ? `${ownerDomId}-assignees-person-${toAutomationIdSegment(focusPerson)}-trigger`
                  : `${ownerDomId}-assignees-add-trigger`
        requestAnimationFrame(() =>
            document.getElementById(focusId)?.focus({ preventScroll: true }),
        )
    }
    const input = (
        <FlowPort
            id={`${data.domId}-input`}
            type="target"
            enabled={isConnectable && data.kind === "split"}
            direction={data.direction}
            quiet={data.kind === "join"}
        />
    )
    const output = (
        <FlowPort
            id={`${data.domId}-output`}
            type="source"
            enabled={isConnectable && data.kind === "join"}
            direction={data.direction}
            quiet={data.kind === "join"}
        />
    )
    if (data.kind === "join")
        return (
            <div className="relative size-2 rounded-full bg-info">
                {input}
                {output}
            </div>
        )
    if (data.kind === "split")
        return (
            <div className="flex h-10 items-center justify-center gap-2 rounded-lg border border-info-border bg-card text-sm font-medium text-info-soft-foreground shadow-sm">
                {input}
                <GitForkIcon className="size-4" />
                或签
                {output}
            </div>
        )
    if (data.kind === "person" && person)
        return (
            <div
                className={cn(
                    "rounded-lg border border-info-border bg-card text-info-soft-foreground shadow-sm",
                    selected && "ring-2 ring-primary/30",
                )}
            >
                {input}
                <div
                    id={`${data.domId}-drag`}
                    className="approval-node-drag flex cursor-grab items-center gap-2 border-b border-info-border px-3 py-2 text-xs"
                >
                    <GripVerticalIcon className="size-3.5" />
                    <span className="flex-1">审批人</span>
                    <Button
                        id={`${data.domId}-remove`}
                        type="button"
                        size="icon-xs"
                        variant="ghost"
                        className="nodrag"
                        disabled={data.disabled}
                        aria-label={`移除审批人${person.name}`}
                        onClick={() =>
                            update(
                                people.filter(
                                    (item) => item.user_id !== person.user_id,
                                ),
                            )
                        }
                    >
                        <XIcon />
                    </Button>
                </div>
                <div className="nodrag nopan flex p-1">
                    <PersonPicker
                        id={`${data.domId}-person`}
                        documentType={data.documentType}
                        people={people}
                        person={person}
                        disabled={data.disabled}
                        onChoose={(replacement) =>
                            update(
                                people.map((item) =>
                                    item.user_id === person.user_id
                                        ? replacement
                                        : item,
                                ),
                                replacement.user_id,
                            )
                        }
                    />
                </div>
                {output}
            </div>
        )
    return (
        <div
            className={cn(
                "relative h-full rounded-xl border border-info-border bg-info-soft/40 shadow-sm",
                selected && "ring-2 ring-info/20",
            )}
        >
            <FlowStepHeader data={data} />
        </div>
    )
}
