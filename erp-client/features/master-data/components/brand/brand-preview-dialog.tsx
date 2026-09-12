"use client"

import { BanIcon } from "lucide-react"
import { BusinessStatusBadge } from "@/components/business"
import { Button } from "@/components/ui/button"
import {
    Dialog,
    DialogContent,
    DialogDescription,
    DialogFooter,
    DialogHeader,
    DialogTitle,
} from "@/components/ui/dialog"
import { DisabledActionHint } from "@/features/master-data/components/list/list-chrome"
import { masterDataCopy } from "@/features/master-data/lib/copy"
import type { MasterDataListItem } from "@/features/master-data/types"

/** 品牌资料使用紧凑的只读预览，更新和停用仍由原有弹窗处理。 */
export function BrandPreviewDialog({
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
    const prefix = "master-data-brands-preview"
    const canRevise = row?.allowedActions.includes("CREATE_REVISION") ?? false
    const canDisable = row?.allowedActions.includes("DISABLE") ?? false
    const reviseReason = canRevise
        ? undefined
        : (row?.actionBlockers.find(
              (blocker) => blocker.action === "CREATE_REVISION",
          )?.message ?? "当前不可修改此品牌")
    const disableReason = canDisable
        ? undefined
        : (row?.actionBlockers.find((blocker) => blocker.action === "DISABLE")
              ?.message ?? "当前不可停用此品牌")

    return (
        <Dialog
            open={row != null}
            onOpenChange={(open) => {
                if (!open) onClose()
            }}
            onOpenChangeComplete={(open) => {
                if (!open && lastFocusedRowId.current) {
                    document
                        .querySelector<HTMLElement>(
                            `[data-row-id="${CSS.escape(lastFocusedRowId.current)}"]`,
                        )
                        ?.focus()
                }
            }}
        >
            <DialogContent
                closeButtonId={`${prefix}-close`}
                className="max-h-[calc(100dvh-2rem)] overflow-y-auto sm:max-w-sm"
            >
                <DialogHeader>
                    <DialogTitle className="break-words pr-6 leading-6">
                        {row?.name ?? "品牌资料"}
                    </DialogTitle>
                    <DialogDescription className="break-all">
                        品牌代码：
                        <span className="num">
                            {row?.dictionaryCode ?? row?.stableNo ?? "—"}
                        </span>
                    </DialogDescription>
                </DialogHeader>
                {row ? (
                    <>
                        <div className="space-y-3">
                            <BusinessStatusBadge
                                context="preview"
                                label={row.lifecycleStatusLabel}
                                tone={row.lifecycleTone}
                            />
                            {row.primaryBlocker ? (
                                <p className="text-sm text-muted-foreground">
                                    {row.primaryBlocker}
                                </p>
                            ) : null}
                        </div>
                        <DialogFooter className="flex-row flex-wrap justify-end">
                            <Button
                                id={`${prefix}-cancel`}
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
                        </DialogFooter>
                    </>
                ) : null}
            </DialogContent>
        </Dialog>
    )
}
