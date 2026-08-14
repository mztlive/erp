"use client"

import { PageHeader, PageScaffold } from "@/components/business"
import { Alert, AlertDescription, AlertTitle } from "@/components/ui/alert"
import { Button } from "@/components/ui/button"

export function MallSyncPageLoading() {
    return (
        <PageScaffold>
            <div className="h-10 w-56 animate-pulse rounded-lg bg-muted" />
            <div className="h-16 animate-pulse rounded-lg bg-muted" />
            <div className="h-24 animate-pulse rounded-lg bg-muted" />
            <div className="grid gap-4 lg:grid-cols-2">
                <div className="h-72 animate-pulse rounded-lg bg-muted" />
                <div className="h-72 animate-pulse rounded-lg bg-muted" />
            </div>
        </PageScaffold>
    )
}

type MallSyncPageErrorProps = {
    message: string
    onRetry: () => void
}

export function MallSyncPageError({
    message,
    onRetry,
}: MallSyncPageErrorProps) {
    return (
        <PageScaffold>
            <PageHeader title="商城同步与映射" description="加载失败" />
            <Alert variant="destructive">
                <AlertTitle>查询失败</AlertTitle>
                <AlertDescription>{message}</AlertDescription>
            </Alert>
            <Button
                type="button"
                variant="secondary"
                className="rounded-lg shadow-none"
                onClick={onRetry}
            >
                重试
            </Button>
        </PageScaffold>
    )
}
