import Link from "next/link"
import {
    BusinessEmptyState,
    BusinessFailureState,
    DetailWorkspaceBody,
    DetailWorkspaceChrome,
    DetailWorkspacePage,
    DetailWorkspaceSection,
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
    detail = false,
}: {
    title: string
    description: string
    error: Error | null
    onRetry: () => void
    id?: string
    idPrefix?: string
    detail?: boolean
}) {
    const failure = (
        <BusinessFailureState
            id={id}
            idPrefix={idPrefix ?? "integration-page-failure"}
            error={error}
            onRetry={onRetry}
        />
    )
    return detail ? (
        <IntegrationDetailState title={title} description={description}>
            {failure}
        </IntegrationDetailState>
    ) : (
        <PageScaffold>
            <PageHeader title={title} description={description} />
            {failure}
        </PageScaffold>
    )
}

export function IntegrationNotFound({
    view,
    queueContextId,
    onRetry,
    detail = false,
}: {
    detail?: boolean
    view: IntegrationView
    queueContextId: string
    onRetry: () => void
}) {
    const title = "外部同步异常与对账"
    const description = "未找到该任务"
    const empty = (
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
    )
    return detail ? (
        <IntegrationDetailState title={title} description={description}>
            {empty}
        </IntegrationDetailState>
    ) : (
        <PageScaffold>
            <PageHeader title={title} description={description} />
            {empty}
        </PageScaffold>
    )
}

function IntegrationDetailState({
    title,
    description,
    children,
}: {
    title: string
    description: string
    children: React.ReactNode
}) {
    return (
        <DetailWorkspacePage>
            <DetailWorkspaceChrome title={title} meta={description} />
            <DetailWorkspaceBody>
                <DetailWorkspaceSection width="form">
                    {children}
                </DetailWorkspaceSection>
            </DetailWorkspaceBody>
        </DetailWorkspacePage>
    )
}
