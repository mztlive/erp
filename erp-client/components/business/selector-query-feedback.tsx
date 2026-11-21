"use client"

import { Button } from "@/components/ui/button"
import { getErrorMessage, isPermissionDeniedError } from "@/lib/api/errors"

/** 候选状态独立于业务列表；有已选值时仍显示失败和重试入口。 */
export function SelectorQueryFeedback({
    id,
    failed,
    error,
    noScope,
    noScopeLabel = "暂无该资料的查看权限，请联系管理员开通",
    onRetry,
}: {
    id?: string
    failed: boolean
    error?: unknown
    noScope?: boolean
    noScopeLabel?: string
    onRetry: () => void
}) {
    if ((!failed && !noScope) || isPermissionDeniedError(error)) return null
    return (
        <div
            role={failed ? "alert" : "status"}
            className="text-body-compact text-muted-foreground"
        >
            <span>
                {failed
                    ? getErrorMessage(error, "候选加载失败，请重试")
                    : noScopeLabel}
            </span>
            <Button
                id={id ? `${id}-retry` : undefined}
                type="button"
                size="sm"
                variant="ghost"
                onClick={onRetry}
            >
                重试
            </Button>
        </div>
    )
}
