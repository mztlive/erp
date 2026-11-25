"use client"

import * as React from "react"
import Link from "next/link"
import { ArrowRightLeftIcon } from "lucide-react"
import { useSearchParams } from "@/lib/navigation"
import {
    BusinessEmptyState,
    BusinessFailureState,
    DetailWorkspaceBody,
    DetailWorkspaceChrome,
    DetailWorkspaceMeta,
    DetailWorkspacePage,
    DetailWorkspaceSection,
    DetailWorkspaceSkeleton,
    DetailWorkspaceTabs,
    detailWorkspaceStyles,
    type DetailWorkspaceTabItem,
} from "@/components/business"
import { useWorkspaceInspectorMode } from "@/components/business/workspace-task-pane"
import { Button } from "@/components/ui/button"
import { WorkItemResponsibilityDialog } from "@/features/work-items"
import { useWorkItemDetailQuery } from "@/features/work-items/queries"
import type { PortalApplication } from "@/features/supplier-portal/types"
import { PortalApplicationContent } from "@/features/supplier-portal/components/application-content"
import { PortalError } from "@/features/supplier-portal/components/surface"
import {
    kindLabels,
    portalApplicationStatus,
    timeLabel,
} from "@/features/supplier-portal/lib/presentation"
import { usePortalReviewApplication } from "../hooks"
import { PortalReviewImages } from "../components/review-images"
import { PortalReviewDiff } from "../components/review-diff"
import { PortalOfferingImpacts } from "../components/offering-impacts"
import { PortalReviewForm } from "./review-form"
import { PortalReviewOverview } from "../components/review-overview"

type ReviewSection = "review" | "impacts" | "history"

function safeReturnTo(raw: string | null): string | null {
    if (
        !raw ||
        !raw.startsWith("/") ||
        raw.startsWith("//") ||
        raw.includes("\\")
    ) {
        return null
    }
    return raw
}

type ReviewPageProps = {
    applicationId: string
    workItemId?: string
    embedded?: boolean
    fallbackSupplier?: string
    onTaskCompleted?: (id: string) => void
}

export function PortalAdminReviewPage({ embedded, ...props }: ReviewPageProps) {
    return embedded ? (
        <EmbeddedPortalReview {...props} />
    ) : (
        <StandalonePortalReview {...props} />
    )
}

