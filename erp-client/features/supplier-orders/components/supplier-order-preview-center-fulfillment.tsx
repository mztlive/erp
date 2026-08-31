"use client"

import { EyeIcon, EyeOffIcon, Loader2Icon } from "lucide-react"

import { DocumentSection, surfaceInsetClassName } from "@/components/business"
import { Badge } from "@/components/ui/badge"
import { Button } from "@/components/ui/button"
import {
    Card,
    CardContent,
    CardDescription,
    CardHeader,
    CardTitle,
} from "@/components/ui/card"
import { DescriptionList } from "@/components/ui/description-list"
import { Separator } from "@/components/ui/separator"
import type { SupplierOrderDetailView } from "@/features/supplier-orders/types"
import { formatDateTime } from "@/lib/datetime"
import { cn } from "@/lib/utils"
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
        <DocumentSection title="履约与物流" description="接单、发货与地址">
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
                            <span className="num">{logistics.trackingNo}</span>
                        ) : (
                            "—"
                        )
                    }
                />
            </DescriptionList>

            <Card
                size="sm"
                className={cn(surfaceInsetClassName, "shadow-none ring-0")}
            >
                <CardHeader className="rounded-t-lg border-b border-grid pb-2">
                    <CardTitle className="text-sm">收货信息</CardTitle>
                    <CardDescription className="text-xs">
                        默认打码；仅履约所需角色可短时揭示，揭示写入审计。
                    </CardDescription>
                </CardHeader>
                <CardContent className="space-y-2 text-sm">
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
                        <Button
                            id="supplier-order-center-fulfillment-reveal"
                            type="button"
                            size="sm"
                            variant="outline"
                            disabled={!canReveal || revealPending}
                            onClick={onReveal}
                        >
                            {revealPending ? (
                                <Loader2Icon
                                    className="size-3.5 animate-spin"
                                    aria-hidden="true"
                                />
                            ) : (
                                <EyeIcon className="size-3.5" />
                            )}
                            {revealPending ? "揭示中…" : "短时揭示"}
                        </Button>
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
                </CardContent>
            </Card>

            <Separator className="my-4" />
            <h4 className="mb-2 text-xs font-semibold">状态历史</h4>
            {statusHistory.length === 0 ? (
                <p className="text-sm text-muted-foreground">暂无状态历史</p>
            ) : (
                <ul className="space-y-2">
                    {statusHistory.map((h) => (
                        <li
                            key={h.id}
                            className={cn(
                                surfaceInsetClassName,
                                "px-3 py-2 text-xs",
                            )}
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
        </DocumentSection>
    )
}
