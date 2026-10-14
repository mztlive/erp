"use client"

import * as React from "react"
import { TruckIcon } from "lucide-react"

import { Button } from "@/components/ui/button"
import {
    Card,
    CardAction,
    CardContent,
    CardDescription,
    CardHeader,
    CardTitle,
} from "@/components/ui/card"

import { useSupplierDeliveryTemplatesQuery } from "../../hooks/queries"
import type { SupplierOfferingView } from "../../types"
import { AssignDeliveryTemplateDialog } from "./assign-delivery-template-dialog"
import { DeliveryRulesPreview } from "./delivery-rules-preview"
import { DeliveryTemplatesDialog } from "./delivery-templates-dialog"

/** 生效模板来自哪里，用一句话说清楚。 */
function sourceText(offering: SupplierOfferingView): string {
    const current = offering.delivery_template
    const name = current?.template_name ?? current?.display_name
    if (current?.source === "ASSIGNED") return `已指定模板「${name}」`
    if (current?.source === "SUPPLIER_DEFAULT")
        return `跟随供应商默认模板「${name}」`
    return "未设置模板，按全国配送、代发与集采均包邮判定"
}

/** 供给资料页的配送范围：生效模板、按地区的规则与设置入口。 */
export function OfferingDeliverySection({
    offering,
    canEdit,
}: {
    offering: SupplierOfferingView
    canEdit: boolean
}) {
    const [dialog, setDialog] = React.useState<"assign" | "manage" | null>(null)
    const templates = useSupplierDeliveryTemplatesQuery(offering.supplier_id)
    const current = offering.delivery_template
    const list = templates.data ?? []
    const template =
        list.find((item) => item.id === current?.template_id) ??
        (current?.source === "UNSET"
            ? null
            : (list.find((item) => item.is_default) ?? null))
    const empty = templates.isSuccess && list.length === 0
    return (
        <Card role="region" aria-labelledby="offering-delivery-title">
            <CardHeader>
                <CardTitle
                    id="offering-delivery-title"
                    className="flex items-center gap-2"
                >
                    <TruckIcon
                        className="size-4 text-muted-foreground"
                        aria-hidden
                    />
                    配送范围
                </CardTitle>
                <CardDescription>{sourceText(offering)}</CardDescription>
                {canEdit ? (
                    <CardAction className="flex flex-wrap justify-end gap-2">
                        <Button
                            id="supplier-offering-detail-delivery-manage"
                            type="button"
                            size="sm"
                            variant="ghost"
                            onClick={() => setDialog("manage")}
                        >
                            管理模板
                        </Button>
                        {empty ? null : (
                            <Button
                                id="supplier-offering-detail-delivery-template"
                                type="button"
                                size="sm"
                                variant="outline"
                                onClick={() => setDialog("assign")}
                            >
                                更换配送模板
                            </Button>
                        )}
                    </CardAction>
                ) : null}
            </CardHeader>
            <CardContent className="space-y-4">
                {templates.isError ? (
                    <p className="text-sm text-muted-foreground">
                        配送规则读取失败，请稍后刷新。
                    </p>
                ) : templates.isPending ? (
                    <p className="text-sm text-muted-foreground">
                        正在读取配送规则…
                    </p>
                ) : (
                    <DeliveryRulesPreview template={template} />
                )}
                {empty && canEdit ? (
                    <div className="flex flex-wrap items-center justify-between gap-3 rounded-lg border border-dashed px-4 py-3">
                        <p className="text-sm text-muted-foreground">
                            该供应商还没有配送模板。新建后可标注不配送地区、集采运费另计地区。
                        </p>
                        <Button
                            id="supplier-offering-detail-delivery-create"
                            type="button"
                            size="sm"
                            onClick={() => setDialog("manage")}
                        >
                            新建配送模板
                        </Button>
                    </div>
                ) : null}
            </CardContent>
            {dialog === "assign" ? (
                <AssignDeliveryTemplateDialog
                    offerings={[offering]}
                    onOpenChange={(open) => {
                        if (!open) setDialog(null)
                    }}
                />
            ) : null}
            {dialog === "manage" ? (
                <DeliveryTemplatesDialog
                    open
                    onOpenChange={(open) => {
                        if (!open) setDialog(null)
                    }}
                    initialSupplierId={offering.supplier_id}
                    canEdit={canEdit}
                />
            ) : null}
        </Card>
    )
}
