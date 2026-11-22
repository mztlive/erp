"use client"

import { EyeIcon, EyeOffIcon } from "lucide-react"

import { DetailWorkspaceSection } from "@/components/business"
import { Badge } from "@/components/ui/badge"
import { Button } from "@/components/ui/button"
import { LoadingButton } from "@/components/ui/loading-button"
import { DescriptionList } from "@/components/ui/description-list"
import type { SupplierOrderDetailView } from "@/features/supplier-orders/types"
import { formatDateTime } from "@/lib/datetime"
import { Item } from "@/features/supplier-orders/components/supplier-order-preview-center-section-parts"

export function FulfillmentSection({
    logistics,
    address,
    statusHistory,
    canReveal,
    revealPending,
    onReveal,
    onHide,
}: {
    logistics: SupplierOrderDetailView["logistics"]
    address: SupplierOrderDetailView["address"]
    statusHistory: SupplierOrderDetailView["statusHistory"]
    canReveal: boolean
    revealPending: boolean
    onReveal: () => void
    onHide: () => void
}) {
    return (
        <>
            <DetailWorkspaceSection
                title="履约与物流"
                width="form"
                subtitle="接单、发货与地址"
            >
                <DescriptionList className="mb-4 gap-y-3">
                    <Item
                        label="接单时间"
                        value={
                            logistics.acceptedAt
                                ? formatDateTime(
                                      logistics.acceptedAt,
                                      "fullIntl",
                                      "passthrough",
                                  )
                                : "—"
                        }
                    />
                    <Item
                        label="发货时间"
                        value={
                            logistics.shippedAt
                                ? formatDateTime(
                                      logistics.shippedAt,
                                      "fullIntl",
                                      "passthrough",
                                  )
                                : "—"
                        }
                    />
                    <Item
                        label="完成时间"
                        value={
                            logistics.completedAt
                                ? formatDateTime(
                                      logistics.completedAt,
                                      "fullIntl",
                                      "passthrough",
                                  )
                                : "—"
                        }
                    />
                    <Item label="承运商" value={logistics.carrier ?? "—"} />
                    <Item
                        label="物流号"
                        value={
                            logistics.trackingNo ? (
                                <span className="num">
                                    {logistics.trackingNo}
                                </span>
                            ) : (
                                "—"
                            )
                        }
                    />
                </DescriptionList>
            </DetailWorkspaceSection>
            <DetailWorkspaceSection
                title="收货信息"
                width="form"
                subtitle="默认打码；仅履约所需角色可短时揭示，揭示写入审计。"
                contentClassName="space-y-2 text-body-compact"
            >
                <div>
                    收件人：
                    {address.recipientRevealed ?? address.recipientMasked}
                </div>
                <div>
                    手机：
                    <span className="num">
                        {address.phoneRevealed ?? address.phoneMasked}
                    </span>
                </div>
                <div>地址：{address.revealed ?? address.masked}</div>
                {address.auditNote ? (
                    <p className="text-xs text-muted-foreground">
                        {address.auditNote}
                    </p>
                ) : null}
                <div className="flex gap-2 pt-1">
                    {canReveal && (
                        <LoadingButton
                            id="supplier-order-center-fulfillment-reveal"
                            loading={revealPending}
                            type="button"
                            size="sm"
                            variant="outline"
                            disabled={!canReveal || revealPending}
                            onClick={onReveal}
                        >
                            {!revealPending && <EyeIcon className="size-3.5" />}
                            {revealPending ? "揭示中…" : "短时揭示"}
                        </LoadingButton>
                    )}
                    {address.revealed ? (
                        <Button
                            id="supplier-order-center-fulfillment-hide"
                            type="button"
                            size="sm"
                            variant="ghost"
                            onClick={onHide}
                        >
                            <EyeOffIcon className="size-3.5" />
                            立即隐藏
                        </Button>
                    ) : null}
                </div>
            </DetailWorkspaceSection>
            <DetailWorkspaceSection title="状态历史" width="form">
                {statusHistory.length === 0 ? (
                    <p className="text-xs text-muted-foreground">
                        暂无状态历史
                    </p>
                ) : (
                    <ul className="space-y-2">
                        {statusHistory.map((h) => (
                            <li
                                key={h.id}
                                className="border-t border-border/60 py-2 text-xs"
                            >
                                <div className="flex flex-wrap gap-2">
                                    <Badge variant="secondary">{h.track}</Badge>
                                    <span>
                                        {h.fromLabel} → {h.toLabel}
                                    </span>
                                    <span className="text-muted-foreground">
                                        {formatDateTime(
                                            h.at,
                                            "fullIntl",
                                            "passthrough",
                                        )}{" "}
                                        · {h.source}
                                    </span>
                                </div>
                                {h.note ? (
                                    <p className="mt-1 text-muted-foreground">
                                        {h.note}
                                    </p>
                                ) : null}
                            </li>
                        ))}
                    </ul>
                )}
            </DetailWorkspaceSection>
        </>
    )
}
