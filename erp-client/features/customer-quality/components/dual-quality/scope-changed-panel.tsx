"use client"

import { BusinessFailureState } from "@/components/business"
import { Button } from "@/components/ui/button"

export function ScopeChangedPanel({
    idPrefix,
    error,
    onRefreshFromFirst,
    onRetry,
}: {
    idPrefix: string
    error: unknown
    onRefreshFromFirst: () => void
    onRetry: () => void
}) {
    return (
        <BusinessFailureState
            kind="conflict"
            title="数据范围已变化"
            error={error}
            action={
                <div className="flex flex-wrap gap-2">
                    <Button
                        id={`${idPrefix}-scope-changed-refresh`}
                        type="button"
                        size="sm"
                        onClick={onRefreshFromFirst}
                    >
                        从第一页刷新
                    </Button>
                    <Button
                        id={`${idPrefix}-scope-changed-retry`}
                        type="button"
                        size="sm"
                        variant="outline"
                        onClick={onRetry}
                    >
                        重试
                    </Button>
                </div>
            }
        />
    )
}
