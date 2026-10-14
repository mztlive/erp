"use client"

import { useState } from "react"
import { useSearchParams } from "next/navigation"
import {
    BusinessFailureState,
    PageScaffold,
    surfacePanelClassName,
} from "@/components/business"
import { DetailPageHeader } from "@/components/business/detail-page-header"
import { Badge } from "@/components/ui/badge"
import { Tabs, TabsList, TabsTrigger, TabsContent } from "@/components/ui/tabs"
import { useAccountProfileQuery } from "@/features/auth/queries"
import { hasPermission } from "@/lib/permissions"
import { cn } from "@/lib/utils"
import { useSupplierOfferingQuery } from "../hooks/queries"
import { offeringListHref } from "../lib/detail"
import { statusVariant } from "../lib/presentation"
import { OFFERING_STATUS_LABELS } from "../types"
import type { OfferingStatusIntent } from "../lib/offering-status"
import {
    OfferingIdentityCard,
    OfferingSummary,
    OfferingTermsCard,
} from "../components/offering-overview"
import { OfferingDeliverySection } from "../components/delivery-templates/offering-delivery-section"
import { OfferingHistory } from "../components/offering-history"
import { SupplierOfferingRowActions } from "../components/supplier-offering-row-actions"
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
    if (query.isPending)
        return (
            <PageScaffold density="compact">
                <DetailPageHeader title="供给资料" back={back} />
                <p role="status" className="text-sm text-muted-foreground">
                    正在加载供给资料…
                </p>
            </PageScaffold>
        )
    if (query.isError || !offering)
        return (
            <PageScaffold density="compact">
                <DetailPageHeader title="供给资料" back={back} />
                <BusinessFailureState
                    title="供给资料无法读取"
                    error={query.error ?? undefined}
                    onRetry={() => void query.refetch()}
                />
            </PageScaffold>
        )
    return (
        <PageScaffold density="compact">
            <div className="min-w-0 space-y-5">
                <DetailPageHeader
                    title={offering.sku_name || offering.sku_no || "供给资料"}
                    back={back}
                    titleExtra={
                        <Badge variant={statusVariant(offering.status)}>
                            {OFFERING_STATUS_LABELS[offering.status]}
                        </Badge>
                    }
                    meta={
                        <>
                            <span>
                                {offering.supplier_name ||
                                    offering.supplier_no ||
                                    "供应商"}
                            </span>
                            <span className="num">
                                订货编码 {offering.supplier_sku_code}
                            </span>
                            {offering.specification ? (
                                <span>{offering.specification}</span>
                            ) : null}
                        </>
                    }
                    primaryAction={
                        taskMode ? undefined : (
                            <SupplierOfferingRowActions
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
                                maxInline={2}
                                idPrefix="supplier-offering-detail"
                                onReviseOffering={() => setDialog("revise")}
                                onUpdateAvailability={() =>
                                    setDialog("availability")
                                }
                                onChangeStatus={(_, next) => setIntent(next)}
                            />
                        )
                    }
                />
                <OfferingSummary
                    offering={offering}
                    canViewCosts={canViewCosts}
                />
                <Tabs
                    value={section}
                    onValueChange={(value) => {
                        if (value) setSection(String(value))
                    }}
                    className={cn(surfacePanelClassName, "min-w-0 gap-0")}
                >
                    <TabsList
                        variant="line"
                        aria-label="供给资料分区"
                        className="h-auto w-full justify-start gap-6 rounded-none border-b border-border bg-card px-4 md:px-6"
                    >
                        <TabsTrigger
                            id="supplier-offering-detail-tab-current"
                            value="current"
                            className="h-11 rounded-none px-0"
                        >
                            当前供给
                        </TabsTrigger>
                        <TabsTrigger
                            id="supplier-offering-detail-tab-history"
                            value="history"
                            className="h-11 rounded-none px-0"
                        >
                            条款历史
                        </TabsTrigger>
                    </TabsList>
                    <TabsContent
                        value="current"
                        className="min-w-0 bg-muted/20 p-4 md:p-6"
                    >
                        <div className="grid min-w-0 items-start gap-5 xl:grid-cols-[minmax(0,1fr)_340px]">
                            <div className="min-w-0 space-y-5">
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
                                <OfferingTermsCard
                                    offering={offering}
                                    canViewCosts={canViewCosts}
                                />
                            </div>
                            <OfferingIdentityCard offering={offering} />
                        </div>
                    </TabsContent>
                    <TabsContent value="history" className="min-w-0 p-4 md:p-6">
                        {section === "history" ? (
                            <OfferingHistory
                                offeringId={offeringId}
                                canViewCosts={canViewCosts}
                            />
                        ) : null}
                    </TabsContent>
                </Tabs>
            </div>
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
        </PageScaffold>
    )
}
