import Link from "next/link"
import {
    BusinessEmptyState,
    BusinessFailureState,
    PageHeader,
    PageScaffold,
} from "@/components/business"
import { Button } from "@/components/ui/button"
import type { IntegrationView } from "../../types"

export function IntegrationPageSkeleton({ focus }: { focus?: boolean }) {
    return (
        <PageScaffold>
            <div className="h-10 w-56 animate-pulse rounded-lg bg-muted" />
            <div className="h-16 animate-pulse rounded-lg bg-muted" />
            {focus ? (
                <div className="h-80 animate-pulse rounded-lg bg-muted" />
            ) : (
                <div className="grid gap-4 xl:grid-cols-[minmax(0,38fr)_minmax(0,62fr)]">
                    <div className="h-80 animate-pulse rounded-lg bg-muted" />
                    <div className="h-80 animate-pulse rounded-lg bg-muted" />
                </div>
            )}
        </PageScaffold>
    )
}

export function IntegrationPageFailure({
    title,
    description,
    error,
    onRetry,
    id,
    idPrefix,
}: {
    title: string
    description: string
    error: Error | null
    onRetry: () => void
    id?: string
    idPrefix?: string
}) {
    return (
        <PageScaffold>
            <PageHeader title={title} description={description} />
            <BusinessFailureState
                id={id}
                idPrefix={idPrefix ?? "integration-page-failure"}
                error={error}
                onRetry={onRetry}
            />
        </PageScaffold>
    )
}

export function IntegrationNotFound({
    view,
    queueContextId,
    onRetry,
}: {
    view: IntegrationView
    queueContextId: string
    onRetry: () => void
}) {
    return (
        <PageScaffold>
            <PageHeader title="接口错误与对账中心" description="未找到该任务" />
            <BusinessEmptyState
                kind="no-data"
                title="未找到该任务或差异"
                description="任务可能已结束或链接失效；可返回队列重新选择。"
                className="rounded-lg border-0 bg-transparent shadow-none ring-0"
                action={
                    <div className="flex flex-wrap gap-2">
                        <Button
                            id="integration-detail-not-found-retry"
                            type="button"
                            variant="secondary"
                            className="rounded-lg shadow-none"
                            onClick={onRetry}
                        >
                            重试
                        </Button>
                        <Button
                            id="integration-detail-not-found-back"
                            type="button"
                            variant="secondary"
                            className="rounded-lg shadow-none"
                            render={
                                <Link
                                    href={`/governance/integration-errors?view=${view}&queueContextId=${encodeURIComponent(queueContextId)}`}
                                />
                            }
                        >
                            返回队列
                        </Button>
                    </div>
                }
            />
        </PageScaffold>
    )
}
