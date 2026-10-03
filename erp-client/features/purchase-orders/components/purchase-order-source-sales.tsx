"use client"

import { useRef, useState } from "react"
import type { ColumnDef } from "@tanstack/react-table"

import {
    BusinessStatusBadge,
    DataTable,
    MoneyValue,
    QuantityValue,
    QuickPreviewSheet,
} from "@/components/business"
import { Button } from "@/components/ui/button"
import { LoadingButton } from "@/components/ui/loading-button"
import { PurchaseSalesMaterialPreview } from "@/features/purchase-orders/components/purchase-sales-material-preview"
import { useDownloadPurchaseSalesMaterial } from "@/features/purchase-orders/hooks/use-purchase-sales-materials"
import type {
    PurchaseOrderCenterView,
    PurchaseSourceSalesMaterialView,
    PurchaseSourceSalesOrderView,
} from "@/features/purchase-orders/types"
import { getErrorMessage } from "@/lib/api"
import { toAutomationIdSegment } from "@/lib/automation-id"

type SalesLine = PurchaseSourceSalesOrderView["lines"][number]

const SALES_COLUMNS: ColumnDef<SalesLine>[] = [
    {
        id: "item",
        accessorKey: "itemName",
        header: "销售项目",
        meta: { label: "销售项目", width: "flex" },
        cell: ({ row }) => (
            <div className="whitespace-normal">
                <div className="font-medium">{row.original.itemName}</div>
                <div className="text-xs text-muted-foreground">
                    销售明细 {row.original.lineNo}
                    {row.original.specification
                        ? ` · ${row.original.specification}`
                        : ""}
                </div>
            </div>
        ),
    },
    {
        id: "quantity",
        accessorKey: "quantity",
        header: "销售数量",
        meta: {
            label: "销售数量",
            width: "quantity",
            align: "end",
            numeric: true,
        },
        cell: ({ row }) => (
            <QuantityValue
                value={row.original.quantity}
                unit={row.original.unit}
            />
        ),
    },
    {
        id: "price",
        accessorKey: "unitPriceGross",
        header: "客户成交含税单价",
        meta: {
            label: "客户成交含税单价",
            width: "amount",
            align: "end",
            numeric: true,
        },
        cell: ({ row }) => <MoneyValue value={row.original.unitPriceGross} />,
    },
    {
        id: "gross",
        accessorKey: "grossAmount",
        header: "销售行含税金额",
        meta: {
            label: "销售行含税金额",
            width: "amount",
            align: "end",
            numeric: true,
        },
        cell: ({ row }) => <MoneyValue value={row.original.grossAmount} />,
    },
]

const READABLE_TYPES = new Set([
    "application/pdf",
    "image/jpeg",
    "image/png",
    "image/webp",
])

function materialLabel(kind: string) {
    if (kind === "CONTRACT") return "合同"
    if (kind === "EVIDENCE") return "开单凭证"
    return "关联资料"
}

/** 采购单范围内查看来源销售版本及材料，无须普通销售或合同详情权限。 */
export function PurchaseOrderSourceSales({
    order,
}: {
    order: PurchaseOrderCenterView
}) {
    const [open, setOpen] = useState(false)
    const triggerRef = useRef<HTMLButtonElement>(null)
    const source = order.sourceSalesOrder
    const idPrefix = `procurement-orders-source-sales-${toAutomationIdSegment(order.identity.purchaseOrderId)}`

    if (!source) {
        return (
            <p className="text-xs text-muted-foreground">
                关联销售资料暂不可查看，请联系经办人核对。
            </p>
        )
    }

    const close = () => setOpen(false)
    return (
        <>
            <Button
                id={`${idPrefix}-open`}
                ref={triggerRef}
                type="button"
                variant="outline"
                size="sm"
                onClick={() => setOpen(true)}
            >
                查看销售单及合同
            </Button>
            <QuickPreviewSheet
                idPrefix={`${idPrefix}-sheet`}
                open={open}
                onOpenChange={(next) => {
                    setOpen(next)
                    if (!next) triggerRef.current?.focus()
                }}
                size="detail"
                identity={
                    <span className="num">销售单号：{source.salesOrderNo}</span>
                }
                title={source.customerName || "关联销售单"}
                summary={
                    <div className="flex flex-wrap items-center gap-2">
                        <span className="text-xs text-muted-foreground">
                            销售单当前状态
                        </span>
                        <BusinessStatusBadge
                            context="preview"
                            label={source.statusLabel}
                            tone={source.statusTone}
                        />
                        <span className="text-xs text-muted-foreground">
                            采购关联销售版本 v{source.revisionNo}
                        </span>
                    </div>
                }
                footer={
                    <Button
                        id={`${idPrefix}-footer-close`}
                        type="button"
                        variant="outline"
                        onClick={close}
                    >
                        关闭
                    </Button>
                }
            >
                {open ? (
                    <SourceSalesBody
                        key={source.revisionId}
                        purchaseOrderId={order.identity.purchaseOrderId}
                        source={source}
                        idPrefix={idPrefix}
                    />
                ) : null}
            </QuickPreviewSheet>
        </>
    )
}

