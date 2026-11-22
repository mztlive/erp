"use client"
import Link from "next/link"
import {
    DetailWorkspaceBody,
    DetailWorkspaceChrome,
    DetailWorkspacePage,
    DetailWorkspaceSection,
    detailWorkspaceStyles,
} from "@/components/business"
import { useWorkItemDetailQuery } from "@/features/work-items/queries"
import {
    PortalApplicationContent,
    PortalTermsContent,
} from "@/features/supplier-portal/components/application-content"
import { PortalError } from "@/features/supplier-portal/components/surface"
import {
    kindLabels,
    portalApplicationStatus,
} from "@/features/supplier-portal/lib/presentation"
import type { PortalTerms } from "@/features/supplier-portal/types"
import { usePortalAdminAccess, usePortalAdminApplication } from "../hooks"
import { PortalReviewImages } from "../components/review-images"
import { PortalReviewDiff } from "../components/review-diff"
import { PortalOfferingImpacts } from "../components/offering-impacts"
import { PortalReviewForm } from "./review-form"

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
    const access = usePortalAdminAccess()
    const query = usePortalAdminApplication(
        applicationId,
        access.can("supplier_portal_request:detail"),
    )
    const id =
        access.can("supplier_portal_request:detail") &&
        access.can("work_item:detail")
            ? (workItemId ?? query.data?.work_item_id ?? "")
            : ""
    const task = useWorkItemDetailQuery(id)
    const application = access.can("supplier_portal_request:detail")
        ? query.data
        : undefined
    const applicationTitle =
        application &&
        (application.title ??
            (application.input.name as string | undefined) ??
            kindLabels[application.kind])
    return (
        <DetailWorkspacePage>
            <DetailWorkspaceChrome
                title={applicationTitle ?? "供应商申请审核"}
                meta={
                    application?.supplier_name ??
                    (application ? "供应商申请" : undefined)
                }
                primaryStatus={
                    application
                        ? portalApplicationStatus(application.status)
                        : undefined
                }
                navigationMeta={applicationTitle ? "供应商申请审核" : undefined}
                secondaryActions={
                    !embedded &&
                    application?.supplier_id &&
                    access.can("supplier:detail") ? (
                        <Link
                            id="supplier-portal-review-open-supplier"
                            href={`/master-data/suppliers/${encodeURIComponent(application.supplier_id)}?section=portal-applications`}
                            className={detailWorkspaceStyles.inlineLinkButton}
                        >
                            返回供应商资料
                        </Link>
                    ) : undefined
                }
            />
            <DetailWorkspaceBody
                alerts={
                    <PortalError
                        error={query.error}
                        retry={() => void query.refetch()}
                        id="supplier-portal-review-reload"
                    />
                }
            >
                {!access.can("supplier_portal_request:detail") &&
                    !access.profile.isPending && (
                        <DetailWorkspaceSection width="form">
                            <p className="text-body-compact">
                                当前账号缺少供应商申请资料查看资格，请联系管理员配置或转交处理。
                            </p>
                        </DetailWorkspaceSection>
                    )}
                {application ? (
                    <>
                        {application.current?.terms && (
                            <DetailWorkspaceSection width="form">
                                <PortalTermsContent
                                    terms={
                                        application.current.terms as PortalTerms
                                    }
                                    title="当前生效条款"
                                />
                            </DetailWorkspaceSection>
                        )}
                        <PortalReviewDiff application={application} />
                        <PortalApplicationContent
                            application={application}
                            showImages={false}
                        />
                        {(application.kind === "terms" ||
                            application.kind === "stop") &&
                            typeof application.input.offering_id ===
                                "string" && (
                                <PortalOfferingImpacts
                                    offeringId={application.input.offering_id}
                                    enabled={access.can(
                                        "supplier_portal_request:detail",
                                    )}
                                />
                            )}
                        {application.kind === "new_product" && (
                            <PortalReviewImages application={application} />
                        )}
                        {application.status === "pending" &&
                            !access.can("work_item:detail") && (
                                <DetailWorkspaceSection width="form">
                                    <p className="text-body-compact">
                                        当前账号缺少审核任务读取资格，请联系管理员配置或转交处理。
                                    </p>
                                </DetailWorkspaceSection>
                            )}
                        {application.status === "pending" && (
                            <>
                                {task.error && (
                                    <DetailWorkspaceSection width="form">
                                        <PortalError
                                            error={task.error}
                                            retry={() => void task.refetch()}
                                            id="supplier-portal-review-task-reload"
                                        />
                                    </DetailWorkspaceSection>
                                )}
                                {access.can("work_item:detail") &&
                                    task.data && (
                                        <PortalReviewForm
                                            key={application.id}
                                            application={application}
                                            task={task.data}
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
                                    )}
                            </>
                        )}
                        {access.can("product:list") && (
                            <DetailWorkspaceSection width="form">
                                <Link
                                    id="supplier-portal-review-unlisted-link"
                                    href="/master-data/products?productListingStatus=unlisted"
                                    className="inline-block text-body-compact text-primary"
                                >
                                    进入商品库集中定价及上架
                                </Link>
                            </DetailWorkspaceSection>
                        )}
                    </>
                ) : access.profile.isPending ||
                  (access.can("supplier_portal_request:detail") &&
                      query.isPending) ? (
                    <DetailWorkspaceSection width="form">
                        <p role="status" className="text-body-compact">
                            正在读取申请资料…
                        </p>
                    </DetailWorkspaceSection>
                ) : null}
            </DetailWorkspaceBody>
        </DetailWorkspacePage>
    )
}
