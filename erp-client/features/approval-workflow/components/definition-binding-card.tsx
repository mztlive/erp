"use client"

import { cn } from "@/lib/utils"

import {
    displayActorName,
    displayProcessVersion,
    displayReadableName,
} from "../display"
import type {
    ApprovalDefinitionBinding,
    ApprovalDefinitionNode,
} from "../types"

function routeNodes(
    definition: ApprovalDefinitionBinding,
): readonly ApprovalDefinitionNode[] {
    return definition.nodes.length > 0
        ? definition.nodes
        : definition.publishedNodes
}

/**
 * 未提交单据上的只读绑定摘要。
 *
 * 只展示流程名、版本和有序节点/审批人，不提供选择定义、增删节点或换人。
 */
export function DefinitionBindingCard({
    definition,
    emptyLabel = "尚未绑定审批流程",
    compact = false,
    variant = "card",
}: {
    definition?: ApprovalDefinitionBinding
    emptyLabel?: string
    /** 对象中心 tab 或侧边抽屉内使用：标题与正文紧凑字号对齐。 */
    compact?: boolean
    /** 兼容旧调用参数；当前统一渲染无阴影扁平表面。 */
    variant?: "card" | "flat"
}) {
    void variant

    const headingClassName = cn(
        "font-semibold tracking-tight text-foreground",
        compact ? "text-body-compact" : "text-section-title",
    )

    if (!definition) {
        return (
            <section data-slot="approval-surface" className="space-y-2">
                <h2 className={headingClassName}>审批流程</h2>
                <p className="text-body-compact text-muted-foreground">
                    {emptyLabel}
                </p>
            </section>
        )
    }

    const nodes = routeNodes(definition)

    return (
        <section data-slot="approval-surface" className="space-y-3">
            <h2 className={headingClassName}>
                {displayProcessVersion({
                    name: definition.name,
                    version: definition.version,
                    id: definition.id,
                })}
            </h2>
            {nodes.length === 0 ? (
                <p className="text-xs text-muted-foreground">
                    当前流程尚未配置审批节点
                </p>
            ) : (
                <ol className="divide-y divide-border/60 border-t border-border/60 text-body-compact">
                    {nodes.map((node, index) => {
                        const assigneeName = displayActorName(node.assigneeName)
                        return (
                            <li
                                key={node.key}
                                className="flex flex-wrap items-baseline justify-between gap-2 py-2"
                            >
                                <span className="flex items-baseline gap-2">
                                    <span className="num text-xs text-muted-foreground">
                                        {index + 1}.
                                    </span>
                                    <span className="text-foreground">
                                        {displayReadableName(
                                            node.name,
                                            node.key,
                                        ) ?? "审批节点未标注"}
                                    </span>
                                </span>
                                {assigneeName ? (
                                    <span className="text-xs text-muted-foreground">
                                        {assigneeName}
                                    </span>
                                ) : null}
                            </li>
                        )
                    })}
                </ol>
            )}
        </section>
    )
}
