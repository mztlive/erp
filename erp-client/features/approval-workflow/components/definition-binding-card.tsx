"use client"

import { Card, CardContent, CardHeader, CardTitle } from "@/components/ui/card"

import { displayProcessVersion } from "../display"
import type { ApprovalDefinitionBinding } from "../types"

/**
 * 未提交单据上的只读绑定卡。
 *
 * 只展示流程名、版本和有序节点/审批人，不提供选择定义、增删节点或换人。
 */
export function DefinitionBindingCard({
    definition,
    emptyLabel = "尚未绑定审批流程",
}: {
    definition?: ApprovalDefinitionBinding
    emptyLabel?: string
}) {
    if (!definition) {
        return (
            <Card size="sm">
                <CardHeader>
                    <CardTitle>审批流程</CardTitle>
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
                <CardTitle>
                    {displayProcessVersion({
                        name: definition.name,
                        version: definition.version,
                    })}
                </CardTitle>
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
