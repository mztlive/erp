"use client"

import { useState } from "react"
import { XIcon } from "lucide-react"
import { MoneyValue, PaperDocument, QuantityValue } from "@/components/business"
import { Button } from "@/components/ui/button"
import {
    Dialog,
    DialogClose,
    DialogContent,
    DialogDescription,
    DialogTitle,
} from "@/components/ui/dialog"
import { getErrorMessage } from "@/lib/api"
import { toAutomationIdSegment } from "@/lib/automation-id"
import {
    usePaymentMaterialsQuery,
    useRequestDownloadMutation,
} from "../hooks/queries"
import type { PaymentRequest, PurchaseSnapshot, SalesSnapshot } from "../types"

type Preview =
    | {
          kind: "purchase"
          number: string
          version: number
          snapshot: PurchaseSnapshot
      }
    | { kind: "sales"; snapshot: SalesSnapshot }

/** 预览只读取申请提交时冻结的版本，不补读当前业务单。 */
export function RequestMaterials({
    request,
    workItemId,
    submissionVersion,
    idPrefix = "ppr",
}: {
    request: PaymentRequest
    workItemId?: string
    submissionVersion?: number
    idPrefix?: string
}) {
    const query = usePaymentMaterialsQuery(
        request.id,
        workItemId,
        submissionVersion,
    )
    const download = useRequestDownloadMutation()
    const [preview, setPreview] = useState<Preview | null>(null)
    const files = [
        ...new Map(
            [
                ...(request.attachments ?? []),
                ...(query.data?.sales.flatMap((sales) => sales.materials) ??
                    []),
            ].map((file) => [file.file_asset_id, file]),
        ).values(),
    ]
    return (
        <section
            className="space-y-4 border-t pt-5"
            aria-label="冻结采购销售与审核材料"
        >
            <div>
                <h3 className="font-medium">采购、销售与审核材料</h3>
                <p className="mt-1 text-xs text-muted-foreground">
                    {request.status === "draft"
                        ? "以下为保存草稿时核对的生效来源；提交时将再次校验并冻结。"
                        : "以下预览保留申请提交时的来源版本。"}
                    采购成本与销售成交金额分别展示。
                </p>
            </div>
            {query.isPending ? (
                <p className="text-sm text-muted-foreground">
                    正在读取来源资料…
                </p>
            ) : query.isError ? (
                <div className="space-y-2">
                    <p role="alert" className="text-sm text-destructive">
                        {getErrorMessage(query.error, "资料读取失败")}
                    </p>
                    <Button
                        id={`${idPrefix}-materials-retry`}
                        variant="outline"
                        onClick={() => void query.refetch()}
                    >
                        重试
                    </Button>
                </div>
            ) : (
                <div className="space-y-4">
                    <div className="flex flex-wrap gap-2">
                        {query.data.purchases.map((purchase) => (
                            <Button
                                key={purchase.purchase_order_id}
                                id={`${idPrefix}-purchase-preview-${toAutomationIdSegment(purchase.purchase_order_id)}`}
                                variant="outline"
                                size="sm"
                                onClick={() =>
                                    setPreview({
                                        kind: "purchase",
                                        number: purchase.purchase_order_no,
                                        version: purchase.version,
                                        snapshot: purchase.snapshot,
                                    })
                                }
                            >
                                采购 {purchase.purchase_order_no} · V
                                {purchase.version}
                            </Button>
                        ))}
                    </div>
                    <div className="flex flex-wrap gap-2">
                        {query.data.sales.map((sales) => (
                            <Button
                                key={sales.revision_id}
                                id={`${idPrefix}-sales-preview-${toAutomationIdSegment(sales.revision_id)}`}
                                variant="outline"
                                size="sm"
                                onClick={() =>
                                    setPreview({
                                        kind: "sales",
                                        snapshot: sales,
                                    })
                                }
                            >
                                销售 {sales.sales_order_no} · V
                                {sales.revision_no}
                            </Button>
                        ))}
                    </div>
                    {query.data.purchases.length === 0 && (
                        <p className="text-sm text-muted-foreground">
                            暂无可预览的采购资料。
                        </p>
                    )}
                </div>
            )}
            {files.length ? (
                <ul className="space-y-2">
                    {files.map((file) => (
                        <li
                            className="flex flex-wrap items-center justify-between gap-2 text-sm"
                            key={file.file_asset_id}
                        >
                            <span className="min-w-0 break-all">
                                {file.file_name}
                            </span>
                            <Button
                                id={`${idPrefix}-attachment-${toAutomationIdSegment(file.file_asset_id)}-download`}
                                variant="outline"
                                size="sm"
                                disabled={download.isPending}
                                onClick={() =>
                                    download.mutate({
                                        requestId: request.id,
                                        attachment: file,
                                        workItemId,
                                        submissionVersion,
                                    })
                                }
                            >
                                下载
                            </Button>
                        </li>
                    ))}
                </ul>
            ) : (
                <p className="text-xs text-muted-foreground">
                    未上传对账材料或其他附件。
                </p>
            )}
            {download.isError && (
                <p role="alert" className="text-sm text-destructive">
                    {getErrorMessage(download.error, "附件读取失败")}
                </p>
            )}
            <Dialog
                open={Boolean(preview)}
                onOpenChange={(open) => {
                    if (!open) setPreview(null)
                }}
            >
                <DialogContent
                    showCloseButton={false}
                    className="flex max-h-[min(96vh,56rem)] w-full max-w-[calc(100%-1.5rem)] flex-col gap-0 overflow-hidden border-0 bg-transparent p-0 shadow-none ring-0 sm:max-w-5xl dark:ring-0"
                >
                    <DialogTitle className="sr-only">
                        {preview?.kind === "purchase"
                            ? "采购单完整预览"
                            : "销售单完整预览"}
                    </DialogTitle>
                    <DialogDescription className="sr-only">
                        付款申请冻结的完整来源版本，按 Esc 或点击遮罩关闭。
                    </DialogDescription>
                    <div className="relative min-h-0 flex-1">
                        <DialogClose
                            render={
                                <Button
                                    id={`${idPrefix}-paper-close`}
                                    variant="secondary"
                                    size="icon-sm"
                                    className="absolute top-3 right-3 z-10 rounded-full"
                                />
                            }
                        >
                            <XIcon />
                            <span className="sr-only">关闭预览</span>
                        </DialogClose>
                        <div className="max-h-[min(96vh,56rem)] overflow-y-auto">
                            {preview?.kind === "purchase" ? (
                                <PurchasePaper preview={preview} />
                            ) : preview ? (
                                <SalesPaper
                                    sales={preview.snapshot}
                                    payer={request.our_company_name}
                                />
                            ) : null}
                        </div>
                    </div>
                </DialogContent>
            </Dialog>
        </section>
    )
}