function StandalonePortalReview({
    applicationId,
    workItemId,
    onTaskCompleted,
}: ReviewPageProps) {
    const searchParams = useSearchParams()
    const {
        access,
        canRead: canReadDetail,
        query,
        application,
    } = usePortalReviewApplication(applicationId)
    const id =
        canReadDetail && access.can("work_item:detail")
            ? (workItemId ?? query.data?.work_item_id ?? "")
            : ""
    const task = useWorkItemDetailQuery(id)
    const taskDetail = task.data

    const [activeSection, setActiveSection] =
        React.useState<ReviewSection>("review")
    const [reassignOpen, setReassignOpen] = React.useState(false)
    const [decisionPending, setDecisionPending] = React.useState(false)

    const returnTo = safeReturnTo(searchParams.get("returnTo"))
    const fallbackHref =
        application?.supplier_id && access.can("supplier:detail")
            ? `/master-data/suppliers/${encodeURIComponent(application.supplier_id)}?section=portal-applications`
            : "/procurement/supplier-offerings"
    const backHref = returnTo ?? fallbackHref
    const backLabel = returnTo
        ? "返回来源页"
        : application?.supplier_id && access.can("supplier:detail")
          ? "返回供应商资料"
          : "返回供应商供货目录"

    const hasImpacts =
        Boolean(application) &&
        (application?.kind === "terms" || application?.kind === "stop") &&
        typeof application?.input.offering_id === "string"
    const offeringIdForImpacts = hasImpacts
        ? (application?.input.offering_id as string)
        : undefined

    const sectionItems = React.useMemo<DetailWorkspaceTabItem[]>(() => {
        if (!application) return []
        const items: DetailWorkspaceTabItem[] = [
            { id: "review", label: "审核与对比" },
        ]
        if (hasImpacts) {
            items.push({ id: "impacts", label: "关联影响" })
        }
        const historyCount =
            (application.decisions?.length ?? 0) +
            (application.submissions?.length ?? 0)
        items.push({
            id: "history",
            label: "提交与处理记录",
            badge:
                historyCount > 0 ? (
                    <span className="num text-xs text-muted-foreground">
                        {historyCount}
                    </span>
                ) : undefined,
        })
        return items
    }, [application, hasImpacts])

    const resolvedSection: ReviewSection =
        activeSection === "impacts" && !hasImpacts ? "review" : activeSection

    const applicationTitle =
        application &&
        (application.title ??
            (application.input.name as string | undefined) ??
            kindLabels[application.kind])

    const isLoading =
        access.profile.isPending || (canReadDetail && query.isPending)

    if (isLoading) {
        return (
            <DetailWorkspaceSkeleton
                title="供应商申请审核"
                description="正在读取申请资料…"
            />
        )
    }

    if (access.profile.isError) {
        return (
            <DetailWorkspacePage>
                <DetailWorkspaceChrome
                    title="供应商申请审核"
                    back={{
                        id: "supplier-portal-review-back",
                        label: backLabel,
                        href: backHref,
                        visibility: returnTo ? "always" : "standalone-only",
                    }}
                />
                <DetailWorkspaceBody>
                    <DetailWorkspaceSection width="form">
                        <BusinessFailureState
                            title="账号权限信息读取失败"
                            error={access.profile.error}
                            onRetry={() => {
                                void access.profile.refetch()
                            }}
                        />
                    </DetailWorkspaceSection>
                </DetailWorkspaceBody>
            </DetailWorkspacePage>
        )
    }

    if (!canReadDetail) {
        return (
            <DetailWorkspacePage>
                <DetailWorkspaceChrome
                    title="供应商申请审核"
                    back={{
                        id: "supplier-portal-review-back",
                        label: backLabel,
                        href: backHref,
                        visibility: returnTo ? "always" : "standalone-only",
                    }}
                />
                <DetailWorkspaceBody>
                    <DetailWorkspaceSection width="form">
                        <BusinessFailureState
                            kind="permission"
                            title="无权查看该供应商申请"
                            description="当前账号缺少供应商申请资料查看资格，请联系管理员配置或转交处理。"
                            action={
                                <Link
                                    id="supplier-portal-review-permission-back"
                                    href={backHref}
                                    className={
                                        detailWorkspaceStyles.inlineLinkButton
                                    }
                                >
                                    {backLabel}
                                </Link>
                            }
                        />
                    </DetailWorkspaceSection>
                </DetailWorkspaceBody>
            </DetailWorkspacePage>
        )
    }

    if (query.isError && !application) {
        return (
            <DetailWorkspacePage>
                <DetailWorkspaceChrome
                    title="供应商申请审核"
                    back={{
                        id: "supplier-portal-review-back",
                        label: backLabel,
                        href: backHref,
                        visibility: returnTo ? "always" : "standalone-only",
                    }}
                />
                <DetailWorkspaceBody>
                    <DetailWorkspaceSection width="form">
                        <BusinessFailureState
                            title="申请资料读取失败"
                            error={query.error}
                            onRetry={() => {
                                void query.refetch()
                            }}
                        />
                    </DetailWorkspaceSection>
                </DetailWorkspaceBody>
            </DetailWorkspacePage>
        )
    }

    if (!application) {
        return (
            <DetailWorkspacePage>
                <DetailWorkspaceChrome
                    title="供应商申请不存在"
                    back={{
                        id: "supplier-portal-review-back",
                        label: backLabel,
                        href: backHref,
                        visibility: returnTo ? "always" : "standalone-only",
                    }}
                />
                <DetailWorkspaceBody>
                    <DetailWorkspaceSection width="form">
                        <BusinessEmptyState
                            kind="no-data"
                            title="未找到该供应商申请"
                            description="该申请可能已被撤回、删除，或当前账号无权查看。"
                            action={
                                <Link
                                    id="supplier-portal-review-empty-back"
                                    href={backHref}
                                    className={
                                        detailWorkspaceStyles.inlineLinkButton
                                    }
                                >
                                    {backLabel}
                                </Link>
                            }
                        />
                    </DetailWorkspaceSection>
                </DetailWorkspaceBody>
            </DetailWorkspacePage>
        )
    }

    const bodyContent = (
        <>
            {resolvedSection === "review" ? (
                <>
                    <PortalReviewDiff
                        key={application.id}
                        application={application}
                    />
                    {application.kind === "new_product" ? (
                        <PortalReviewImages application={application} />
                    ) : null}
                    {application.status === "pending" &&
                    !access.can("work_item:detail") ? (
                        <DetailWorkspaceSection width="form">
                            <BusinessFailureState
                                kind="permission"
                                title="无法读取审核任务"
                            />
                        </DetailWorkspaceSection>
                    ) : null}
                    {task.error ? (
                        <DetailWorkspaceSection width="form">
                            <BusinessFailureState
                                title="审核任务读取失败"
                                error={task.error}
                                onRetry={() => void task.refetch()}
                            />
                        </DetailWorkspaceSection>
                    ) : null}
                    {taskDetail ? (
                        <PortalReviewForm
                            key={taskDetail.id}
                            application={application}
                            task={taskDetail}
                            onPendingChange={setDecisionPending}
                            onReload={async () => {
                                const results = await Promise.all([
                                    query.refetch(),
                                    task.refetch(),
                                ])
                                const failure = results.find(
                                    (result) => result.error,
                                )
                                if (failure?.error) throw failure.error
                            }}
                            onSuccess={() => {
                                void query.refetch()
                                void task.refetch()
                                onTaskCompleted?.(taskDetail.id)
                            }}
                        />
                    ) : null}
                    <UnlistedProductLink
                        application={application}
                        allowed={access.can("product:list")}
                    />
                </>
            ) : null}
            {resolvedSection === "impacts" && offeringIdForImpacts ? (
                <PortalOfferingImpacts
                    offeringId={offeringIdForImpacts}
                    enabled={canReadDetail}
                />
            ) : null}
            {resolvedSection === "history" ? (
                <PortalApplicationContent
                    application={application}
                    showDetails={application.kind === "new_product"}
                    showImages={false}
                    showHistory
                />
            ) : null}
        </>
    )

    const canHeaderReassign = Boolean(
        application?.status === "pending" &&
        access.can("work_item:manage") &&
        task.data?.status === "OPEN" &&
        task.data.allowed_actions?.includes("REASSIGN"),
    )

    return (
        <DetailWorkspacePage>
            <DetailWorkspaceChrome
                title={applicationTitle ?? "供应商申请审核"}
                documentNumber={application?.application_no}
                numberLabel="申请单号"
                back={{
                    id: "supplier-portal-review-back",
                    label: backLabel,
                    href: backHref,
                    visibility: returnTo ? "always" : "standalone-only",
                }}
                primaryStatus={
                    application
                        ? portalApplicationStatus(application.status)
                        : undefined
                }
                meta={
                    application ? (
                        <DetailWorkspaceMeta
                            items={[
                                application.supplier_name,
                                kindLabels[application.kind],
                                application.updated_at ? (
                                    <span key="updated" className="num">
                                        更新于{" "}
                                        {timeLabel(application.updated_at)}
                                    </span>
                                ) : null,
                            ]}
                        />
                    ) : undefined
                }
                secondaryActions={
                    canHeaderReassign ? (
                        <Button
                            id="supplier-portal-review-header-reassign"
                            type="button"
                            size="sm"
                            variant="outline"
                            disabled={decisionPending}
                            onClick={() => setReassignOpen(true)}
                        >
                            <ArrowRightLeftIcon
                                data-icon="inline-start"
                                aria-hidden="true"
                            />
                            转交他人
                        </Button>
                    ) : undefined
                }
                tabs={
                    sectionItems.length >= 2 ? (
                        <DetailWorkspaceTabs
                            value={resolvedSection}
                            onValueChange={(next) =>
                                setActiveSection(next as ReviewSection)
                            }
                            items={sectionItems}
                            ariaLabel="申请审核分区"
                            tabIdPrefix="supplier-portal-review-tab"
                        />
                    ) : undefined
                }
            />
            <DetailWorkspaceBody
                alerts={
                    query.error ? (
                        <PortalError
                            error={query.error}
                            retry={() => void query.refetch()}
                            id="supplier-portal-review-reload"
                        />
                    ) : undefined
                }
            >
                {bodyContent}
            </DetailWorkspaceBody>
            {canHeaderReassign && taskDetail ? (
                <WorkItemResponsibilityDialog
                    mode="reassign"
                    idPrefix="supplier-portal-reassign"
                    targets={[
                        {
                            workItemId: taskDetail.id,
                            label:
                                applicationTitle ??
                                application?.application_no ??
                                "供应商申请确认",
                            expectedTaskVersion: taskDetail.task_version,
                            currentOwnerLabel:
                                taskDetail.owner_user?.display_name ??
                                undefined,
                        },
                    ]}
                    open={reassignOpen}
                    onOpenChange={setReassignOpen}
                    onCompleted={(summary) => {
                        if (summary.succeededIds.length > 0) {
                            void query.refetch()
                            void task.refetch()
                        }
                    }}
                />
            ) : null}
        </DetailWorkspacePage>
    )
}

