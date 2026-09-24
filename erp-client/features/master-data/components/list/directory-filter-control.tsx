"use client"

import type * as React from "react"

import { SelectorQueryFeedback } from "@/components/business/selector-query-feedback"
import { cn } from "@/lib/utils"

/** 目录错误和重试独立于业务列表；按钮 id 由反馈组件从控件 id 派生。 */
export function DirectoryFilterControl({
    id,
    className,
    unavailable,
    noScope,
    failed,
    error,
    onRetry,
    children,
}: {
    id: string
    className?: string
    unavailable: boolean
    noScope: boolean
    failed: boolean
    error?: unknown
    onRetry: () => void
    children: React.ReactNode
}) {
    return (
        <div className={cn("min-w-0", className)}>
            {children}
            <SelectorQueryFeedback
                id={id}
                failed={failed && !unavailable}
                error={error}
                noScope={!failed && !unavailable && noScope}
                onRetry={onRetry}
            />
        </div>
    )
}