function SourceSalesBody({
    purchaseOrderId,
    source,
    idPrefix,
}: {
    purchaseOrderId: string
    source: PurchaseSourceSalesOrderView
    idPrefix: string
}) {
    const [preview, setPreview] =
        useState<PurchaseSourceSalesMaterialView | null>(null)
    const download = useDownloadPurchaseSalesMaterial(purchaseOrderId)
    return (
        <div className="min-h-0 flex-1 space-y-6 overflow-y-auto px-7 py-6">
            <dl className="grid gap-4 text-sm sm:grid-cols-2">
                <div>
                    <dt className="text-xs text-muted-foreground">
                        销售含税金额
                    </dt>
                    <dd className="mt-1 text-xl font-semibold">
                        <MoneyValue value={source.totals.gross} />
                    </dd>
                </div>
                <div>
                    <dt className="text-xs text-muted-foreground">
                        销售负责人
                    </dt>
                    <dd className="mt-1">
                        {source.salesOwnerName || "未标注"}
                    </dd>
                </div>
                <div>
                    <dt className="text-xs text-muted-foreground">关联合同</dt>
                    <dd className="num mt-1 break-all">
                        {source.contractNo || "该销售版本未关联合同"}
                    </dd>
                </div>
                <div>
                    <dt className="text-xs text-muted-foreground">销售版本</dt>
                    <dd className="num mt-1">v{source.revisionNo}</dd>
                </div>
            </dl>
            <section className="space-y-3" aria-label="关联销售明细">
                <h2 className="text-sm font-semibold">销售明细</h2>
                <p className="text-xs leading-5 text-muted-foreground">
                    以下为采购关联销售版本的完整明细，销售金额按整张销售单展示。
                </p>
                <DataTable
                    id={`${idPrefix}-lines-table`}
                    data={[...source.lines]}
                    columns={SALES_COLUMNS}
                    getRowId={(row) => row.salesOrderRevisionLineId}
                    rowCount={source.lines.length}
                    rowLabel={(row) => row.itemName}
                    caption="关联销售明细"
                    density="compact"
                    showPagination={false}
                    showColumnVisibility={false}
                    emptyTitle="该销售版本未保留明细"
                />
            </section>
            <section
                className="space-y-3 border-t pt-5"
                aria-label="合同及开单凭证"
            >
                <h2 className="text-sm font-semibold">合同及开单凭证</h2>
                {source.materials.length ? (
                    <ul className="space-y-3">
                        {source.materials.map((file) => {
                            const filePrefix = `${idPrefix}-material-${toAutomationIdSegment(file.kind)}-${toAutomationIdSegment(file.fileAssetId)}`
                            return (
                                <li
                                    key={`${file.kind}:${file.fileAssetId}`}
                                    className="flex flex-wrap items-center justify-between gap-3"
                                >
                                    <div className="min-w-0">
                                        <p className="break-all text-sm">
                                            {file.fileName}
                                        </p>
                                        <p className="text-xs text-muted-foreground">
                                            {materialLabel(file.kind)}
                                        </p>
                                    </div>
                                    <div className="flex items-center gap-2">
                                        {READABLE_TYPES.has(
                                            file.contentType,
                                        ) ? (
                                            <Button
                                                id={`${filePrefix}-view`}
                                                type="button"
                                                variant="ghost"
                                                size="sm"
                                                onClick={() => setPreview(file)}
                                            >
                                                查看
                                            </Button>
                                        ) : null}
                                        <LoadingButton
                                            id={`${filePrefix}-download`}
                                            type="button"
                                            variant="outline"
                                            size="sm"
                                            loading={
                                                download.isPending &&
                                                download.variables
                                                    ?.fileAssetId ===
                                                    file.fileAssetId
                                            }
                                            disabled={download.isPending}
                                            onClick={() =>
                                                download.mutate(file)
                                            }
                                        >
                                            下载
                                        </LoadingButton>
                                    </div>
                                </li>
                            )
                        })}
                    </ul>
                ) : (
                    <p className="text-sm text-muted-foreground">
                        {source.materialsUnavailable
                            ? "关联文件暂不可读取，请联系经办人核对。"
                            : "该销售版本未保留可读取的合同或开单凭证，历史资料可能未保存。"}
                    </p>
                )}
                {download.isError ? (
                    <p role="alert" className="text-sm text-destructive">
                        {getErrorMessage(
                            download.error,
                            "附件下载失败，请重试",
                        )}
                    </p>
                ) : null}
            </section>
            {preview ? (
                <PurchaseSalesMaterialPreview
                    key={preview.fileAssetId}
                    purchaseOrderId={purchaseOrderId}
                    salesRevisionId={source.revisionId}
                    file={preview}
                    idPrefix={`${idPrefix}-material-${toAutomationIdSegment(preview.kind)}-${toAutomationIdSegment(preview.fileAssetId)}`}
                    onClose={() => setPreview(null)}
                />
            ) : null}
        </div>
    )
}
