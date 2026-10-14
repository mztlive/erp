"use client"

import * as React from "react"
import { z } from "zod"

import { useAppForm } from "@/components/form"
import { Alert, AlertDescription, AlertTitle } from "@/components/ui/alert"
import { Badge } from "@/components/ui/badge"
import { Button } from "@/components/ui/button"
import {
    Dialog,
    DialogContent,
    DialogDescription,
    DialogFooter,
    DialogHeader,
    DialogTitle,
} from "@/components/ui/dialog"
import { useAccountProfileQuery } from "@/features/auth/queries"
import { useWaivePurchaseFreightMutation } from "@/features/purchase-orders/hooks/queries"
import type { PurchaseOrderCenterView } from "@/features/purchase-orders/types"
import { getErrorMessage } from "@/lib/api/errors"
import { toAutomationIdSegment } from "@/lib/automation-id"
import { formatDateTime } from "@/lib/datetime"
import { hasPermission } from "@/lib/permissions"

const reasonSchema = z.object({
    reason: z
        .string()
        .trim()
        .min(1, "请填写供应商不收运费的原因")
        .max(200, "原因不超过 200 字"),
})

type FreightLine = NonNullable<
    PurchaseOrderCenterView["delivery"]
>["lines"][number]

/**
 * 集采运费另计的跟进提示：待追加的商品可标记供应商不收运费；
 * 物流费用采购在销售单详情追加。
 */
export function PurchaseOrderFreightNotice({
    order,
}: {
    order: PurchaseOrderCenterView
}) {
    const profileQuery = useAccountProfileQuery()
    const [waiving, setWaiving] = React.useState<FreightLine | null>(null)
    const lines =
        order.delivery?.lines.filter((line) => line.freightExtra) ?? []
    if (!lines.length) return null
    const canWaive = hasPermission(
        profileQuery.data?.permissions,
        "purchase_order:update",
    )
    const nameOf = (salesOrderLineId: string) =>
        order.currentContent.lines.find(
            (line) => line.salesOrderLineId === salesOrderLineId,
        )?.itemName ?? "商品"
    const pending = lines.some((line) => line.freightPending)
    return (
        <Alert variant={pending ? "warning" : "default"}>
            <AlertTitle>{pending ? "运费待追加" : "运费另计已处理"}</AlertTitle>
            <AlertDescription className="space-y-2">
                <p>
                    以下商品为集采运费另计。供应商收取运费时，在来源销售单详情追加物流费用采购；供应商不收运费时在此标记。
                </p>
                <ul className="space-y-1.5">
                    {lines.map((line) => (
                        <li
                            key={line.salesOrderLineId}
                            className="flex flex-wrap items-center gap-2"
                        >
                            <span>{nameOf(line.salesOrderLineId)}</span>
                            {line.templateName ? (
                                <span className="text-xs text-muted-foreground">
                                    配送模板 {line.templateName}
                                </span>
                            ) : null}
                            {line.waiver ? (
                                <Badge variant="neutral">
                                    供应商不收运费：{line.waiver.reason}（
                                    {formatDateTime(line.waiver.waivedAt)}）
                                </Badge>
                            ) : line.freightPending ? (
                                <>
                                    <Badge variant="warning">待追加</Badge>
                                    {canWaive ? (
                                        <Button
                                            id={`purchase-order-freight-waive-${toAutomationIdSegment(line.salesOrderLineId)}`}
                                            type="button"
                                            size="sm"
                                            variant="ghost"
                                            onClick={() => setWaiving(line)}
                                        >
                                            标记供应商不收运费
                                        </Button>
                                    ) : null}
                                </>
                            ) : (
                                <Badge variant="success">已追加物流费用</Badge>
                            )}
                        </li>
                    ))}
                </ul>
            </AlertDescription>
            <WaiveFreightDialog
                purchaseOrderId={order.identity.purchaseOrderId}
                line={waiving}
                itemName={waiving ? nameOf(waiving.salesOrderLineId) : ""}
                onClose={() => setWaiving(null)}
            />
        </Alert>
    )
}

function WaiveFreightDialog({
    purchaseOrderId,
    line,
    itemName,
    onClose,
}: {
    purchaseOrderId: string
    line: FreightLine | null
    itemName: string
    onClose: () => void
}) {
    const mutation = useWaivePurchaseFreightMutation()
    const [failure, setFailure] = React.useState<string | null>(null)
    const form = useAppForm({
        defaultValues: { reason: "" },
        validators: { onChange: reasonSchema },
        onSubmit: async ({ value }) => {
            if (!line) return
            setFailure(null)
            try {
                await mutation.mutateAsync({
                    purchaseOrderId,
                    salesOrderLineId: line.salesOrderLineId,
                    reason: value.reason,
                })
                onClose()
            } catch (error) {
                setFailure(getErrorMessage(error, "标记失败，请稍后重试。"))
            }
        },
    })
    const open = line !== null
    const wasOpen = React.useRef(false)
    React.useEffect(() => {
        if (open && !wasOpen.current) {
            form.reset({ reason: "" })
            setFailure(null)
        }
        wasOpen.current = open
    }, [form, open])
    return (
        <Dialog open={open} onOpenChange={(next) => (next ? null : onClose())}>
            <DialogContent closeButtonId="purchase-order-freight-waive-close">
                <DialogHeader>
                    <DialogTitle>标记供应商不收运费</DialogTitle>
                    <DialogDescription>
                        {itemName}
                        的运费另计将视为已处理，不再提示追加运费。标记后不可撤销。
                    </DialogDescription>
                </DialogHeader>
                <form
                    className="flex flex-col gap-4"
                    onSubmit={(event) => {
                        event.preventDefault()
                        void form.handleSubmit()
                    }}
                >
                    <form.AppField name="reason">
                        {(field) => (
                            <field.TextareaField
                                id="purchase-order-freight-waive-reason"
                                label="原因"
                                required
                                maxLength={200}
                                placeholder="例如：供应商确认本批运费由其承担"
                            />
                        )}
                    </form.AppField>
                    {failure ? (
                        <p role="alert" className="text-sm text-destructive">
                            {failure}
                        </p>
                    ) : null}
                    <DialogFooter>
                        <Button
                            id="purchase-order-freight-waive-cancel"
                            type="button"
                            variant="outline"
                            onClick={onClose}
                        >
                            取消
                        </Button>
                        <Button
                            id="purchase-order-freight-waive-submit"
                            type="submit"
                            disabled={mutation.isPending}
                        >
                            确认标记
                        </Button>
                    </DialogFooter>
                </form>
            </DialogContent>
        </Dialog>
    )
}
