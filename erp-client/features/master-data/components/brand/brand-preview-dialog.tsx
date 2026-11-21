"use client"

import { PermissionGuard } from "@/features/auth/components/permission-guard"

import { BanIcon, PencilIcon } from "lucide-react"
import { BusinessStatusBadge, QuickPreviewSheet } from "@/components/business"
import { Button } from "@/components/ui/button"
import { DisabledActionHint } from "@/features/master-data/components/list/list-chrome"
import { BrandLogoImage } from "@/features/master-data/components/brand/brand-card-content"
import type { MasterDataListItem } from "@/features/master-data/types"
import { restoreListRowFocus } from "@/lib/list-row-focus"

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
        <QuickPreviewSheet
            idPrefix={prefix}
            size="preview"
            open={row != null}
            onOpenChange={(open) => {
                if (!open) onClose()
            }}
            onOpenChangeComplete={(open) => {
                if (!open) restoreListRowFocus(lastFocusedRowId.current)
            }}
            identity={
                row ? (
                    <span className="num wrap-anywhere">
                        品牌代码：{row.dictionaryCode ?? row.stableNo}
                    </span>
                ) : null
            }
            title={
                <span id={`${prefix}-title`}>{row?.name ?? "品牌资料"}</span>
            }
            headerMedia={
                row ? (
                    <BrandLogoImage
                        assetId={row.brandLogoAssetId}
                        name={row.name}
                    />
                ) : null
            }
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
                            id={`${prefix}-cancel`}
                            type="button"
                            variant="outline"
                            size="sm"
                            onClick={onClose}
                        >
                            关闭
                        </Button>
                        <PermissionGuard permission="product_brand:update">
                            <DisabledActionHint message={disableReason}>
                                <Button
                                    id={`${prefix}-disable`}
                                    type="button"
                                    variant="ghost"
                                    size="sm"
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
                            <DisabledActionHint message={reviseReason}>
                                <Button
                                    id={`${prefix}-revise`}
                                    type="button"
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
                        </PermissionGuard>
                    </>
                ) : null
            }
        >
            {row?.primaryBlocker ? (
                <p className="text-body-compact text-muted-foreground">
                    {row.primaryBlocker}
                </p>
            ) : null}
        </QuickPreviewSheet>
    )
}
