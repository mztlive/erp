"use client"

import { Alert, AlertDescription, AlertTitle } from "@/components/ui/alert"

import { displayRoute } from "../display"
import type { ApprovalDefinitionBinding } from "../types"

/**
 * 提交确认上的固定审批路线与驳回说明。
 *
 * 驳回只表达「从第一节点开始下一轮」，不展示可选驳回目标。
 */
export function SubmissionRouteConfirmation({
    definition,
}: {
    definition?: ApprovalDefinitionBinding
}) {
    if (!definition || definition.nodes.length === 0) {
        return (
            <Alert>
                <AlertTitle>提交后将启动审批</AlertTitle>
                <AlertDescription>
                    当前单据尚未展示审批路线，请刷新后再确认提交。
                </AlertDescription>
            </Alert>
        )
    }

    const firstNode = definition.nodes[0]
    const route = displayRoute(definition.nodes)
    const firstLabel = firstNode.assigneeName?.trim() || firstNode.name

    return (
        <Alert>
            <AlertTitle>提交后的审批路线</AlertTitle>
            <AlertDescription>
                <p>{route}</p>
                <p className="mt-2">
                    任一层驳回后，将从{firstLabel}开始下一轮审批。
                </p>
            </AlertDescription>
        </Alert>
    )
}
