"use client"

import {
    BanIcon,
    FilePenLineIcon,
    MoreHorizontalIcon,
    PackageCheckIcon,
    PauseIcon,
    PlayIcon,
} from "lucide-react"

import { Button } from "@/components/ui/button"
import {
    DropdownMenu,
    DropdownMenuContent,
    DropdownMenuItem,
    DropdownMenuSeparator,
    DropdownMenuTrigger,
} from "@/components/ui/dropdown-menu"
import { toAutomationIdSegment } from "@/lib/automation-id"
import {
    statusIntentsFor,
    statusRevisionBlocker,
    type OfferingStatusIntent,
} from "@/features/supplier-offerings/lib/offering-status"
import type { SupplierOfferingView } from "@/features/supplier-offerings/types"

function StatusIntentIcon({
    nextStatus,
}: {
    nextStatus: OfferingStatusIntent["nextStatus"]
}) {
    if (nextStatus === "PAUSED") {
        return <PauseIcon aria-hidden="true" />
    }
    if (nextStatus === "STOPPED") {
        return <BanIcon aria-hidden="true" />
    }
    return <PlayIcon aria-hidden="true" />
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
        offering.sku_name?.trim() || offering.sku_no || offering.supplier_sku_code
    return (
        <DropdownMenu>
            <DropdownMenuTrigger
                id={`supplier-offerings-table-row-${rowId}-actions`}
                render={
                    <Button
                        type="button"
                        size="icon-xs"
                        variant="ghost"
                        aria-label={`${label} 操作`}
                    />
                }
            >
                <MoreHorizontalIcon aria-hidden="true" />
            </DropdownMenuTrigger>
            <DropdownMenuContent align="end" className="min-w-44">
                <DropdownMenuItem
                    id={`supplier-offerings-table-row-${rowId}-update-availability`}
                    onClick={() => onUpdateAvailability(offering)}
                >
                    <PackageCheckIcon aria-hidden="true" />
                    更新可供
                </DropdownMenuItem>
                <DropdownMenuItem
                    id={`supplier-offerings-table-row-${rowId}-revise`}
                    onClick={() => onReviseOffering(offering)}
                >
                    <FilePenLineIcon aria-hidden="true" />
                    修订条款
                </DropdownMenuItem>
                <DropdownMenuSeparator />
                {statusIntentsFor(offering.status).map((intent) => (
                    <DropdownMenuItem
                        key={intent.actionId}
                        id={`supplier-offerings-table-row-${rowId}-${intent.actionId}`}
                        variant={intent.destructive ? "destructive" : "default"}
                        disabled={blocker != null}
                        title={blocker ?? undefined}
                        onClick={() => onChangeStatus(offering, intent)}
                    >
                        <StatusIntentIcon nextStatus={intent.nextStatus} />
                        {intent.label}
                    </DropdownMenuItem>
                ))}
            </DropdownMenuContent>
        </DropdownMenu>
    )
}
