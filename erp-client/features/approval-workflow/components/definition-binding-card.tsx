"use client"

import type * as React from "react"

import { Card, CardContent, CardHeader, CardTitle } from "@/components/ui/card"

import { displayProcessVersion } from "../display"
import type { ApprovalDefinitionBinding } from "../types"

function BindingHeading({
    compact,
    children,
}: {
    compact?: boolean
    children: React.ReactNode
}) {
    if (compact) {
        return <h2 className="text-sm font-medium">{children}</h2>
    }
    return <CardTitle>{children}</CardTitle>
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
                    <BindingHeading compact={compact}>审批流程</BindingHeading>
                </CardHeader>
                <CardContent className="text-sm text-muted-foreground">
                    {emptyLabel}
                </CardContent>
            </Card>
        )
    }

    return (
        <Card size="sm">
            <CardHeader>
                <BindingHeading compact={compact}>
                    {displayProcessVersion({
                        name: definition.name,
                        version: definition.version,
                    })}
                </BindingHeading>
            </CardHeader>
            <CardContent>
                {definition.nodes.length === 0 ? (
                    <p className="text-sm text-muted-foreground">
                        当前流程尚未配置审批节点
                    </p>
                ) : (
                    <ol className="space-y-2 text-sm">
                        {definition.nodes.map((node, index) => (
                            <li
                                key={node.key}
                                className="flex items-baseline gap-2"
                            >
                                <span className="text-muted-foreground">
                                    {index + 1}.
                                </span>
                                <span>{node.name}</span>
                                {node.assigneeName ? (
                                    <span className="text-muted-foreground">
                                        {node.assigneeName}
                                    </span>
                                ) : null}
                            </li>
                        ))}
                    </ol>
                )}
            </CardContent>
        </Card>
    )
}
