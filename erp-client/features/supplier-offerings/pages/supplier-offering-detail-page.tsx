"use client"

import { useState } from "react"
import { useSearchParams } from "next/navigation"
import {
    BusinessFailureState,
    DetailWorkspaceBody,
    DetailWorkspaceChrome,
    DetailWorkspaceMeta,
    DetailWorkspacePage,
    DetailWorkspaceSection,
    DetailWorkspaceTabs,
} from "@/components/business"
import { useAccountProfileQuery } from "@/features/auth/queries"
import { hasPermission } from "@/lib/permissions"
import { useSupplierOfferingQuery } from "../hooks/queries"
import { offeringListHref } from "../lib/detail"
import { OFFERING_STATUS_LABELS } from "../types"
import type { OfferingStatusIntent } from "../lib/offering-status"
import {
    OfferingIdentitySection,
    OfferingSummary,
    OfferingTermsSection,
} from "../components/offering-overview"
import { OfferingDeliverySection } from "../components/delivery-templates/offering-delivery-section"
import { OfferingHistory } from "../components/offering-history"
import { SupplierOfferingDetailActions } from "../components/supplier-offering-row-actions"
import { ReviseOfferingDialog } from "../components/dialogs/revise-offering-dialog"
import { UpdateAvailabilityDialog } from "../components/dialogs/update-availability-dialog"
import { ChangeOfferingStatusDialog } from "../components/dialogs/change-offering-status-dialog"

export function SupplierOfferingDetailPage({
    offeringId,
}: {
    offeringId: string
}) {
    const searchParams = useSearchParams()
    const returnTo = offeringListHref(searchParams.get("returnTo"))
    const taskMode = Boolean(
        new URLSearchParams(returnTo.split("?")[1]).get("workItemId"),
    )
    const query = useSupplierOfferingQuery(offeringId)
    const account = useAccountProfileQuery()
    const [section, setSection] = useState("current")
    const [dialog, setDialog] = useState<"revise" | "availability" | null>(null)
    const [intent, setIntent] = useState<OfferingStatusIntent | null>(null)
    const offering = query.data
    const canViewCosts = hasPermission(
        account.data?.permissions,
        "supplier_offering_cost:detail",
    )
    const back = {
        id: "supplier-offering-detail-back",
        href: returnTo,
        label: taskMode ? "供应停止核对" : "供应商供给",
    }
    if (query.isPending || query.isError || !offering)
        return (
            <DetailWorkspacePage>
                <DetailWorkspaceChrome title="供给资料" back={back} />
                <DetailWorkspaceBody>
                    <DetailWorkspaceSection width="form">
                        {query.isPending ? (
                            <p
                                role="status"
                                className="text-xs text-muted-foreground"
                            >
                                正在加载供给资料…
                            </p>
                        ) : (
                            <BusinessFailureState
                                title="供给资料无法读取"
                                error={query.error ?? undefined}
                                onRetry={() => void query.refetch()}
                            />
                        )}
                    </DetailWorkspaceSection>
                </DetailWorkspaceBody>
            </DetailWorkspacePage>
        )
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
                primaryAction={
                    taskMode ? undefined : (
                        <SupplierOfferingDetailActions
                            offering={offering}
                            canRevise={
                                canViewCosts &&
                                hasPermission(
                                    account.data?.permissions,
                                    "supplier_offering:update",
                                )
                            }
                            canAvailability={hasPermission(
                                account.data?.permissions,
                                "supplier_offering_availability:update",
                            )}
                            idPrefix="supplier-offering-detail"
                            onReviseOffering={() => setDialog("revise")}
                            onUpdateAvailability={() =>
                                setDialog("availability")
                            }
                            onChangeStatus={setIntent}
                        />
                    )
                }
                tabs={
                    <DetailWorkspaceTabs
                        value={section}
                        onValueChange={setSection}
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
                        <OfferingTermsSection
                            offering={offering}
                            canViewCosts={canViewCosts}
                        />
                    </>
                ) : (
                    <DetailWorkspaceSection>
                        <OfferingHistory
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
