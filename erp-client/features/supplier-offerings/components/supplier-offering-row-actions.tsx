"use client"

import {
    BanIcon,
    FilePenLineIcon,
    PackageCheckIcon,
    PauseIcon,
    PlayIcon,
} from "lucide-react"

import { TableRowActions } from "@/components/business/table-row-actions"
import { toAutomationIdSegment } from "@/lib/automation-id"
import {
    statusIntentsFor,
    statusRevisionBlocker,
    type OfferingStatusIntent,
} from "@/features/supplier-offerings/lib/offering-status"
import type { SupplierOfferingView } from "@/features/supplier-offerings/types"

function statusIntentIcon(nextStatus: OfferingStatusIntent["nextStatus"]) {
    if (nextStatus === "PAUSED") {
        return PauseIcon
    }
    if (nextStatus === "STOPPED") {
        return BanIcon
    }
    return PlayIcon
}

export function SupplierOfferingRowActions({
    offering,
    onUpdateAvailability,
    onReviseOffering,
    onChangeStatus,
    canRevise = true,
    canAvailability = true,
    maxInline = 0,
    idPrefix = "supplier-offerings-table-row",
}: {
    offering: SupplierOfferingView
    canRevise?: boolean
    canAvailability?: boolean
    maxInline?: number
    idPrefix?: string
    onUpdateAvailability: (offering: SupplierOfferingView) => void
    onReviseOffering: (offering: SupplierOfferingView) => void
    onChangeStatus: (
        offering: SupplierOfferingView,
        intent: OfferingStatusIntent,
    ) => void
}) {
    const rowId = toAutomationIdSegment(offering.id)
    const blocker = statusRevisionBlocker(offering)
    const label =
        offering.sku_name?.trim() ||
        offering.sku_no ||
        offering.supplier_sku_code
    return (
        <TableRowActions
            className={maxInline > 0 ? "flex-wrap" : undefined}
            moreId={`${idPrefix}-${rowId}-actions`}
            moreLabel={`${label} 操作`}
            maxInline={maxInline}
            actions={[
                ...(canAvailability
                    ? [
                          {
                              id: `${idPrefix}-${rowId}-update-availability`,
                              label: "更新可供",
                              icon: PackageCheckIcon,
                              onClick: () => onUpdateAvailability(offering),
                          },
                      ]
                    : []),
                ...(canRevise
                    ? [
                          {
                              id: `${idPrefix}-${rowId}-revise`,
                              label: "修订条款",
                              icon: FilePenLineIcon,
                              onClick: () => onReviseOffering(offering),
                          },
                      ]
                    : []),
                ...(canRevise ? statusIntentsFor(offering.status) : []).map(
                    (intent) => ({
                        id: `${idPrefix}-${rowId}-${intent.actionId}`,
                        label: intent.label,
                        icon: statusIntentIcon(intent.nextStatus),
                        destructive: intent.destructive,
                        disabled: blocker != null,
                        disabledReason: blocker ?? undefined,
                        onClick: () => onChangeStatus(offering, intent),
                    }),
                ),
            ]}
        />
    )
}
