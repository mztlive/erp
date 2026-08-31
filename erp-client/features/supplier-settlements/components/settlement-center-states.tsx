"use client"

import { ArrowLeftIcon } from "lucide-react"

import {
    BusinessEmptyState,
    BusinessFailureState,
    PageScaffold,
} from "@/components/business"
import { Button } from "@/components/ui/button"

function SettlementCenterLoading() {
    return (
        <PageScaffold>
            <div className="h-10 w-56 animate-pulse rounded-lg bg-muted" />
            <div className="h-24 animate-pulse rounded-lg bg-muted" />
            <div className="h-96 animate-pulse rounded-lg bg-muted" />
            <p className="text-sm text-muted-foreground">
                正在加载结算单，请稍候…
            </p>
        </PageScaffold>
    )
}

function SettlementCenterError({
    error,
    onBack,
    onRetry,
}: {
    error: unknown
    onBack: () => void
    onRetry: () => void
}) {
    return (
        <PageScaffold>
            <Button
                id="supplier-settlements-center-error-back"
                type="button"
                variant="ghost"
                size="sm"
                onClick={onBack}
            >
                <ArrowLeftIcon className="size-4" />
                返回列表
            </Button>
            <BusinessFailureState
                title="结算单加载失败"
                error={error}
                action={
                    <Button
                        id="supplier-settlements-center-error-retry"
                        type="button"
                        onClick={onRetry}
                    >
                        重试
                    </Button>
                }
            />
        </PageScaffold>
    )
}

function SettlementCenterEmpty({ onBack }: { onBack: () => void }) {
    return (
        <PageScaffold>
            <Button
                id="supplier-settlements-center-empty-back"
                type="button"
                variant="ghost"
                size="sm"
                onClick={onBack}
            >
                <ArrowLeftIcon className="size-4" />
                返回列表
            </Button>
            <BusinessEmptyState
                kind="no-data"
                className="rounded-lg border-0 bg-transparent p-6 shadow-none ring-0"
                title="结算单不存在"
                description="该结算单不存在或已被作废。可返回列表重新选择，或检查分享链接是否正确。"
            />
        </PageScaffold>
    )
}

export { SettlementCenterEmpty, SettlementCenterError, SettlementCenterLoading }
