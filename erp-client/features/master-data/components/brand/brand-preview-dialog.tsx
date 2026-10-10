"use client"

import { useRef } from "react"
import { BanIcon, PencilIcon } from "lucide-react"
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
import { BrandLogoImage } from "@/features/master-data/components/brand/brand-card-content"
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
    const titleRef = useRef<HTMLHeadingElement>(null)
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
                initialFocus={titleRef}
                className="max-h-[calc(100dvh-2rem)] gap-0 overflow-y-auto p-0 sm:max-w-md"
            >
                <DialogHeader className="flex-row items-center gap-4 px-6 py-6 pr-14">
                    <BrandLogoImage
                        assetId={row?.brandLogoAssetId}
                        name={row?.name ?? "品牌"}
                        compact
                    />
                    <div className="min-w-0 flex-1 space-y-2">
                        <DialogTitle
                            ref={titleRef}
                            id={`${prefix}-title`}
                            tabIndex={-1}
                            className="break-words text-lg leading-6 outline-none"
                        >
                            {row?.name ?? "品牌资料"}
                        </DialogTitle>
                        <DialogDescription className="break-all text-xs leading-5">
                            品牌代码：
                            <span className="num">
                                {row?.dictionaryCode ?? row?.stableNo ?? "—"}
                            </span>
                        </DialogDescription>
                        {row ? (
                            <BusinessStatusBadge
                                context="preview"
                                label={row.lifecycleStatusLabel}
                                tone={row.lifecycleTone}
                            />
                        ) : null}
                    </div>
                </DialogHeader>
                {row ? (
                    <>
                        {row.primaryBlocker ? (
                            <p className="px-6 pb-5 text-sm text-muted-foreground">
                                {row.primaryBlocker}
                            </p>
                        ) : null}
                        <DialogFooter className="flex-row flex-wrap items-center justify-between gap-3 border-t bg-surface-sunken/40 px-6 py-4 sm:justify-between">
                            <DisabledActionHint message={disableReason}>
                                <Button
                                    id={`${prefix}-disable`}
                                    variant="ghost"
                                    size="sm"
                                    className="text-muted-foreground"
                                    disabled={!canDisable}
                                    title={disableReason}
                                    onClick={() => onDisable(row)}
                                >
                                    <BanIcon
                                        data-icon="inline-start"
                                        aria-hidden
                                    />
                                    停用品牌
                                </Button>
                            </DisabledActionHint>
                            <div className="ml-auto flex items-center gap-2">
                                <Button
                                    id={`${prefix}-cancel`}
                                    variant="outline"
                                    size="sm"
                                    onClick={onClose}
                                >
                                    关闭
                                </Button>
                                <DisabledActionHint message={reviseReason}>
                                    <Button
                                        id={`${prefix}-revise`}
                                        size="sm"
                                        disabled={!canRevise}
                                        title={reviseReason}
                                        onClick={() => onRevise(row)}
                                    >
                                        <PencilIcon
                                            data-icon="inline-start"
                                            aria-hidden
                                        />
                                        编辑品牌
                                    </Button>
                                </DisabledActionHint>
                            </div>
                        </DialogFooter>
                    </>
                ) : null}
            </DialogContent>
        </Dialog>
    )
}
