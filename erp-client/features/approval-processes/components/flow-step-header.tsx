"use client"

import { useEffect, useRef, useState } from "react"
import { BellIcon, GripVerticalIcon, PencilIcon, XIcon } from "lucide-react"
import { Button } from "@/components/ui/button"
import {
    Popover,
    PopoverContent,
    PopoverTitle,
    PopoverTrigger,
} from "@/components/ui/popover"
import { toAutomationIdSegment } from "@/lib/automation-id"
import type { CanvasNode } from "./flow-canvas-node"
import { nodePeople, personNodeId } from "./flow-canvas-model"
import { PersonPicker } from "./node-assignee-group"

/** 单人、组织负责人和或签共用标题栏及画布内设置入口。 */
export function FlowStepHeader({ data }: { data: CanvasNode["data"] }) {
    const node = data.node
    const [settingsOpen, setSettingsOpen] = useState(() =>
        Boolean(
            data.editor && node && !node.node_name.trim() && !data.disabled,
        ),
    )
    const contentRef = useRef<HTMLDivElement>(null)
    useEffect(() => {
        if (!data.editor) setSettingsOpen(false)
    }, [data.editor])
    if (!node) return null
    const people = nodePeople(node)
    const specified = node.assignee_source === "SPECIFIED"
    return (
        <div
            id={`${data.domId}-drag`}
            className="approval-node-drag flex h-16 cursor-grab items-center gap-3 rounded-t-xl border-b border-info-border bg-info-soft px-4 active:cursor-grabbing"
        >
            <GripVerticalIcon className="size-4 shrink-0 text-info-soft-foreground" />
            <span className="num text-xs text-muted-foreground">
                {String(data.order).padStart(2, "0")}
            </span>
            <span className="min-w-0 flex-1 truncate text-sm font-medium">
                {node.node_name.trim() || "未命名节点"}
            </span>
            <span className="shrink-0 text-xs text-muted-foreground">
                {specified ? `${people.length} 人` : "组织负责人"}
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
                <Popover
                    open={settingsOpen && Boolean(data.editor)}
                    onOpenChange={setSettingsOpen}
                >
                    <PopoverTrigger
                        id={`${data.domId}-edit`}
                        render={
                            <Button
                                type="button"
                                size="icon-xs"
                                variant="ghost"
                            />
                        }
                        aria-label="编辑节点"
                        title="编辑节点"
                        onClick={() => data.onEdit?.()}
                    >
                        <PencilIcon />
                    </PopoverTrigger>
                    <PopoverContent
                        ref={contentRef}
                        className="nodrag nopan nowheel w-96 gap-0 p-0"
                        initialFocus={() =>
                            contentRef.current?.querySelector<HTMLInputElement>(
                                "input",
                            ) ?? false
                        }
                        onClick={(event) => event.stopPropagation()}
                        onKeyDown={(event) => event.stopPropagation()}
                    >
                        <div className="flex items-center justify-between px-4 pt-3">
                            <PopoverTitle className="text-sm">
                                节点设置
                            </PopoverTitle>
                            <Button
                                id={`${data.domId}-settings-close`}
                                type="button"
                                size="icon-xs"
                                variant="ghost"
                                aria-label="关闭节点设置"
                                onClick={() => setSettingsOpen(false)}
                            >
                                <XIcon />
                            </Button>
                        </div>
                        {data.editor}
                        <Button
                            id={`${data.domId}-copies`}
                            type="button"
                            variant="ghost"
                            className="m-3"
                            onClick={() => {
                                setSettingsOpen(false)
                                // 设置浮层关闭后，抄送返回焦点应落在仍存在的编辑按钮。
                                document
                                    .getElementById(`${data.domId}-edit`)
                                    ?.focus({ preventScroll: true })
                                data.onCopies?.()
                            }}
                        >
                            <BellIcon />
                            通过后抄送
                            {data.copyCount ? ` · ${data.copyCount} 人` : ""}
                        </Button>
                    </PopoverContent>
                </Popover>
            </div>
        </div>
    )
}
