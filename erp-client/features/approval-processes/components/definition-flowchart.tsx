"use client"

import {
    ArrowDownIcon,
    BellIcon,
    CheckIcon,
    GitBranchIcon,
    PlusIcon,
    UserRoundIcon,
} from "lucide-react"
import { Button } from "@/components/ui/button"
import { toAutomationIdSegment } from "@/lib/automation-id"
import { cn } from "@/lib/utils"
import type { EditorCopyRule, EditorNode } from "../types"
import { isOrSign } from "./flow-canvas-model"
import { OrSignPreview } from "./or-sign-preview"

/** 共用线性流程画布；概览完整展示审批人，编辑模式提供节点选择和增加入口。 */
export function DefinitionFlowchart({
    nodes,
    selectedClientId,
    onSelect,
    onAdd,
    copyRules = [],
    onConfigureCopies,
    compact = false,
    overview = false,
    id = "governance-approval-processes-detail-editor-flow",
}: {
    nodes: readonly EditorNode[]
    selectedClientId?: string | null
    onSelect?: (clientId: string, index: number) => void
    onAdd?: () => void
    copyRules?: readonly EditorCopyRule[]
    onConfigureCopies?: (nodeClientId: string | null) => void
    compact?: boolean
    overview?: boolean
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
                <div className="flex flex-wrap items-center gap-3">
                    <span className="text-xs text-muted-foreground">
                        {nodes.length} 个节点 · 顺序审批
                    </span>
                    {onConfigureCopies && (
                        <Button
                            id={`${id}-copy-settings`}
                            type="button"
                            variant="outline"
                            size="sm"
                            onClick={() => onConfigureCopies(null)}
                        >
                            <BellIcon data-icon="inline-start" />
                            抄送设置
                            {copyRules.length ? ` · ${copyRules.length}` : ""}
                        </Button>
                    )}
                </div>
            </div>
            <div
                id={`${id}-canvas`}
                role="region"
                aria-label="审批流程画布"
                className={cn(
                    "overflow-y-auto px-4 py-8 focus-visible:outline-2 focus-visible:-outline-offset-2 focus-visible:outline-ring",
                    compact
                        ? "max-h-80"
                        : overview
                          ? "max-h-[65vh]"
                          : "max-h-[65vh] min-h-96",
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
                        const copyCount =
                            copyRules.find(
                                (rule) =>
                                    rule.trigger === "NODE_APPROVED" &&
                                    rule.nodeClientId === node.client_id,
                            )?.recipients.length ?? 0
                        const people = [
                            {
                                user_id: node.assignee_user_id,
                                name:
                                    node.assignee_name ||
                                    (overview && node.assignee_user_id
                                        ? "待确认人员"
                                        : ""),
                            },
                            ...node.additional_assignees.map((person) => ({
                                ...person,
                                name:
                                    person.name ||
                                    (overview ? "待确认人员" : ""),
                            })),
                        ].filter((person) => Boolean(person.name))
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
                                        {node.assignee_source !== "SPECIFIED"
                                            ? "组织负责人"
                                            : node.additional_assignees.length
                                              ? "或签"
                                              : overview
                                                ? "单人审批"
                                                : "单人"}
                                    </span>
                                </div>
                                {isOrSign(node) ? (
                                    <OrSignPreview node={node} />
                                ) : (
                                    <div className="space-y-2 px-4 py-3 text-xs text-muted-foreground">
                                        {node.assignee_source === "SPECIFIED" &&
                                        people.length > 0 ? (
                                            <div className="grid grid-cols-2 gap-2">
                                                {(overview
                                                    ? people
                                                    : people.slice(0, 4)
                                                ).map((person) => (
                                                    <span
                                                        key={person.user_id}
                                                        className={cn(
                                                            "flex min-w-0 items-center gap-2 rounded-md border px-3 py-2",
                                                            node
                                                                .additional_assignees
                                                                .length
                                                                ? "border-info-border bg-info-soft text-info-soft-foreground"
                                                                : "border-border bg-muted/40",
                                                        )}
                                                    >
                                                        <UserRoundIcon className="size-3.5 shrink-0" />
                                                        <span className="min-w-0 break-words">
                                                            {person.name}
                                                        </span>
                                                    </span>
                                                ))}
                                            </div>
                                        ) : (
                                            <span className="flex items-center gap-2 leading-5">
                                                <UserRoundIcon className="size-3.5 shrink-0" />
                                                {people
                                                    .map(
                                                        (person) => person.name,
                                                    )
                                                    .join("、") ||
                                                    "待设置审批人"}
                                            </span>
                                        )}
                                        {!overview && people.length > 4 && (
                                            <p>共 {people.length} 人</p>
                                        )}
                                    </div>
                                )}
                            </>
                        )
                        const cardClass = cn(
                            "w-full overflow-hidden rounded-lg border bg-card text-left shadow-sm transition-colors",
                            selected
                                ? "border-primary ring-2 ring-primary/15"
                                : (node.assignee_source === "SPECIFIED" &&
                                        !node.assignee_user_id) ||
                                    !node.node_name.trim()
                                  ? "border-warning-border"
                                  : "border-border",
                            onSelect && "hover:border-primary/50",
                        )
                        return (
                            <li
                                key={node.client_id}
                                className="flex w-full flex-col items-center"
                            >
                                <div className={cardClass}>
                                    {onSelect ? (
                                        <button
                                            id={`${id}-node-${toAutomationIdSegment(node.client_id)}`}
                                            type="button"
                                            aria-pressed={selected}
                                            aria-label={`第 ${index + 1} 个节点：${node.node_name || "未命名节点"}`}
                                            className="block w-full text-left focus-visible:outline-2 focus-visible:-outline-offset-2 focus-visible:outline-ring"
                                            onClick={() =>
                                                onSelect(node.client_id, index)
                                            }
                                        >
                                            {content}
                                        </button>
                                    ) : (
                                        <div>{content}</div>
                                    )}
                                    {copyCount > 0 && onConfigureCopies && (
                                        <button
                                            id={`${id}-node-${toAutomationIdSegment(node.client_id)}-copies`}
                                            type="button"
                                            className="flex w-full items-center gap-2 border-t border-border/60 bg-muted/30 px-4 py-2.5 text-left text-xs text-muted-foreground hover:bg-accent hover:text-foreground focus-visible:outline-2 focus-visible:-outline-offset-2 focus-visible:outline-ring"
                                            aria-label={`${node.node_name.trim() || `第 ${index + 1} 个节点`}通过后抄送 ${copyCount} 人，设置抄送`}
                                            onClick={() =>
                                                onConfigureCopies(
                                                    node.client_id,
                                                )
                                            }
                                        >
                                            <BellIcon className="size-3.5 shrink-0" />
                                            通过后抄送 {copyCount} 人
                                        </button>
                                    )}
                                </div>
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
