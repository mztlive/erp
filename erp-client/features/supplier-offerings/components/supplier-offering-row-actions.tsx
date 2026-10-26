"use client"

import {
    BanIcon,
    FilePenLineIcon,
    PackageCheckIcon,
    PauseIcon,
    PlayIcon,
} from "lucide-react"

import { GuardedBusinessAction } from "@/components/business"
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

/** 详情页头操作：状态动作与更新可供用描边按钮，修订条款为主按钮放最右。 */
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
    const rowId = toAutomationIdSegment(offering.id)
    const blocker = statusRevisionBlocker(offering)
    return (
        <>
            {canAvailability ? (
                <GuardedBusinessAction
                    id={`${idPrefix}-${rowId}-update-availability`}
                    type="button"
                    size="sm"
                    variant="outline"
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
            {canRevise
                ? statusIntentsFor(offering.status).map((intent) => {
                      const Icon = statusIntentIcon(intent.nextStatus)
                      return (
                          <GuardedBusinessAction
                              key={intent.actionId}
                              id={`${idPrefix}-${rowId}-${intent.actionId}`}
                              type="button"
                              size="sm"
                              variant="outline"
                              disabled={blocker != null}
                              reason={blocker ?? undefined}
                              onClick={() => onChangeStatus(intent)}
                          >
                              <Icon
                                  data-icon="inline-start"
                                  aria-hidden="true"
                              />
                              {intent.label}
                          </GuardedBusinessAction>
                      )
                  })
                : null}
            {canRevise ? (
                <GuardedBusinessAction
                    id={`${idPrefix}-${rowId}-revise`}
                    type="button"
                    size="sm"
                    onClick={onReviseOffering}
                >
                    <FilePenLineIcon
                        data-icon="inline-start"
                        aria-hidden="true"
                    />
                    修订条款
                </GuardedBusinessAction>
            ) : null}
        </>
    )
}
