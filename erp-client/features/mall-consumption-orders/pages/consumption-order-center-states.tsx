"use client"

import Link from "next/link"

import {
    BusinessEmptyState,
    BusinessFailureState,
    PageScaffold,
} from "@/components/business"
import { Button } from "@/components/ui/button"

export function CenterPagePendingState() {
    return (
        <PageScaffold>
            <div className="space-y-3" aria-busy="true" aria-label="加载中">
                <div className="h-10 w-56 animate-pulse rounded-lg bg-muted" />
                <div className="h-24 animate-pulse rounded-lg bg-muted" />
                <div className="h-96 animate-pulse rounded-lg bg-muted" />
            </div>
        </PageScaffold>
    )
}

export function CenterPageErrorState({
    error,
    backToListHref,
    onRetry,
}: {
    error: unknown
    backToListHref: string
    onRetry: () => void
}) {
    return (
        <PageScaffold>
            <BusinessFailureState
                title="加载失败，请重试"
                error={error}
                action={
                    <div className="flex flex-wrap gap-2">
                        <Button
                            type="button"
                            variant="secondary"
                            className="rounded-lg shadow-none"
                            onClick={onRetry}
                        >
                            重试
                        </Button>
                        <Button
                            type="button"
                            variant="outline"
                            render={<Link href={backToListHref} />}
                        >
                            返回列表
                        </Button>
                    </div>
                }
            />
        </PageScaffold>
    )
}

export function CenterPageEmptyState() {
    return (
        <PageScaffold>
            <BusinessEmptyState
                kind="no-data"
                title="未找到消费订单"
                description="该消费订单不存在或当前账号无权访问。"
                action={
                    <Button
                        type="button"
                        variant="secondary"
                        className="rounded-lg shadow-none"
                        render={<Link href="/commerce/consumption-orders" />}
                    >
                        返回列表
                    </Button>
                }
            />
        </PageScaffold>
    )
}
