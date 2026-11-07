"use client"

import { Card, CardContent, CardHeader } from "@/components/ui/card"
import { ApprovalSectionHeading } from "./section-heading"

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
 * 未提交单据上的只读绑定卡。
 *
 * 只展示流程名、版本和有序节点/审批人，不提供选择定义、增删节点或换人。
 */
export function DefinitionBindingCard({
    definition,
    emptyLabel = "尚未绑定审批流程",
    compact = false,
}: {
    definition?: ApprovalDefinitionBinding
    emptyLabel?: string
    /** 对象中心 tab 内使用：标题与概览 text-sm 对齐。 */
    compact?: boolean
}) {
    if (!definition) {
        return (
            <Card size="sm">
                <CardHeader>
                    <ApprovalSectionHeading compact={compact}>
                        审批流程
                    </ApprovalSectionHeading>
                </CardHeader>
                <CardContent className="text-sm text-muted-foreground">
                    {emptyLabel}
                </CardContent>
            </Card>
        )
    }

    const nodes = routeNodes(definition)

    return (
        <Card size="sm">
            <CardHeader>
                <ApprovalSectionHeading compact={compact}>
                    {displayProcessVersion({
                        name: definition.name,
                        version: definition.version,
                        id: definition.id,
                    })}
                </ApprovalSectionHeading>
            </CardHeader>
            <CardContent>
                {nodes.length === 0 ? (
                    <p className="text-sm text-muted-foreground">
                        当前流程尚未配置审批节点
                    </p>
                ) : (
                    <ol className="space-y-2 text-sm">
                        {nodes.map((node, index) => {
                            const assigneeName = displayActorName(
                                node.assigneeName,
                            )
                            return (
                                <li
                                    key={node.key}
                                    className="flex items-baseline gap-2"
                                >
                                    <span className="text-muted-foreground">
                                        {index + 1}.
                                    </span>
                                    <span>
                                        {displayReadableName(
                                            node.name,
                                            node.key,
                                        ) ?? "审批节点未标注"}
                                    </span>
                                    {assigneeName ? (
                                        <span className="text-muted-foreground">
                                            {assigneeName}
                                        </span>
                                    ) : null}
                                </li>
                            )
                        })}
                    </ol>
                )}
            </CardContent>
        </Card>
    )
}
