"use client"

import { BusinessStatusBadge, QuickPreviewSheet } from "@/components/business"
import { Badge } from "@/components/ui/badge"
import { Button } from "@/components/ui/button"
import { VoucherCategoryStatusButton } from "./voucher-category-status-dialog"
import type { useMasterDataCenterQuery } from "@/features/master-data/hooks/queries"
import { masterDataCopy } from "@/features/master-data/lib/copy"
import type { MasterDataListItem } from "@/features/master-data/types"

/** 卡券类目只读详情与版本记录，沿用公司商品池的窄栏与分区样式。 */
export function VoucherCategoryPreviewSheet({
    row,
    detailQuery,
    lastFocusedRowId,
    onClose,
    onRevise,
    onStatusChange,
}: {
    row: MasterDataListItem | null
    detailQuery: ReturnType<typeof useMasterDataCenterQuery>
    lastFocusedRowId: { current: string | null }
    onClose: () => void
    onRevise: (row: MasterDataListItem) => void
    onStatusChange: (row: MasterDataListItem) => void
}) {
    const prefix = "master-data-voucher-categories-preview"
    const detail = detailQuery.data
    const description = (detail?.currentRevision.fields ?? row?.keyFacts)?.find(
        (fact) =>
            fact.label === "说明" || fact.label === masterDataCopy.fDescription,
    )?.value
    const canRevise = row?.allowedActions.includes("CREATE_REVISION") ?? false

    return (
        <QuickPreviewSheet
            idPrefix={`${prefix}-sheet`}
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
            size="preview"
            contentClassName="data-[side=right]:sm:w-[460px] data-[side=right]:sm:max-w-[460px]"
            title={detail?.name ?? row?.name ?? "卡券类目详情"}
            identity={
                row ? (
                    <span className="num">类目编号：{row.stableNo}</span>
                ) : null
            }
            summary={
                row ? (
                    <div className="flex flex-wrap items-center gap-2">
                        <BusinessStatusBadge
                            context="preview"
                            label={
                                detail?.lifecycleStatusLabel ??
                                row.lifecycleStatusLabel
                            }
                            tone={detail?.lifecycleTone ?? row.lifecycleTone}
                        />
                        <span className="num text-xs text-muted-foreground">
                            当前版本 v
                            {detail?.currentRevision.revisionNo ??
                                row.revisionNo}
                        </span>
                    </div>
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
                        <VoucherCategoryStatusButton
                            row={row}
                            surface="sheet"
                            onClick={() => onStatusChange(row)}
                        />
                        <Button
                            id={`${prefix}-revise`}
                            disabled={!canRevise}
                            title={
                                row.actionBlockers.find(
                                    (blocker) =>
                                        blocker.action === "CREATE_REVISION",
                                )?.message
                            }
                            onClick={() => onRevise(row)}
                        >
                            {masterDataCopy.actionUpdate}
                        </Button>
                    </>
                ) : null
            }
        >
            {row ? (
                <div className="space-y-6 text-sm">
                    <section className="space-y-3 border-b border-border pb-6">
                        <h3 className="font-medium">类目资料</h3>
                        <dl className="space-y-4">
                            <div className="flex min-w-0 items-baseline justify-between gap-5">
                                <dt className="shrink-0 text-xs text-muted-foreground">
                                    类目编号
                                </dt>
                                <dd className="num min-w-0 break-all text-right text-[13px]">
                                    {row.stableNo}
                                </dd>
                            </div>
                            <div className="space-y-2">
                                <dt className="text-xs text-muted-foreground">
                                    描述
                                </dt>
                                <dd className="whitespace-pre-wrap break-words leading-6">
                                    {description || "暂无描述"}
                                </dd>
                            </div>
                        </dl>
                    </section>
                    <section className="space-y-4" aria-label="历史版本">
                        <div className="flex items-center justify-between gap-3">
                            <h3 className="font-medium">历史版本</h3>
                            {detail ? (
                                <span className="text-xs text-muted-foreground">
                                    共 {detail.revisionTimeline.length} 个版本
                                </span>
                            ) : null}
                        </div>
                        {detailQuery.isPending ? (
                            <p
                                role="status"
                                className="text-xs text-muted-foreground"
                            >
                                正在加载详情与历史版本…
                            </p>
                        ) : detailQuery.isError || !detail ? (
                            <div role="alert" className="space-y-3">
                                <p className="text-xs text-muted-foreground">
                                    详情与历史版本加载失败，请重试。
                                </p>
                                <Button
                                    id={`${prefix}-retry`}
                                    size="sm"
                                    variant="outline"
                                    onClick={() => void detailQuery.refetch()}
                                >
                                    重新加载
                                </Button>
                            </div>
                        ) : detail.revisionTimeline.length === 0 ? (
                            <p className="text-xs text-muted-foreground">
                                暂无版本记录
                            </p>
                        ) : (
                            <ol className="space-y-5">
                                {detail.revisionTimeline.map((revision) => (
                                    <li
                                        key={revision.id}
                                        className="space-y-2 border-l-2 border-border pl-4"
                                    >
                                        <div className="flex flex-wrap items-center gap-2">
                                            <span className="num font-medium">
                                                v{revision.revisionNo}
                                            </span>
                                            {revision.isCurrent ? (
                                                <Badge variant="secondary">
                                                    当前版本
                                                </Badge>
                                            ) : null}
                                            <span className="text-xs text-muted-foreground">
                                                {revision.lifecycleAtRevision ===
                                                "ENABLED"
                                                    ? "启用"
                                                    : "停用"}
                                            </span>
                                        </div>
                                        <p className="num text-xs text-muted-foreground">
                                            记录时间：
                                            {new Date(
                                                revision.effectiveFrom,
                                            ).toLocaleString("zh-CN", {
                                                hour12: false,
                                            })}
                                        </p>
                                        <p className="whitespace-pre-wrap break-words text-[13px] leading-6">
                                            {revision.descriptionSnapshot ||
                                                "暂无描述"}
                                        </p>
                                    </li>
                                ))}
                            </ol>
                        )}
                    </section>
                </div>
            ) : null}
        </QuickPreviewSheet>
    )
}
