"use client"

import { useEffect, type ReactNode } from "react"
import {
    Handle,
    Position,
    useUpdateNodeInternals,
    type Node,
    type NodeProps,
} from "@xyflow/react"
import { UserRoundIcon, XIcon } from "lucide-react"
import { Button } from "@/components/ui/button"
import { cn } from "@/lib/utils"
import type { DocumentType, EditorNode } from "../types"
import type { CanvasKind, FlowDirection } from "./flow-canvas-model"
import { PersonPicker } from "./node-assignee-group"
import { FlowStepHeader } from "./flow-step-header"
import { toAutomationIdSegment } from "@/lib/automation-id"
import { nodePeople } from "./flow-canvas-model"
import type { NodeInsertion } from "./flow-node-insertion"

export type CanvasNode = Node<
    {
        domId: string
        canvasId: string
        onInsert: (placement: NodeInsertion) => boolean
        onRemove: (clientId: string) => boolean
        nodeLimitReached: boolean
        direction: FlowDirection
        documentType: DocumentType
        disabled: boolean
        onChange: (patch: Partial<EditorNode>) => void
        kind: CanvasKind
        ownerId?: string
        personId?: string
        node?: EditorNode
        order?: number
        editor?: ReactNode
        copyCount?: number
        onEdit?: () => void
        onCopies?: () => void
    },
    "approval" | "branch"
>

/** 连接点同时提供库内部身份与原生 DOM id。 */
export function FlowPort({
    id,
    type,
    enabled,
    direction,
    quiet = false,
}: {
    id: string
    type: "source" | "target"
    enabled: boolean
    direction: FlowDirection
    quiet?: boolean
}) {
    return (
        <Handle
            id={type}
            ref={(element) => {
                if (element) element.id = id
            }}
            type={type}
            position={
                direction === "horizontal"
                    ? type === "source"
                        ? Position.Right
                        : Position.Left
                    : type === "source"
                      ? Position.Bottom
                      : Position.Top
            }
            isConnectable={enabled}
            isConnectableStart={enabled}
            isConnectableEnd={enabled}
            className={cn(
                "!size-3 !border-2 !border-info-border !bg-card hover:!border-info",
                quiet &&
                    "!size-2 !border-0 !bg-info opacity-0 hover:opacity-100",
            )}
            title={
                type === "source"
                    ? "拖到下一节点的入口连接点"
                    : "连接上一审批节点"
            }
            aria-label={type === "source" ? "下一节点连接点" : "上一节点连接点"}
        />
    )
}

export function FlowCanvasNode({
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
    const people = node ? nodePeople(node) : []
    const person = people[0]
    return (
        <div
            className={cn(
                "relative rounded-xl border shadow-sm",
                node
                    ? "border-info-border bg-info-soft/40"
                    : "border-border bg-card",
                selected && "ring-2 ring-info/20",
            )}
        >
            {data.kind !== "start" && (
                <FlowPort
                    id={`${data.domId}-input`}
                    type="target"
                    enabled={isConnectable}
                    direction={data.direction}
                />
            )}
            {data.kind === "approval" && node ? (
                <>
                    <FlowStepHeader data={data} />
                    <div className="nodrag nopan space-y-2 p-4">
                        <p className="text-xs text-muted-foreground">
                            {node.assignee_source === "SPECIFIED"
                                ? "单人审批"
                                : "负责人审批"}
                        </p>
                        <div className="flex min-w-0 items-center rounded-lg border border-info-border bg-card text-info-soft-foreground shadow-sm">
                            {node.assignee_source === "SPECIFIED" ? (
                                <>
                                    <PersonPicker
                                        id={
                                            person
                                                ? `${data.domId}-assignees-person-${toAutomationIdSegment(person.user_id)}`
                                                : `${data.domId}-assignees-add`
                                        }
                                        documentType={data.documentType}
                                        people={people}
                                        person={person}
                                        disabled={data.disabled}
                                        onChoose={(next) => {
                                            data.onChange({
                                                assignee_user_id: next.user_id,
                                                assignee_name: next.name,
                                                additional_assignees: [],
                                            })
                                            requestAnimationFrame(() =>
                                                document
                                                    .getElementById(
                                                        `${data.domId}-assignees-person-${toAutomationIdSegment(next.user_id)}-trigger`,
                                                    )
                                                    ?.focus({
                                                        preventScroll: true,
                                                    }),
                                            )
                                        }}
                                    />
                                    {person && (
                                        <Button
                                            id={`${data.domId}-assignees-person-${toAutomationIdSegment(person.user_id)}-remove`}
                                            type="button"
                                            size="icon-xs"
                                            variant="ghost"
                                            className="mr-1 shrink-0"
                                            aria-label={`移除审批人${person.name}`}
                                            disabled={data.disabled}
                                            onClick={() => {
                                                data.onChange({
                                                    assignee_user_id: "",
                                                    assignee_name: "",
                                                    additional_assignees: [],
                                                })
                                                requestAnimationFrame(() =>
                                                    document
                                                        .getElementById(
                                                            `${data.domId}-assignees-add-trigger`,
                                                        )
                                                        ?.focus({
                                                            preventScroll: true,
                                                        }),
                                                )
                                            }}
                                        >
                                            <XIcon />
                                        </Button>
                                    )}
                                </>
                            ) : (
                                <span className="flex items-center gap-2 px-3 py-2 text-sm">
                                    <UserRoundIcon className="size-3.5 shrink-0" />
                                    {node.assignee_source ===
                                    "APPLICANT_TEAM_LEADER"
                                        ? "申请人的团队负责人"
                                        : "申请人的部门负责人"}
                                </span>
                            )}
                        </div>
                    </div>
                </>
            ) : (
                <div
                    id={`${data.domId}-drag`}
                    className="approval-node-drag cursor-grab px-5 py-3 text-center text-sm font-medium active:cursor-grabbing"
                >
                    {data.kind === "start" ? "提交申请" : "审批完成"}
                </div>
            )}
            {data.kind !== "end" && (
                <FlowPort
                    id={`${data.domId}-output`}
                    type="source"
                    enabled={isConnectable}
                    direction={data.direction}
                />
            )}
        </div>
    )
}
