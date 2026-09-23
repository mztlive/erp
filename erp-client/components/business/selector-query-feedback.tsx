"use client"

import { Button } from "@/components/ui/button"
import { getErrorMessage } from "@/lib/api/errors"

/** 候选状态独立于业务列表；有已选值时仍显示失败和重试入口。 */
export function SelectorQueryFeedback({
    id,
    failed,
    error,
    noScope,
    onRetry,
}: {
    id?: string
    failed: boolean
    error?: unknown
    noScope?: boolean
    onRetry: () => void
}) {
    if (!failed && !noScope) return null
    return (
        <div role={failed ? "alert" : "status"} className="text-sm text-muted-foreground">
            <span>
                {failed
                    ? getErrorMessage(error, "候选加载失败，请重试")
                    : "当前角色无此目录的数据范围，请申请权限"}
            </span>
            <Button id={id ? `${id}-retry` : undefined} type="button" size="sm" variant="ghost" onClick={onRetry}>
                重试
            </Button>
        </div>
    )
}
