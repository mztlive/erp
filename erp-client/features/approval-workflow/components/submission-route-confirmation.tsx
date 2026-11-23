"use client"

import { Alert, AlertDescription, AlertTitle } from "@/components/ui/alert"

import { useLeaderPreview } from "../hooks/use-leader-preview"
import { getErrorMessage } from "@/lib/api"

import { displayRoute } from "../display"
import type { ApprovalDefinitionBinding } from "../types"

/**
 * 提交确认上的固定审批路线与驳回说明。
 *
 * 按单据政策说明驳回结果，不展示可选驳回目标。
 */
export function SubmissionRouteConfirmation({
    definition,
    fallbackDescription = "当前单据尚未展示审批路线，请刷新后再确认提交。",
}: {
    definition?: ApprovalDefinitionBinding
    documentType?: string
    fallbackDescription?: string
}) {
    const preview = useLeaderPreview(definition?.nodes ?? [])
    if (!definition || definition.nodes.length === 0) {
        return (
            <Alert>
                <AlertTitle>提交后将启动审批</AlertTitle>
                <AlertDescription>{fallbackDescription}</AlertDescription>
            </Alert>
        )
    }

    const route = displayRoute(preview.nodes)

    return (
        <Alert>
            <AlertTitle>提交后的审批路线</AlertTitle>
            <AlertDescription>
                <p>{route}</p>
                {preview.loading && <p>正在确认组织负责人…</p>}
                {preview.error && (
                    <p className="text-destructive">
                        {getErrorMessage(
                            preview.error,
                            "负责人查询失败，请重试",
                        )}
                    </p>
                )}
                {definition.nodes.some((node) => node.assigneeSource) && (
                    <p>
                        负责人以正式提交时的组织配置为准，审批开始后保持不变。
                    </p>
                )}
                <p className="mt-2">
                    任一节点驳回将结束本次审批，退回提交人处理。填写重新提交说明后可重新发起审批。
                </p>
            </AlertDescription>
        </Alert>
    )
}
