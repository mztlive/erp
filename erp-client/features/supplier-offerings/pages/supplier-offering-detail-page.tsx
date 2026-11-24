"use client"

import { useState } from "react"
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
} from "@/components/business"
import { useAccountProfileQuery } from "@/features/auth/queries"
import { usePathname, useRouter, useSearchParams } from "@/lib/navigation"
import { hasPermission } from "@/lib/permissions"
import { useSupplierOfferingQuery } from "../hooks/queries"
import { resolveOfferingBackContext } from "../lib/detail"
import { OFFERING_STATUS_LABELS } from "../types"
import type { OfferingStatusIntent } from "../lib/offering-status"
import {
    OfferingIdentitySection,
    OfferingSummary,
    OfferingTermsSection,
} from "../components/offering-overview"
import { OfferingDeliverySection } from "../components/delivery-templates/offering-delivery-section"
import { OfferingHistory } from "../components/offering-history"
import {
    SupplierOfferingDetailPrimaryAction,
    SupplierOfferingDetailSecondaryActions,
} from "../components/supplier-offering-row-actions"
import { ReviseOfferingDialog } from "../components/dialogs/revise-offering-dialog"
import { UpdateAvailabilityDialog } from "../components/dialogs/update-availability-dialog"
import { ChangeOfferingStatusDialog } from "../components/dialogs/change-offering-status-dialog"

type OfferingSection = "current" | "history"

function parseOfferingSection(raw: string | null): OfferingSection {
    return raw === "history" ? "history" : "current"
}

export function SupplierOfferingDetailPage({
    offeringId,
}: {
    offeringId: string
}) {
    const pathname = usePathname()
    const router = useRouter()
    const searchParams = useSearchParams()
    const backContext = resolveOfferingBackContext(searchParams.get("returnTo"))
    const taskMode = backContext.taskMode
    const section = parseOfferingSection(searchParams.get("section"))
    const query = useSupplierOfferingQuery(offeringId)
    const account = useAccountProfileQuery()
    const [dialog, setDialog] = useState<"revise" | "availability" | null>(null)
    const [intent, setIntent] = useState<OfferingStatusIntent | null>(null)
    const offering = query.data
    const canViewCosts = hasPermission(
        account.data?.permissions,
        "supplier_offering_cost:detail",
    )
    const canRevise =
        canViewCosts &&
        hasPermission(account.data?.permissions, "supplier_offering:update")
    const canAvailability = hasPermission(
        account.data?.permissions,
        "supplier_offering_availability:update",
    )
    const back = {
        id: "supplier-offering-detail-back",
        href: backContext.href,
        label: backContext.label,
        visibility: backContext.hasReturnTo
            ? ("always" as const)
            : ("standalone-only" as const),
    }

    const handleSectionChange = (next: string) => {
        const normalized = parseOfferingSection(next)
        if (normalized === section) return
        const params = new URLSearchParams(searchParams.toString())
        if (normalized === "current") {
            params.delete("section")
        } else {
            params.set("section", normalized)
        }
        const qs = params.toString()
        router.replace(qs ? `${pathname}?${qs}` : pathname, { scroll: false })
    }

    if (query.isPending || account.isPending) {
        return <DetailWorkspaceSkeleton title="供给资料" summary sidebar />
    }

    if (query.isError) {
        return (
            <DetailWorkspacePage>
                <DetailWorkspaceChrome title="供给资料" back={back} />
                <DetailWorkspaceBody>
                    <DetailWorkspaceSection width="form">
                        <BusinessFailureState
                            title="供给资料无法读取"
                            error={query.error}
                            onRetry={() => void query.refetch()}
                        />
                    </DetailWorkspaceSection>
                </DetailWorkspaceBody>
            </DetailWorkspacePage>
        )
    }

    if (!offering) {
        return (
            <DetailWorkspacePage>
                <DetailWorkspaceChrome title="供给资料" back={back} />
                <DetailWorkspaceBody>
                    <DetailWorkspaceSection width="form">
                        <BusinessEmptyState
                            kind="no-data"
                            title="未找到供给资料"
                            description="该供给记录可能已被删除，或当前账号暂无查看权限。"
                        />
                    </DetailWorkspaceSection>
                </DetailWorkspaceBody>
            </DetailWorkspacePage>
        )
    }

    return (
        <DetailWorkspacePage>
            <DetailWorkspaceChrome
                title={offering.sku_name || offering.sku_no || "供给资料"}
                back={back}
                primaryStatus={{
                    label: OFFERING_STATUS_LABELS[offering.status],
                    tone:
                        offering.status === "ACTIVE"
                            ? "success"
                            : offering.status === "STOPPED"
                              ? "destructive"
                              : "neutral",
                }}
                documentNumber={offering.supplier_sku_code}
                numberLabel="订货编码"
                meta={
                    <DetailWorkspaceMeta
                        items={[
                            offering.supplier_name ||
                                offering.supplier_no ||
                                "供应商",
                            offering.specification,
                        ]}
                    />
                }
                secondaryActions={
                    taskMode ? undefined : (
                        <SupplierOfferingDetailSecondaryActions
                            offering={offering}
                            canRevise={canRevise}
                            canAvailability={canAvailability}
                            idPrefix="supplier-offering-detail"
                            onUpdateAvailability={() =>
                                setDialog("availability")
                            }
                            onChangeStatus={setIntent}
                        />
                    )
                }
                primaryAction={
                    taskMode ? undefined : (
                        <SupplierOfferingDetailPrimaryAction
                            offering={offering}
                            canRevise={canRevise}
                            idPrefix="supplier-offering-detail"
                            onReviseOffering={() => setDialog("revise")}
                        />
                    )
                }
                tabs={
                    <DetailWorkspaceTabs
                        value={section}
                        onValueChange={handleSectionChange}
                        items={[
                            { id: "current", label: "当前供给" },
                            { id: "history", label: "条款历史" },
                        ]}
                        ariaLabel="供给资料分区"
                        tabIdPrefix="supplier-offering-detail-tab"
                    />
                }
            />
            <DetailWorkspaceBody
                summary={
                    <OfferingSummary
                        offering={offering}
                        canViewCosts={canViewCosts}
                    />
                }
                sidebar={
                    section === "current" ? (
                        <OfferingIdentitySection offering={offering} />
                    ) : undefined
                }
                sidebarLabel="供给资料"
            >
                {section === "current" ? (
                    <>
                        <OfferingDeliverySection
                            offering={offering}
                            canEdit={
                                !taskMode &&
                                hasPermission(
                                    account.data?.permissions,
                                    "supplier_offering:update",
                                )
                            }
                        />
                        <OfferingTermsSection offering={offering} />
                    </>
                ) : (
                    <DetailWorkspaceSection>
                        <OfferingHistory
                            skuId={offering.sku_id}
                            offeringId={offeringId}
                            canViewCosts={canViewCosts}
                        />
                    </DetailWorkspaceSection>
                )}
            </DetailWorkspaceBody>
            {dialog === "revise" ? (
                <ReviseOfferingDialog
                    offering={offering}
                    onOpenChange={(open) => {
                        if (!open) setDialog(null)
                    }}
                />
            ) : null}
            {dialog === "availability" ? (
                <UpdateAvailabilityDialog
                    offering={offering}
                    onOpenChange={(open) => {
                        if (!open) setDialog(null)
                    }}
                />
            ) : null}
            {intent ? (
                <ChangeOfferingStatusDialog
                    offering={offering}
                    intent={intent}
                    onOpenChange={(open) => {
                        if (!open) setIntent(null)
                    }}
                />
            ) : null}
        </DetailWorkspacePage>
    )
}
