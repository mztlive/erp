"use client"

import { useState } from "react"
import { Button } from "@/components/ui/button"
import { useAccountProfileQuery } from "@/features/auth/queries"
import { hasPermission } from "@/lib/permissions"
import { getErrorMessage } from "@/lib/api/errors"
import { compactFixed } from "@/lib/fixed-decimal"
import { useDeliveryPackingQuery } from "../hooks/queries"

/** 完成结果、发货详情和验收共用正式地址包裹事实；按权限按需查询。 */
export function DeliveryPackingDetails({
    deliveryId,
    fulfillmentLineId,
    idPrefix,
}: {
    deliveryId: string
    fulfillmentLineId?: string
    idPrefix: string
}) {
    const [open, setOpen] = useState(false)
    const profile = useAccountProfileQuery()
    const authorized = hasPermission(
        profile.data?.permissions,
        "delivery:detail",
    )
    const query = useDeliveryPackingQuery(deliveryId, open && authorized)
    if (!authorized) return null
    const data = query.data
    const addresses = (
        data?.detail.address_snapshot?.destinations ?? []
    ).filter(
        (row) =>
            !fulfillmentLineId ||
            row.items.some(
                (item) => item.delivery_line_id === fulfillmentLineId,
            ),
    )
    const title = (id: string) =>
        data?.operation.lines.find((line) => line.lineId === id)?.itemName ||
        "商品名称未提供"
    return (
        <section className="my-3 space-y-3" aria-label="收货地址与包裹记录">
            <Button
                id={`${idPrefix}-packing-toggle`}
                variant="outline"
                size="sm"
                onClick={() => setOpen(!open)}
                aria-expanded={open}
            >
                {open ? "收起收货地址与包裹" : "查看收货地址与包裹"}
            </Button>
            {open && (
                <div className="space-y-3 rounded-lg border p-3 text-sm">
                    {query.isPending && (
                        <p className="text-muted-foreground">
                            正在读取发货记录…
                        </p>
                    )}
                    {query.isError && (
                        <div>
                            <p>
                                {getErrorMessage(
                                    query.error,
                                    "发货记录读取失败",
                                )}
                            </p>
                            <Button
                                id={`${idPrefix}-packing-retry`}
                                size="sm"
                                variant="ghost"
                                onClick={() => void query.refetch()}
                            >
                                重试
                            </Button>
                        </div>
                    )}
                    {data && !addresses.length && (
                        <p className="text-muted-foreground">
                            该发货记录未按收货地址拆分包裹，仅展示整单物流单号。
                        </p>
                    )}
                    {addresses.map((address) => (
                        <section
                            key={address.key}
                            className="space-y-2 border-b pb-3 last:border-0 last:pb-0"
                        >
                            <p className="font-medium">
                                {address.recipient} · {address.phone}
                            </p>
                            <p className="text-muted-foreground">
                                {address.address}
                            </p>
                            {(data?.detail.packing?.packages ?? [])
                                .filter(
                                    (parcel) =>
                                        parcel.destination_key ===
                                            address.key &&
                                        (!fulfillmentLineId ||
                                            parcel.items.some(
                                                (item) =>
                                                    item.delivery_line_id ===
                                                    fulfillmentLineId,
                                            )),
                                )
                                .map((parcel) => (
                                    <div
                                        key={parcel.id}
                                        className="space-y-1 rounded-md bg-muted/30 p-2"
                                    >
                                        <p>
                                            {parcel.carrier} ·{" "}
                                            {parcel.tracking_no}
                                        </p>
                                        <p className="text-xs text-muted-foreground">
                                            {parcel.items
                                                .filter(
                                                    (item) =>
                                                        !fulfillmentLineId ||
                                                        item.delivery_line_id ===
                                                            fulfillmentLineId,
                                                )
                                                .map(
                                                    (item) =>
                                                        `${title(item.delivery_line_id)} × ${compactFixed(item.quantity)}`,
                                                )
                                                .join(" · ")}
                                        </p>
                                    </div>
                                ))}
                        </section>
                    ))}
                </div>
            )}
        </section>
    )
}
