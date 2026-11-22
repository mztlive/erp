"use client"

import { useState } from "react"
import {
    BanIcon,
    FilePenLineIcon,
    MoreHorizontalIcon,
    PackageCheckIcon,
    PauseIcon,
    PlayIcon,
} from "lucide-react"

import { detailWorkspaceStyles } from "@/components/business/detail-workspace"
import { GuardedBusinessAction } from "@/components/business"
import { TableRowActions } from "@/components/business/table-row-actions"
import { Button } from "@/components/ui/button"
import {
    Popover,
    PopoverContent,
    PopoverTrigger,
} from "@/components/ui/popover"
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
                              disabled: offering.source_type === "API",
                              disabledReason:
                                  offering.source_type === "API"
                                      ? "可供情况随供应商接口更新；停止使用请暂停供给"
                                      : undefined,
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

/** 详情页头次要操作：更新可供平铺，暂停/启用/停止收拢至「更多」菜单。 */
export function SupplierOfferingDetailSecondaryActions({
    offering,
    onUpdateAvailability,
    onChangeStatus,
    canRevise,
    canAvailability,
    idPrefix,
}: {
    offering: SupplierOfferingView
    canRevise: boolean
    canAvailability: boolean
    idPrefix: string
    onUpdateAvailability: () => void
    onChangeStatus: (intent: OfferingStatusIntent) => void
}) {
    const [moreOpen, setMoreOpen] = useState(false)
    const rowId = toAutomationIdSegment(offering.id)
    const blocker = statusRevisionBlocker(offering)
    const statusIntents = canRevise ? statusIntentsFor(offering.status) : []
    if (!canAvailability && statusIntents.length === 0) return null
    return (
        <>
            {canAvailability ? (
                <GuardedBusinessAction
                    id={`${idPrefix}-${rowId}-update-availability`}
                    type="button"
                    size="sm"
                    variant="ghost"
                    className={detailWorkspaceStyles.quietButton}
                    onClick={onUpdateAvailability}
                    disabled={offering.source_type === "API"}
                    reason={
                        offering.source_type === "API"
                            ? "可供情况随供应商接口更新；停止使用请暂停供给"
                            : undefined
                    }
                >
                    <PackageCheckIcon
                        data-icon="inline-start"
                        aria-hidden="true"
                    />
                    更新可供
                </GuardedBusinessAction>
            ) : null}
            {statusIntents.length > 0 ? (
                <Popover open={moreOpen} onOpenChange={setMoreOpen}>
                    <PopoverTrigger
                        render={
                            <Button
                                id={`${idPrefix}-${rowId}-more`}
                                type="button"
                                size="sm"
                                variant="ghost"
                                className={detailWorkspaceStyles.quietButton}
                                aria-label="更多操作"
                            />
                        }
                    >
                        <MoreHorizontalIcon
                            data-icon="inline-start"
                            aria-hidden="true"
                        />
                        更多
                    </PopoverTrigger>
                    <PopoverContent align="end" className="w-44 p-1.5">
                        <div className="flex flex-col gap-0.5">
                            {statusIntents.map((intent) => {
                                const Icon = statusIntentIcon(intent.nextStatus)
                                return (
                                    <GuardedBusinessAction
                                        key={intent.actionId}
                                        id={`${idPrefix}-${rowId}-${intent.actionId}`}
                                        type="button"
                                        size="sm"
                                        variant="ghost"
                                        className={
                                            intent.destructive
                                                ? "w-full justify-start text-destructive hover:bg-destructive/10 hover:text-destructive"
                                                : "w-full justify-start"
                                        }
                                        disabled={blocker != null}
                                        reason={blocker ?? undefined}
                                        onClick={() => {
                                            setMoreOpen(false)
                                            onChangeStatus(intent)
                                        }}
                                    >
                                        <Icon
                                            data-icon="inline-start"
                                            aria-hidden="true"
                                        />
                                        {intent.label}
                                    </GuardedBusinessAction>
                                )
                            })}
                        </div>
                    </PopoverContent>
                </Popover>
            ) : null}
        </>
    )
}

/** 详情页头唯一实心主按钮：修订条款。 */
export function SupplierOfferingDetailPrimaryAction({
    offering,
    canRevise,
    idPrefix,
    onReviseOffering,
}: {
    offering: SupplierOfferingView
    canRevise: boolean
    idPrefix: string
    onReviseOffering: () => void
}) {
    if (!canRevise) return null
    const rowId = toAutomationIdSegment(offering.id)
    return (
        <GuardedBusinessAction
            id={`${idPrefix}-${rowId}-revise`}
            type="button"
            size="sm"
            className="px-3.5"
            onClick={onReviseOffering}
        >
            <FilePenLineIcon data-icon="inline-start" aria-hidden="true" />
            修订条款
        </GuardedBusinessAction>
    )
}

/** 详情页头操作兼容导出。 */
export function SupplierOfferingDetailActions({
    offering,
    onUpdateAvailability,
    onReviseOffering,
    onChangeStatus,
    canRevise,
    canAvailability,
    idPrefix,
}: {
    offering: SupplierOfferingView
    canRevise: boolean
    canAvailability: boolean
    idPrefix: string
    onUpdateAvailability: () => void
    onReviseOffering: () => void
    onChangeStatus: (intent: OfferingStatusIntent) => void
}) {
    return (
        <>
            <SupplierOfferingDetailSecondaryActions
                offering={offering}
                canRevise={canRevise}
                canAvailability={canAvailability}
                idPrefix={idPrefix}
                onUpdateAvailability={onUpdateAvailability}
                onChangeStatus={onChangeStatus}
            />
            <SupplierOfferingDetailPrimaryAction
                offering={offering}
                canRevise={canRevise}
                idPrefix={idPrefix}
                onReviseOffering={onReviseOffering}
            />
        </>
    )
}
