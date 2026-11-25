"use client"

import { Button } from "@/components/ui/button"
import { getErrorMessage, isPermissionDeniedError } from "@/lib/api/errors"
import { cn } from "@/lib/utils"

/** 候选状态独立于业务列表；有已选值时仍显示失败和重试入口，无数据范围时隐藏重试按钮。 */
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
    onRetry?: () => void
}) {
    if ((!failed && !noScope) || isPermissionDeniedError(error)) return null
    return (
        <div
            role={failed ? "alert" : "status"}
            className={cn(
                "flex flex-wrap items-center gap-1.5 text-xs",
                failed ? "text-destructive" : "text-muted-foreground",
            )}
        >
            <span>
                {failed
                    ? getErrorMessage(error, "候选加载失败，请重试")
                    : noScopeLabel}
            </span>
            {failed && onRetry ? (
                <Button
                    id={id ? `${id}-retry` : undefined}
                    type="button"
                    size="xs"
                    variant="ghost"
                    className="text-destructive hover:bg-destructive-soft hover:text-destructive-soft-foreground"
                    onClick={onRetry}
                >
                    重试
                </Button>
            ) : null}
        </div>
    )
}
