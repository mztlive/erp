"use client"

import { GitBranchIcon } from "lucide-react"

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
    compact = false,
}: {
    definition?: ApprovalDefinitionBinding
    documentType?: string
    fallbackDescription?: string
    compact?: boolean
}) {
    const preview = useLeaderPreview(definition?.nodes ?? [])
    if (!definition || definition.nodes.length === 0) {
        if (compact) {
            return (
                <div className="flex items-start gap-2 text-xs">
                    <GitBranchIcon
                        aria-hidden="true"
                        className="mt-0.5 size-3.5 shrink-0 text-primary"
                    />
                    <div className="min-w-0 space-y-0.5">
                        <span className="font-medium text-foreground">
                            提交后将启动审批
                        </span>
                        <span className="mx-1.5 text-muted-foreground/50">
                            ·
                        </span>
                        <span className="text-muted-foreground">
                            {fallbackDescription}
                        </span>
                    </div>
                </div>
            )
        }
        return (
            <Alert>
                <AlertTitle>提交后将启动审批</AlertTitle>
                <AlertDescription>{fallbackDescription}</AlertDescription>
            </Alert>
        )
    }

    const route = displayRoute(preview.nodes)

    if (compact) {
        return (
            <div className="flex items-start gap-2 text-xs">
                <GitBranchIcon
                    aria-hidden="true"
                    className="mt-0.5 size-3.5 shrink-0 text-primary"
                />
                <div className="min-w-0 flex-1 space-y-0.5">
                    <div className="flex flex-wrap items-baseline gap-x-2 gap-y-0.5">
                        <span className="font-medium text-foreground">
                            审批路线
                        </span>
                        <span className="text-foreground/90">{route}</span>
                        {preview.loading ? (
                            <span className="text-muted-foreground">
                                正在确认组织负责人…
                            </span>
                        ) : null}
                        {preview.error ? (
                            <span className="text-destructive">
                                {getErrorMessage(
                                    preview.error,
                                    "负责人查询失败，请重试",
                                )}
                            </span>
                        ) : null}
                    </div>
                    <p className="text-muted-foreground">
                        {definition.nodes.some((node) => node.assigneeSource)
                            ? "负责人以提交时组织配置为准；"
                            : null}
                        任一节点驳回将退回提交人处理，填写说明后可重新提交。
                    </p>
                </div>
            </div>
        )
    }

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
