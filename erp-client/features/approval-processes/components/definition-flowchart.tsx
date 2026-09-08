"use client"

import { ChevronDownIcon, UserRoundIcon } from "lucide-react"

import { toAutomationIdSegment } from "@/lib/automation-id"
import { cn } from "@/lib/utils"

import type { EditorNode } from "../types"

/**
 * 线性审批流程图。纯展示，按 nodes 当前顺序渲染管道。
 *
 * 缺审批人的节点用警告态而不是置灰，避免看起来像禁用。
 */
export function DefinitionFlowchart({
    nodes,
    selectedClientId,
    onSelect,
    id = "governance-approval-processes-detail-editor-flow",
}: {
    nodes: readonly EditorNode[]
    selectedClientId?: string | null
    onSelect?: (clientId: string, index: number) => void
    id?: string
}) {
    return (
        <section
            aria-label="审批流程图"
            className="flex min-w-0 flex-col gap-4 rounded-xl border border-border bg-muted/40 p-4"
        >
            <div className="flex items-center justify-between gap-3">
                <h3 className="text-sm font-semibold">流程图</h3>
                <span className="num rounded-full bg-card px-2.5 py-0.5 text-xs text-muted-foreground ring-1 ring-foreground/10">
                    {nodes.length === 0
                        ? "暂无节点"
                        : `共 ${nodes.length} 个节点`}
                </span>
            </div>
            <ol className="flex flex-col items-stretch">
                <li className="flex justify-center">
                    <span
                        id={`${id}-start`}
                        className="rounded-full bg-primary px-4 py-1 text-xs font-medium text-primary-foreground"
                    >
                        开始
                    </span>
                </li>
                <FlowConnector />
                {nodes.length === 0 ? (
                    <li className="flex justify-center">
                        <span className="text-xs text-muted-foreground">
                            在左侧增加节点后，这里会生成流程图。
                        </span>
                    </li>
                ) : (
                    nodes.map((node, index) => {
                        const selected =
                            selectedClientId != null &&
                            node.client_id === selectedClientId
                        const segment = toAutomationIdSegment(node.client_id)
                        const title = node.node_name.trim() || "未命名节点"
                        const assignee = node.assignee_name.trim()
                        return (
                            <li
                                key={node.client_id}
                                className="flex flex-col items-stretch"
                            >
                                <button
                                    id={`${id}-node-${segment}`}
                                    type="button"
                                    aria-current={selected ? "true" : undefined}
                                    aria-label={`第 ${index + 1} 个节点：${title}，${assignee || "待指定审批人"}`}
                                    onClick={() =>
                                        onSelect?.(node.client_id, index)
                                    }
                                    className={cn(
                                        "w-full rounded-xl border bg-card p-3 text-left shadow-sm transition-colors hover:border-primary/50",
                                        selected
                                            ? "border-primary ring-2 ring-primary/40"
                                            : "border-border",
                                        !assignee &&
                                            !selected &&
                                            "border-warning-border bg-warning-soft/50",
                                    )}
                                >
                                    <span className="flex items-center gap-2.5">
                                        <span
                                            aria-hidden="true"
                                            className={cn(
                                                "num flex size-7 shrink-0 items-center justify-center rounded-full text-xs font-semibold",
                                                assignee
                                                    ? "bg-primary text-primary-foreground"
                                                    : "bg-warning-soft text-warning-soft-foreground ring-1 ring-warning-border",
                                            )}
                                        >
                                            {index + 1}
                                        </span>
                                        <span className="min-w-0 flex-1 truncate text-sm font-semibold">
                                            {title}
                                        </span>
                                    </span>
                                    <span
                                        className={cn(
                                            "mt-2 inline-flex items-center gap-1.5 text-xs",
                                            assignee
                                                ? "text-muted-foreground"
                                                : "font-medium text-warning-soft-foreground",
                                        )}
                                    >
                                        <UserRoundIcon
                                            aria-hidden="true"
                                            className="size-3.5 shrink-0"
                                        />
                                        <span className="min-w-0 flex-1 truncate">
                                            {assignee || "待指定审批人"}
                                        </span>
                                    </span>
                                </button>
                                <FlowConnector />
                            </li>
                        )
                    })
                )}
                <li className="flex justify-center">
                    <span
                        id={`${id}-end`}
                        className="rounded-full border border-success-border bg-success-soft px-4 py-1 text-xs font-medium text-success-soft-foreground"
                    >
                        结束
                    </span>
                </li>
            </ol>
            <p className="text-xs leading-5 text-muted-foreground">
                点击节点定位到左侧编辑卡。
            </p>
        </section>
    )
}

function FlowConnector() {
    return (
        <span
            aria-hidden="true"
            className="flex flex-col items-center py-1 text-primary/60"
        >
            <span className="h-3 w-0.5 rounded-full bg-primary/40" />
            <ChevronDownIcon className="size-4" strokeWidth={2.5} />
        </span>
    )
}
