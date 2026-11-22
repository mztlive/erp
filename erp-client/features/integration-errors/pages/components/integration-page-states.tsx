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
    backHref,
    backAlways = false,
}: {
    title: string
    description: string
    error: Error | null
    onRetry: () => void
    id?: string
    idPrefix?: string
    detail?: boolean
    backHref?: string
    backAlways?: boolean
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
        <IntegrationDetailState
            title={title}
            description={description}
            backHref={backHref}
            backAlways={backAlways}
        >
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
    title = "外部同步异常",
    view,
    queueContextId,
    backHref,
    backAlways = false,
    onRetry,
    detail = false,
}: {
    title?: string
    detail?: boolean
    view: IntegrationView
    queueContextId: string
    backHref?: string
    backAlways?: boolean
    onRetry: () => void
}) {
    const resolvedBackHref =
        backHref ??
        `/governance/integration-errors?view=${view}&queueContextId=${encodeURIComponent(queueContextId)}`
    const description =
        title === "对账差异" ? "未找到该对账差异" : "未找到该异常任务"
    const empty = (
        <BusinessEmptyState
            kind="no-data"
            title={description}
            description="记录可能已结束或链接失效；可返回队列重新选择。"
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
                        render={<Link href={resolvedBackHref} />}
                    >
                        返回队列
                    </Button>
                </div>
            }
        />
    )
    return detail ? (
        <IntegrationDetailState
            title={title}
            description={description}
            backHref={resolvedBackHref}
            backAlways={backAlways}
        >
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
    backHref,
    backAlways = false,
    children,
}: {
    title: string
    description: string
    backHref?: string
    backAlways?: boolean
    children: React.ReactNode
}) {
    return (
        <DetailWorkspacePage>
            <DetailWorkspaceChrome
                title={title}
                meta={description}
                back={
                    backHref
                        ? {
                              id: "integration-detail-nav-back",
                              label: "返回队列",
                              href: backHref,
                              visibility: backAlways
                                  ? "always"
                                  : "standalone-only",
                          }
                        : undefined
                }
            />
            <DetailWorkspaceBody>
                <DetailWorkspaceSection width="form">
                    {children}
                </DetailWorkspaceSection>
            </DetailWorkspaceBody>
        </DetailWorkspacePage>
    )
}
