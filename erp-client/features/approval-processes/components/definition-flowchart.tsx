"use client"

import {
    ArrowDownIcon,
    CheckIcon,
    GitBranchIcon,
    PlusIcon,
    UserRoundIcon,
} from "lucide-react"
import { Button } from "@/components/ui/button"
import { toAutomationIdSegment } from "@/lib/automation-id"
import { cn } from "@/lib/utils"
import type { EditorNode } from "../types"

/** 共用线性流程画布；只在编辑模式下提供节点选择和增加入口。 */
export function DefinitionFlowchart({
    nodes,
    selectedClientId,
    onSelect,
    onAdd,
    compact = false,
    id = "governance-approval-processes-detail-editor-flow",
}: {
    nodes: readonly EditorNode[]
    selectedClientId?: string | null
    onSelect?: (clientId: string, index: number) => void
    onAdd?: () => void
    compact?: boolean
    id?: string
}) {
    return (
        <section
            aria-label="审批流程图"
            className="min-w-0 overflow-hidden rounded-lg border border-border bg-muted/30"
        >
            <div className="flex flex-wrap items-center justify-between gap-2 border-b border-border bg-card px-4 py-3">
                <h3 className="flex items-center gap-2 text-sm font-medium">
                    <GitBranchIcon className="size-4 text-muted-foreground" />
                    审批流程
                </h3>
                <span className="text-xs text-muted-foreground">
                    {nodes.length} 个节点 · 顺序审批
                </span>
            </div>
            <div
                id={`${id}-canvas`}
                role="region"
                aria-label="审批流程画布"
                className={cn(
                    "overflow-y-auto px-4 py-8 focus-visible:outline-2 focus-visible:-outline-offset-2 focus-visible:outline-ring",
                    compact ? "max-h-80" : "max-h-[65vh] min-h-96",
                )}
            >
                <ol
                    className="mx-auto flex w-full max-w-sm flex-col items-center"
                    aria-label="审批节点顺序"
                >
                    <li className="flex w-full flex-col items-center">
                        <span className="inline-flex items-center gap-2 rounded-full border border-border bg-card px-4 py-2 text-xs font-medium">
                            <span className="size-2 rounded-full bg-primary" />
                            提交申请
                        </span>
                        <FlowConnector />
                    </li>
                    {nodes.map((node, index) => {
                        const selected = node.client_id === selectedClientId
                        const people = [
                            node.assignee_name,
                            ...node.additional_assignees.map(
                                (person) => person.name,
                            ),
                        ].filter(Boolean)
                        const content = (
                            <>
                                <div className="flex items-center gap-3 border-b border-border/60 px-4 py-3">
                                    <span
                                        className={cn(
                                            "num flex size-7 shrink-0 items-center justify-center rounded-md text-xs font-medium",
                                            selected
                                                ? "bg-primary text-primary-foreground"
                                                : "bg-accent text-accent-foreground",
                                        )}
                                    >
                                        {String(index + 1).padStart(2, "0")}
                                    </span>
                                    <span className="min-w-0 flex-1 break-words text-sm font-medium">
                                        {node.node_name.trim() || "未命名节点"}
                                    </span>
                                    <span className="shrink-0 rounded bg-muted px-2 py-0.5 text-xs text-muted-foreground">
                                        {node.additional_assignees.length
                                            ? "或签"
                                            : "单人"}
                                    </span>
                                </div>
                                <div className="flex items-start gap-2 px-4 py-3 text-xs text-muted-foreground">
                                    <UserRoundIcon className="mt-0.5 size-3.5 shrink-0" />
                                    <span
                                        className={cn(
                                            "min-w-0 break-words leading-5",
                                            !node.assignee_user_id &&
                                                "text-warning-soft-foreground",
                                        )}
                                    >
                                        {people.length
                                            ? people.slice(0, 4).join("、")
                                            : "待设置审批人"}
                                        {people.length > 4
                                            ? ` 等 ${people.length} 人`
                                            : ""}
                                    </span>
                                </div>
                            </>
                        )
                        const cardClass = cn(
                            "w-full rounded-lg border bg-card text-left shadow-sm transition-colors",
                            selected
                                ? "border-primary ring-2 ring-primary/15"
                                : !node.assignee_user_id ||
                                    !node.node_name.trim()
                                  ? "border-warning-border"
                                  : "border-border",
                            onSelect &&
                                "hover:border-primary/50 focus-visible:outline-2 focus-visible:outline-offset-4 focus-visible:outline-ring",
                        )
                        return (
                            <li
                                key={node.client_id}
                                className="flex w-full flex-col items-center"
                            >
                                {onSelect ? (
                                    <button
                                        id={`${id}-node-${toAutomationIdSegment(node.client_id)}`}
                                        type="button"
                                        aria-pressed={selected}
                                        aria-label={`第 ${index + 1} 个节点：${node.node_name || "未命名节点"}`}
                                        className={cardClass}
                                        onClick={() =>
                                            onSelect(node.client_id, index)
                                        }
                                    >
                                        {content}
                                    </button>
                                ) : (
                                    <div className={cardClass}>{content}</div>
                                )}
                                <FlowConnector />
                            </li>
                        )
                    })}
                    {nodes.length === 0 && (
                        <li className="mb-6 w-full rounded-lg border border-dashed border-border px-4 py-6 text-center text-sm text-muted-foreground">
                            {onAdd
                                ? "添加第一个审批节点，或从其它流程复制。"
                                : "此版本尚未配置审批节点"}
                        </li>
                    )}
                    {onAdd && (
                        <li className="flex flex-col items-center">
                            <Button
                                id={`${id}-add-node`}
                                type="button"
                                variant="outline"
                                size="sm"
                                onClick={onAdd}
                            >
                                <PlusIcon data-icon="inline-start" />
                                添加节点
                            </Button>
                            <FlowConnector />
                        </li>
                    )}
                    <li>
                        <span className="inline-flex items-center gap-2 rounded-full border border-border bg-card px-4 py-2 text-xs font-medium">
                            <CheckIcon className="size-3.5 text-muted-foreground" />
                            审批完成
                        </span>
                    </li>
                </ol>
            </div>
        </section>
    )
}

function FlowConnector() {
    return (
        <span
            aria-hidden="true"
            className="flex h-9 flex-col items-center text-border"
        >
            <span className="h-6 w-px bg-border" />
            <ArrowDownIcon className="-mt-1 size-3.5" />
        </span>
    )
}
