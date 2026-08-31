"use client"

import Link from "next/link"

import {
    BusinessFailureState,
    BusinessStatusBadge,
    QuickPreviewSheet,
} from "@/components/business"
import { Badge } from "@/components/ui/badge"
import { LoaderCircleIcon } from "lucide-react"

import { Button } from "@/components/ui/button"
import { Separator } from "@/components/ui/separator"
import type {
    BalanceDetailView,
    StockBalanceRow,
} from "@/features/inventory/types"
import { toAutomationIdSegment } from "@/lib/automation-id"
import { formatDateTime } from "@/lib/datetime"
import { workspaceLabel } from "@/lib/ui-text"
import type { WorkspaceId } from "@/lib/workspace-registry"

type InventoryBalancePreviewProps = {
    open: boolean
    detail: BalanceDetailView | null | undefined
    isPending: boolean
    isCreating?: boolean
    onClose: () => void
    onViewMovements: (detail: BalanceDetailView) => void
    onStartAdjustment: (row: StockBalanceRow) => Promise<void>
    onOpenAdjustment?: (adjustmentId: string) => void
}

function InventoryBalancePreview({
    open,
    detail,
    isPending,
    isCreating = false,
    onClose,
    onViewMovements,
    onStartAdjustment,
    onOpenAdjustment,
}: InventoryBalancePreviewProps) {
    return (
        <QuickPreviewSheet
            id="inventory-balance-preview-sheet"
            open={open}
            onOpenChange={(nextOpen) => {
                if (!nextOpen) onClose()
            }}
            size="preview"
            title={detail ? `${detail.balance.skuName}` : "余额详情"}
            identity={
                detail ? (
                    <span className="num text-sm">
                        {detail.balance.warehouseCode} ·{" "}
                        {detail.balance.skuCode}
                    </span>
                ) : null
            }
            summary={
                detail ? (
                    <div className="flex flex-wrap items-center gap-2">
                        <BusinessStatusBadge
                            context="preview"
                            label={detail.balance.statusLabel}
                            tone={detail.balance.statusTone}
                        />
                        <Badge variant="secondary">自有实物</Badge>
                    </div>
                ) : null
            }
            footer={
                detail ? (
                    <>
                        <Button
                            id="inventory-balance-preview-close"
                            type="button"
                            variant="outline"
                            onClick={onClose}
                        >
                            关闭
                        </Button>
                        <Button
                            id="inventory-balance-preview-view-movements"
                            type="button"
                            variant="outline"
                            onClick={() => onViewMovements(detail)}
                        >
                            查看全部流水
                        </Button>
                        {detail.balance.allowedActions.includes(
                            "CREATE_ADJUSTMENT",
                        ) ? (
                            <Button
                                id="inventory-balance-preview-start-adjustment"
                                type="button"
                                disabled={isCreating}
                                title={
                                    detail.balance.actionBlockers.find(
                                        (b) => b.action === "CREATE_ADJUSTMENT",
                                    )?.message
                                }
                                onClick={() =>
                                    void onStartAdjustment(detail.balance)
                                }
                            >
                                {isCreating ? (
                                    <LoaderCircleIcon
                                        data-icon="inline-start"
                                        aria-hidden="true"
                                        className="animate-spin"
                                    />
                                ) : null}
                                {isCreating ? "创建中…" : "发起库存调整"}
                            </Button>
                        ) : null}
                    </>
                ) : null
            }
        >
            {isPending ? (
                <div className="space-y-3 p-1">
                    <div className="h-24 animate-pulse rounded-xl bg-muted" />
                    <div className="h-40 animate-pulse rounded-xl bg-muted" />
                </div>
            ) : detail ? (
                <div className="flex flex-col gap-4">
                    <div className="grid grid-cols-3 gap-2 rounded-xl border bg-card p-3">
                        <div>
                            <div className="text-xs text-muted-foreground">
                                账面现存
                            </div>
                            <div className="num text-base font-semibold">
                                {detail.balance.onHandQuantity}
                                <span className="ml-1 text-xs font-normal text-muted-foreground">
                                    {detail.balance.baseUnit}
                                </span>
                            </div>
                        </div>
                        <div>
                            <div className="text-xs text-muted-foreground">
                                有效预占
                            </div>
                            <div className="num text-base font-semibold">
                                {detail.balance.reservedQuantity}
                                <span className="ml-1 text-xs font-normal text-muted-foreground">
                                    {detail.balance.baseUnit}
                                </span>
                            </div>
                        </div>
                        <div>
                            <div className="text-xs text-muted-foreground">
                                可用数量
                            </div>
                            <div className="num text-base font-semibold text-primary">
                                {detail.balance.availableQuantity}
                                <span className="ml-1 text-xs font-normal text-muted-foreground">
                                    {detail.balance.baseUnit}
                                </span>
                            </div>
                            <div className="text-2xs text-muted-foreground">
                                系统计算
                            </div>
                        </div>
                    </div>

                    <section className="space-y-2">
                        <h3 className="text-sm font-medium">最近流水</h3>
                        {detail.recentMovements.length === 0 ? (
                            <p className="text-xs text-muted-foreground">
                                暂无流水
                            </p>
                        ) : (
                            <ul className="space-y-2">
                                {detail.recentMovements.map((m) => (
                                    <li
                                        key={m.movementId}
                                        className="rounded-lg border px-3 py-2 text-sm"
                                    >
                                        <div className="flex items-start justify-between gap-2">
                                            <div>
                                                <div className="font-medium">
                                                    {m.movementTypeLabel}
                                                    <span className="ml-2 text-xs text-muted-foreground">
                                                        {m.direction ===
                                                        "increase"
                                                            ? "增加"
                                                            : "减少"}
                                                    </span>
                                                </div>
                                                <div className="num text-xs text-muted-foreground">
                                                    {formatDateTime(
                                                        m.occurredAt,
                                                        "full",
                                                        "passthrough",
                                                    )}{" "}
                                                    · {m.recordedByLabel}
                                                </div>
                                            </div>
                                            <div className="num shrink-0 font-medium">
                                                {m.quantity} {m.baseUnit}
                                            </div>
                                        </div>
                                        {m.sourceHref ? (
                                            <Button
                                                id={`inventory-balance-preview-movement-${toAutomationIdSegment(m.movementId)}-source`}
                                                type="button"
                                                variant="link"
                                                size="xs"
                                                className="mt-1 h-auto px-0"
                                                render={
                                                    <Link href={m.sourceHref} />
                                                }
                                            >
                                                来源 {m.sourceDocumentNo}
                                            </Button>
                                        ) : (
                                            <div className="num mt-1 text-xs text-muted-foreground">
                                                来源 {m.sourceDocumentNo}
                                            </div>
                                        )}
                                    </li>
                                ))}
                            </ul>
                        )}
                    </section>

                    <Separator />

                    <section className="space-y-2">
                        <h3 className="text-sm font-medium">来源单据</h3>
                        {detail.sourceDocuments.length === 0 ? (
                            <p className="text-xs text-muted-foreground">
                                无关联来源
                            </p>
                        ) : (
                            <ul className="space-y-1.5">
                                {detail.sourceDocuments.map((doc) => (
                                    <li
                                        key={`${doc.documentType}:${doc.documentId}`}
                                        className="flex items-center justify-between gap-2 text-sm"
                                    >
                                        <span>
                                            {doc.label}
                                            <span className="num ml-2 text-muted-foreground">
                                                {doc.documentNo}
                                            </span>
                                        </span>
                                        {doc.href ? (
                                            <Button
                                                id={`inventory-balance-preview-doc-${toAutomationIdSegment(doc.documentId)}-open`}
                                                type="button"
                                                variant="outline"
                                                size="xs"
                                                render={
                                                    <Link href={doc.href} />
                                                }
                                            >
                                                {doc.workspaceId
                                                    ? workspaceLabel(
                                                          doc.workspaceId as WorkspaceId,
                                                      )
                                                    : "打开"}
                                            </Button>
                                        ) : null}
                                    </li>
                                ))}
                            </ul>
                        )}
                    </section>

                    <Separator />

                    <section className="space-y-2">
                        <h3 className="text-sm font-medium">有效预占</h3>
                        {detail.reservations.length === 0 ? (
                            <p className="text-xs text-muted-foreground">
                                无有效预占
                            </p>
                        ) : (
                            <ul className="space-y-2">
                                {detail.reservations.map((r) => (
                                    <li
                                        key={r.reservationId}
                                        className="rounded-lg border px-3 py-2 text-sm"
                                    >
                                        <div className="flex items-start justify-between gap-2">
                                            <div>
                                                <div className="num font-medium">
                                                    {r.salesOrderNo}
                                                </div>
                                                <div className="text-xs text-muted-foreground">
                                                    {r.salesOrderLineLabel}
                                                </div>
                                                <div className="mt-1 text-xs">
                                                    剩余{" "}
                                                    <span className="num">
                                                        {r.remainingQuantity}{" "}
                                                        {r.baseUnit}
                                                    </span>
                                                    {" · "}
                                                    建立 {
                                                        r.establishedQuantity
                                                    }{" "}
                                                    · 消耗 {r.consumedQuantity}
                                                </div>
                                            </div>
                                            <BusinessStatusBadge
                                                context="list"
                                                label={r.statusLabel}
                                                tone={r.statusTone}
                                            />
                                        </div>
                                        {r.fulfillmentHref ? (
                                            <Button
                                                id={`inventory-balance-preview-reservation-${toAutomationIdSegment(r.reservationId)}-fulfillment`}
                                                type="button"
                                                variant="link"
                                                size="xs"
                                                className="mt-1 h-auto px-0"
                                                render={
                                                    <Link
                                                        href={r.fulfillmentHref}
                                                    />
                                                }
                                            >
                                                打开收货与发货
                                            </Button>
                                        ) : null}
                                        {/* 无「释放预占」入口 */}
                                    </li>
                                ))}
                            </ul>
                        )}
                    </section>

                    {detail.pendingAdjustments.length > 0 ? (
                        <>
                            <Separator />
                            <section className="space-y-2">
                                <h3 className="text-sm font-medium">
                                    进行中的调整
                                </h3>
                                <ul className="space-y-1 text-sm">
                                    {detail.pendingAdjustments.map((a) => (
                                        <li
                                            key={a.adjustmentId}
                                            className="flex justify-between"
                                        >
                                            {onOpenAdjustment ? (
                                                <Button
                                                    id={`inventory-balance-preview-adjustment-${toAutomationIdSegment(a.adjustmentId)}-open`}
                                                    type="button"
                                                    variant="link"
                                                    size="xs"
                                                    className="num h-auto px-0"
                                                    onClick={() =>
                                                        onOpenAdjustment(
                                                            a.adjustmentId,
                                                        )
                                                    }
                                                >
                                                    {a.adjustmentNo}
                                                </Button>
                                            ) : (
                                                <span className="num">
                                                    {a.adjustmentNo}
                                                </span>
                                            )}
                                            <BusinessStatusBadge
                                                context="list"
                                                label={a.statusLabel}
                                                tone={a.statusTone}
                                            />
                                        </li>
                                    ))}
                                </ul>
                            </section>
                        </>
                    ) : null}

                    <p className="text-tiny leading-relaxed text-muted-foreground">
                        查询于{" "}
                        {formatDateTime(
                            detail.queriedAt,
                            "full",
                            "passthrough",
                        )}
                        。页面不提供编辑库存数量或直接释放预占；纠错须走调整单。
                    </p>
                </div>
            ) : (
                <BusinessFailureState
                    kind="business"
                    title="无法加载余额详情"
                    description="余额可能已不存在，或权限已变化。"
                />
            )}
        </QuickPreviewSheet>
    )
}

export { InventoryBalancePreview }
