"use client"

import { BanIcon } from "lucide-react"
import { BusinessStatusBadge, QuickPreviewSheet } from "@/components/business"
import { Button } from "@/components/ui/button"
import { DisabledActionHint } from "@/features/master-data/components/list/list-chrome"
import { masterDataCopy } from "@/features/master-data/lib/copy"
import type { MasterDataListItem } from "@/features/master-data/types"

/** 单位预览直接展示列表已有资料，修改和停用沿用独立确认弹窗。 */
export function UnitOfMeasurePreviewSheet({
    row,
    lastFocusedRowId,
    onClose,
    onRevise,
    onDisable,
}: {
    row: MasterDataListItem | null
    lastFocusedRowId: { current: string | null }
    onClose: () => void
    onRevise: (row: MasterDataListItem) => void
    onDisable: (row: MasterDataListItem) => void
}) {
    const prefix = "master-data-unit-of-measures-preview"
    const canRevise = row?.allowedActions.includes("CREATE_REVISION") ?? false
    const canDisable = row?.allowedActions.includes("DISABLE") ?? false
    const reviseReason = canRevise
        ? undefined
        : (row?.actionBlockers.find(
              (blocker) => blocker.action === "CREATE_REVISION",
          )?.message ?? "当前不可修改此单位")
    const disableReason = canDisable
        ? undefined
        : (row?.actionBlockers.find((blocker) => blocker.action === "DISABLE")
              ?.message ?? "当前不可停用此单位")

    return (
        <QuickPreviewSheet
            idPrefix={`${prefix}-sheet`}
            open={row != null}
            onOpenChange={(open) => {
                if (!open) onClose()
            }}
            onOpenChangeComplete={(open) => {
                if (!open && lastFocusedRowId.current)
                    document
                        .querySelector<HTMLElement>(
                            `[data-row-id="${CSS.escape(lastFocusedRowId.current)}"]`,
                        )
                        ?.focus()
            }}
            size="preview"
            title={row?.name ?? "计量单位详情"}
            identity="计量单位"
            summary={
                row ? (
                    <BusinessStatusBadge
                        context="preview"
                        label={row.lifecycleStatusLabel}
                        tone={row.lifecycleTone}
                    />
                ) : null
            }
            footer={
                row ? (
                    <>
                        <Button
                            id={`${prefix}-close`}
                            variant="outline"
                            onClick={onClose}
                        >
                            关闭
                        </Button>
                        <DisabledActionHint message={disableReason}>
                            <Button
                                id={`${prefix}-disable`}
                                variant="ghost"
                                disabled={!canDisable}
                                title={disableReason}
                                onClick={() => onDisable(row)}
                            >
                                <BanIcon aria-hidden />
                                {masterDataCopy.actionDisable}
                            </Button>
                        </DisabledActionHint>
                        <DisabledActionHint message={reviseReason}>
                            <Button
                                id={`${prefix}-revise`}
                                disabled={!canRevise}
                                title={reviseReason}
                                onClick={() => onRevise(row)}
                            >
                                {masterDataCopy.actionUpdate}
                            </Button>
                        </DisabledActionHint>
                    </>
                ) : null
            }
        >
            {row ? (
                <section className="space-y-4 text-sm">
                    <h3 className="font-medium">单位资料</h3>
                    <dl className="space-y-4">
                        {[
                            masterDataCopy.fUnitCode,
                            masterDataCopy.fUnitSymbol,
                            masterDataCopy.fQuantityScale,
                        ].map((label) => (
                            <div
                                key={label}
                                className="flex min-w-0 items-baseline justify-between gap-5"
                            >
                                <dt className="shrink-0 text-xs text-muted-foreground">
                                    {label}
                                </dt>
                                <dd className="num min-w-0 break-all text-right text-[13px]">
                                    {row.keyFacts.find(
                                        (fact) => fact.label === label,
                                    )?.value || "—"}
                                </dd>
                            </div>
                        ))}
                    </dl>
                    {row.primaryBlocker ? (
                        <p className="text-sm text-muted-foreground">
                            {row.primaryBlocker}
                        </p>
                    ) : null}
                </section>
            ) : null}
        </QuickPreviewSheet>
    )
}
