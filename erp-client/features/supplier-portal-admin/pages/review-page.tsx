"use client"

import * as React from "react"
import Link from "next/link"
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
import { Button } from "@/components/ui/button"
import { LoadingButton } from "@/components/ui/loading-button"
import { useWorkItemDetailQuery } from "@/features/work-items/queries"
import { PortalApplicationContent } from "@/features/supplier-portal/components/application-content"
import { PortalError } from "@/features/supplier-portal/components/surface"
import {
    kindLabels,
    portalApplicationStatus,
    timeLabel,
} from "@/features/supplier-portal/lib/presentation"
import { usePortalAdminAccess, usePortalAdminApplication } from "../hooks"
import { PortalReviewImages } from "../components/review-images"
import { PortalReviewDiff } from "../components/review-diff"
import { PortalOfferingImpacts } from "../components/offering-impacts"
import {
    PortalReviewForm,
    type PortalReviewActionState,
    type PortalReviewFormHandle,
} from "./review-form"

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

export function PortalAdminReviewPage({
    applicationId,
    workItemId,
    embedded = false,
    onTaskCompleted,
}: {
    applicationId: string
    workItemId?: string
    embedded?: boolean
    onTaskCompleted?: (id: string) => void
}) {
    const searchParams = useSearchParams()
    const access = usePortalAdminAccess()
    const canReadDetail = access.can("supplier_portal_request:detail")
    const query = usePortalAdminApplication(applicationId, canReadDetail)
    const id =
        canReadDetail && access.can("work_item:detail")
            ? (workItemId ?? query.data?.work_item_id ?? "")
            : ""
    const task = useWorkItemDetailQuery(id)
    const application = canReadDetail ? query.data : undefined

    const [activeSection, setActiveSection] =
        React.useState<ReviewSection>("review")
    const [actionState, setActionState] =
        React.useState<PortalReviewActionState>({
            canProcess: false,
            pending: false,
            conflict: false,
            retryDecision: null,
        })
    const reviewFormRef = React.useRef<PortalReviewFormHandle>(null)

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

    const revealReviewTab = React.useCallback(() => {
        setActiveSection("review")
    }, [])

    const applicationTitle =
        application &&
        (application.title ??
            (application.input.name as string | undefined) ??
            kindLabels[application.kind])

    const isLoading =
        access.profile.isPending || (canReadDetail && query.isPending)

    if (!embedded && isLoading) {
        return (
            <DetailWorkspaceSkeleton
                title="供应商申请审核"
                description="正在读取申请资料…"
            />
        )
    }

    if (!embedded && access.profile.isError) {
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

    if (!embedded && !canReadDetail) {
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

    if (!embedded && query.isError && !application) {
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

    if (!embedded && !application) {
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
            {!canReadDetail && !access.profile.isPending ? (
                <DetailWorkspaceSection width="form">
                    <BusinessFailureState
                        kind="permission"
                        title="无权查看该供应商申请"
                        description="当前账号缺少供应商申请资料查看资格，请联系管理员配置或转交处理。"
                    />
                </DetailWorkspaceSection>
            ) : isLoading ? (
                <DetailWorkspaceSection width="form">
                    <p
                        role="status"
                        className="text-body-compact text-muted-foreground"
                    >
                        正在读取申请资料…
                    </p>
                </DetailWorkspaceSection>
            ) : !application ? (
                <DetailWorkspaceSection width="form">
                    <BusinessEmptyState
                        kind="no-data"
                        title="未找到该供应商申请"
                        description="该申请可能已被撤回、删除，或当前账号无权查看。"
                    />
                </DetailWorkspaceSection>
            ) : (
                <>
                    {embedded && sectionItems.length >= 2 ? (
                        <div className="border-b border-border/60">
                            <DetailWorkspaceTabs
                                value={resolvedSection}
                                onValueChange={(next) =>
                                    setActiveSection(next as ReviewSection)
                                }
                                items={sectionItems}
                                ariaLabel="申请审核分区"
                                tabIdPrefix="supplier-portal-review-tab"
                            />
                        </div>
                    ) : null}

                    {resolvedSection === "review" ? (
                        <>
                            <PortalReviewDiff application={application} />
                            {application.kind === "new_product" ? (
                                <PortalReviewImages application={application} />
                            ) : null}
                            {application.status === "pending" &&
                            !access.can("work_item:detail") ? (
                                <DetailWorkspaceSection width="form">
                                    <BusinessFailureState
                                        kind="permission"
                                        title="无法读取审核任务"
                                        description="当前账号缺少审核任务读取资格，请联系管理员配置或转交处理。"
                                    />
                                </DetailWorkspaceSection>
                            ) : null}
                            {application.status === "pending" ? (
                                <>
                                    {task.error ? (
                                        <DetailWorkspaceSection width="form">
                                            <BusinessFailureState
                                                title="审核任务读取失败"
                                                error={task.error}
                                                onRetry={() => {
                                                    void task.refetch()
                                                }}
                                            />
                                        </DetailWorkspaceSection>
                                    ) : null}
                                    {access.can("work_item:detail") &&
                                    task.data ? (
                                        <PortalReviewForm
                                            key={application.id}
                                            application={application}
                                            task={task.data}
                                            formRef={reviewFormRef}
                                            onActionStateChange={setActionState}
                                            onRevealReviewTab={revealReviewTab}
                                            onReload={async () => {
                                                const results =
                                                    await Promise.all([
                                                        query.refetch(),
                                                        task.refetch(),
                                                    ])
                                                const failed = results.find(
                                                    (result) => result.error,
                                                )
                                                if (failed?.error)
                                                    throw failed.error
                                            }}
                                            onSuccess={() => {
                                                void query.refetch()
                                                void task.refetch()
                                                onTaskCompleted?.(
                                                    task.data?.id ?? "",
                                                )
                                            }}
                                        />
                                    ) : null}
                                </>
                            ) : null}
                            {access.can("product:list") &&
                            application.kind === "new_product" &&
                            application.status === "effective" ? (
                                <DetailWorkspaceSection width="form">
                                    <Link
                                        id="supplier-portal-review-unlisted-link"
                                        href="/master-data/products?productListingStatus=unlisted"
                                        className={
                                            detailWorkspaceStyles.inlineLinkButton
                                        }
                                    >
                                        进入商品库集中定价及上架
                                    </Link>
                                </DetailWorkspaceSection>
                            ) : null}
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
            )}
        </>
    )

    if (embedded) {
        return (
            <div className="min-w-0">
                {query.error ? (
                    <div className="pb-4">
                        <PortalError
                            error={query.error}
                            retry={() => void query.refetch()}
                            id="supplier-portal-review-reload"
                        />
                    </div>
                ) : null}
                {bodyContent}
            </div>
        )
    }

    const showHeaderDecisionActions =
        application?.status === "pending" &&
        actionState.canProcess &&
        !actionState.conflict

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
                    showHeaderDecisionActions ? (
                        <Button
                            id="supplier-portal-review-header-return"
                            type="button"
                            size="sm"
                            variant="ghost"
                            className={detailWorkspaceStyles.quietDangerButton}
                            disabled={actionState.pending}
                            onClick={() => {
                                revealReviewTab()
                                reviewFormRef.current?.requestDecision("return")
                            }}
                        >
                            {actionState.retryDecision === "return"
                                ? "重试退回"
                                : "退回修改"}
                        </Button>
                    ) : undefined
                }
                primaryAction={
                    showHeaderDecisionActions ? (
                        <LoadingButton
                            id="supplier-portal-review-header-approve"
                            type="button"
                            size="sm"
                            className="px-3.5"
                            loading={actionState.pending}
                            disabled={actionState.pending}
                            onClick={() => {
                                revealReviewTab()
                                reviewFormRef.current?.requestDecision(
                                    "approve",
                                )
                            }}
                        >
                            {actionState.retryDecision === "approve"
                                ? "重试确认通过"
                                : "确认通过并生效"}
                        </LoadingButton>
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
        </DetailWorkspacePage>
    )
}
