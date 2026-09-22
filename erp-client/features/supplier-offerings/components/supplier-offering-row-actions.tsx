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
}: {
    offering: SupplierOfferingView
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
            moreId={`supplier-offerings-table-row-${rowId}-actions`}
            moreLabel={`${label} 操作`}
            maxInline={0}
            actions={[
                {
                    id: `supplier-offerings-table-row-${rowId}-update-availability`,
                    label: "更新可供",
                    icon: PackageCheckIcon,
                    onClick: () => onUpdateAvailability(offering),
                },
                {
                    id: `supplier-offerings-table-row-${rowId}-revise`,
                    label: "修订条款",
                    icon: FilePenLineIcon,
                    onClick: () => onReviseOffering(offering),
                },
                ...statusIntentsFor(offering.status).map((intent) => ({
                    id: `supplier-offerings-table-row-${rowId}-${intent.actionId}`,
                    label: intent.label,
                    icon: statusIntentIcon(intent.nextStatus),
                    destructive: intent.destructive,
                    disabled: blocker != null,
                    disabledReason: blocker ?? undefined,
                    onClick: () => onChangeStatus(offering, intent),
                })),
            ]}
        />
    )
}