function PurchasePaper({
    preview,
}: {
    preview: Extract<Preview, { kind: "purchase" }>
}) {
    const snapshot = preview.snapshot
    return (
        <PaperDocument
            frame="bare"
            title="采购单"
            subtitle="采购成本"
            documentNumber={preview.number}
            version={`V${preview.version}`}
            parties={[
                {
                    id: "payer",
                    label: "我方付款主体",
                    name: snapshot.our_company_name,
                },
                {
                    id: "supplier",
                    label: "供应商",
                    name: snapshot.supplier_name,
                },
            ]}
            metadata={[
                {
                    id: "payment",
                    label: "付款条件",
                    value: snapshot.payment_term_name,
                },
            ]}
            rows={snapshot.items}
            getRowId={(row) => row.id}
            columns={[
                {
                    id: "name",
                    header: "采购项目 / 规格",
                    cell: (row) => (
                        <div>
                            {row.item_name}
                            {row.spec && (
                                <p className="text-xs text-muted-foreground">
                                    {row.spec}
                                </p>
                            )}
                        </div>
                    ),
                },
                {
                    id: "quantity",
                    header: "数量",
                    align: "end",
                    cell: (row) => (
                        <QuantityValue value={row.quantity} unit={row.unit} />
                    ),
                },
                {
                    id: "cost",
                    header: "采购含税单价",
                    align: "end",
                    cell: (row) => <MoneyValue value={row.unit_price} />,
                },
                {
                    id: "amount",
                    header: "采购含税金额",
                    align: "end",
                    cell: (row) => <MoneyValue value={row.amount} />,
                },
            ]}
            totals={[
                {
                    id: "gross",
                    label: "采购含税总额",
                    value: <MoneyValue value={snapshot.total_amount} />,
                    emphasized: true,
                },
            ]}
        />
    )
}
function SalesPaper({ sales, payer }: { sales: SalesSnapshot; payer: string }) {
    return (
        <PaperDocument
            frame="bare"
            title="销售单"
            subtitle="客户成交金额"
            documentNumber={sales.sales_order_no}
            version={`V${sales.revision_no}`}
            parties={[
                { id: "company", label: "我方结算主体", name: payer },
                { id: "customer", label: "客户", name: sales.customer_name },
            ]}
            metadata={[
                {
                    id: "contract",
                    label: "合同",
                    value: sales.contract_no ?? "—",
                },
            ]}
            rows={sales.lines}
            getRowId={(row) => row.sales_order_revision_line_id}
            columns={[
                { id: "line", header: "序号", cell: (row) => row.line_no },
                {
                    id: "name",
                    header: "销售项目 / 规格",
                    cell: (row) => (
                        <div>
                            {row.item_name}
                            {row.specification && (
                                <p className="text-xs text-muted-foreground">
                                    {row.specification}
                                </p>
                            )}
                        </div>
                    ),
                },
                {
                    id: "quantity",
                    header: "数量",
                    align: "end",
                    cell: (row) => (
                        <QuantityValue value={row.quantity} unit={row.unit} />
                    ),
                },
                {
                    id: "price",
                    header: "销售含税单价",
                    align: "end",
                    cell: (row) => <MoneyValue value={row.unit_price_gross} />,
                },
                {
                    id: "gross",
                    header: "销售含税金额",
                    align: "end",
                    cell: (row) => <MoneyValue value={row.gross_amount} />,
                },
            ]}
            totals={[
                {
                    id: "gross",
                    label: "销售含税总额",
                    value: <MoneyValue value={sales.totals.gross} />,
                    emphasized: true,
                },
                {
                    id: "net",
                    label: "不含税金额",
                    value: <MoneyValue value={sales.totals.net} />,
                },
                {
                    id: "tax",
                    label: "税额",
                    value: <MoneyValue value={sales.totals.tax} />,
                },
            ]}
        />
    )
}