function UnlistedProductLink({
    application,
    allowed,
}: {
    application: PortalApplication
    allowed: boolean
}) {
    if (
        !allowed ||
        application.kind !== "new_product" ||
        application.status !== "effective"
    )
        return null
    return (
        <DetailWorkspaceSection width="form">
            <Link
                id="supplier-portal-review-unlisted-link"
                href="/master-data/products?productListingStatus=unlisted"
                className={detailWorkspaceStyles.inlineLinkButton}
            >
                进入商品库集中定价及上架
            </Link>
        </DetailWorkspaceSection>
    )
}

function EmbeddedPortalReview({
    applicationId,
    workItemId,
    fallbackSupplier,
    onTaskCompleted,
}: ReviewPageProps) {
    const inspectorMode = useWorkspaceInspectorMode()
    const { access, canRead, query, application } =
        usePortalReviewApplication(applicationId)
    const canReadTask =
        canRead && access.can("work_item:detail") && !inspectorMode
    const task = useWorkItemDetailQuery(
        canReadTask ? (workItemId ?? application?.work_item_id ?? "") : "",
    )

    const taskDetail = task.data

    if (access.profile.isPending || (canRead && query.isPending)) {
        return (
            <p role="status" className="py-4 text-xs text-muted-foreground">
                正在读取申请资料…
            </p>
        )
    }
    if (access.profile.isError || (!application && query.isError)) {
        return (
            <BusinessFailureState
                idPrefix="supplier-portal-workspace-review-failure"
                title={
                    access.profile.isError
                        ? "账号权限信息读取失败"
                        : "申请资料读取失败"
                }
                error={access.profile.error ?? query.error}
                onRetry={() =>
                    void (access.profile.isError
                        ? access.profile.refetch()
                        : query.refetch())
                }
            />
        )
    }
    if (!canRead) {
        return (
            <BusinessFailureState
                kind="permission"
                title="无权查看该供应商申请"
            />
        )
    }
    if (!application) {
        return <BusinessEmptyState kind="no-data" title="未找到该供应商申请" />
    }

    const details = (
        <PortalReviewDiff
            key={application.id}
            application={application}
            compact
        />
    )
    return (
        <div className="@container/supplier-portal-review min-w-0 space-y-3.5 bg-card py-3.5">
            <PortalError
                error={query.error}
                retry={() => void query.refetch()}
                id="supplier-portal-review-reload"
            />
            {canReadTask && taskDetail ? (
                <PortalReviewForm
                    key={taskDetail.id}
                    application={application}
                    task={taskDetail}
                    workspace={{ details, fallbackSupplier }}
                    onReload={async () => {
                        const results = await Promise.all([
                            query.refetch(),
                            task.refetch(),
                        ])
                        const failure = results.find((result) => result.error)
                        if (failure?.error) throw failure.error
                    }}
                    onSuccess={() => onTaskCompleted?.(taskDetail.id)}
                />
            ) : (
                <>
                    <PortalReviewOverview
                        application={application}
                        fallbackSupplier={fallbackSupplier}
                    />
                    {!inspectorMode && application.status === "pending" ? (
                        !canReadTask ? (
                            <BusinessFailureState
                                kind="permission"
                                title="无法读取审核任务"
                            />
                        ) : task.error ? (
                            <BusinessFailureState
                                idPrefix="supplier-portal-workspace-task-failure"
                                title="审核任务读取失败"
                                error={task.error}
                                onRetry={() => void task.refetch()}
                            />
                        ) : task.isPending ? (
                            <p
                                role="status"
                                className="text-xs text-muted-foreground"
                            >
                                正在读取审核任务…
                            </p>
                        ) : null
                    ) : null}
                    {!inspectorMode ? details : null}
                </>
            )}
            {!inspectorMode ? (
                <UnlistedProductLink
                    application={application}
                    allowed={access.can("product:list")}
                />
            ) : null}
        </div>
    )
}
