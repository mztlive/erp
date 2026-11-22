import {
    DetailWorkspaceFact,
    DetailWorkspaceFacts,
    MoneyValue,
    detailWorkspaceStyles,
} from "@/components/business"
import {
    Table,
    TableBody,
    TableCell,
    TableFooter,
    TableHead,
    TableHeader,
    TableRow,
} from "@/components/ui/table"
import { formatFixedDisplay } from "@/lib/fixed-decimal"
import { cn } from "@/lib/utils"
import type { InvoiceRequestData, InvoiceRequestLine } from "../api"
import {
    canonicalInvoiceDecimal,
    INVOICE_QUANTITY_SCALE,
    INVOICE_UNIT_PRICE_SCALE,
    invoiceLineAmount,
    invoiceLinesTotal,
    isPositiveInvoiceDecimal,
} from "../line-amounts"

function detailDecimal(value: string, maxScale: number): string {
    if (!isPositiveInvoiceDecimal(value, maxScale)) return "—"
    return formatFixedDisplay(canonicalInvoiceDecimal(value), { maxScale })
}

export function resolveInvoiceRequestLines(
    data: Omit<InvoiceRequestData, "amount"> | undefined,
): InvoiceRequestLine[] {
    if (data?.lines?.length) return data.lines
    if (data?.invoice_content?.trim()) {
        return [{ content: data.invoice_content, quantity: "", unit_price: "" }]
    }
    return []
}

export function InvoiceRequestInfoFacts({
    data,
}: {
    data: Omit<InvoiceRequestData, "amount"> | undefined
}) {
    return (
        <DetailWorkspaceFacts columns={2} ariaLabel="开票资料">
            <DetailWorkspaceFact
                label="购方开票抬头"
                value={data?.invoice_title?.trim()}
            />
            <DetailWorkspaceFact
                label="购方纳税人识别号"
                value={data?.tax_number?.trim()}
                numeric
            />
            <DetailWorkspaceFact
                label="开票主体名称（销方）"
                value={data?.seller_name?.trim()}
            />
            <DetailWorkspaceFact
                label="开票主体纳税人识别号"
                value={data?.seller_tax_number?.trim()}
                numeric
            />
            <DetailWorkspaceFact
                label="申请事由"
                value={
                    data?.reason?.trim() ? (
                        <span className="whitespace-pre-wrap">
                            {data.reason.trim()}
                        </span>
                    ) : null
                }
                span="full"
            />
        </DetailWorkspaceFacts>
    )
}

export function InvoiceRequestLinesTable({
    data,
    showHeader = false,
}: {
    data: Omit<InvoiceRequestData, "amount"> | undefined
    showHeader?: boolean
}) {
    const lines = resolveInvoiceRequestLines(data)
    const linesTotal = lines.length ? invoiceLinesTotal(lines) : null

    return (
        <div className="flex min-w-0 flex-col gap-2.5">
            {showHeader ? (
                <div className="flex items-center justify-between gap-2">
                    <h4 className="text-body-compact font-semibold text-foreground">
                        开票明细
                    </h4>
                    {lines.length ? (
                        <span className="num text-xs text-muted-foreground">
                            共 {lines.length} 行
                        </span>
                    ) : null}
                </div>
            ) : null}
            {lines.length ? (
                <div
                    data-slot="detail-inline-table"
                    className={cn(
                        detailWorkspaceStyles.inlineTableWrapper,
                        "overflow-x-auto",
                    )}
                >
                    <Table aria-label="开票明细" className="min-w-96">
                        <TableHeader>
                            <TableRow>
                                <TableHead className="w-10">#</TableHead>
                                <TableHead>开票内容</TableHead>
                                <TableHead className="w-28 text-right">
                                    数量
                                </TableHead>
                                <TableHead className="w-36 text-right">
                                    含税单价（元）
                                </TableHead>
                                <TableHead className="w-36 text-right">
                                    行金额
                                </TableHead>
                            </TableRow>
                        </TableHeader>
                        <TableBody>
                            {lines.map((line, index) => {
                                const amount = invoiceLineAmount(line)
                                return (
                                    <TableRow key={index}>
                                        <TableCell className="num text-muted-foreground">
                                            {index + 1}
                                        </TableCell>
                                        <TableCell className="min-w-36 break-words whitespace-pre-wrap">
                                            {line.content?.trim() || "—"}
                                        </TableCell>
                                        <TableCell className="num text-right">
                                            {detailDecimal(
                                                line.quantity,
                                                INVOICE_QUANTITY_SCALE,
                                            )}
                                        </TableCell>
                                        <TableCell className="num text-right">
                                            {detailDecimal(
                                                line.unit_price,
                                                INVOICE_UNIT_PRICE_SCALE,
                                            )}
                                        </TableCell>
                                        <TableCell className="text-right">
                                            {amount === null ? (
                                                "—"
                                            ) : (
                                                <MoneyValue
                                                    value={amount}
                                                    display="exact"
                                                />
                                            )}
                                        </TableCell>
                                    </TableRow>
                                )
                            })}
                        </TableBody>
                        {linesTotal !== null ? (
                            <TableFooter>
                                <TableRow>
                                    <TableCell colSpan={4}>明细合计</TableCell>
                                    <TableCell className="text-right font-semibold">
                                        <MoneyValue
                                            value={linesTotal}
                                            display="exact"
                                        />
                                    </TableCell>
                                </TableRow>
                            </TableFooter>
                        ) : null}
                    </Table>
                </div>
            ) : (
                <p className="text-xs text-muted-foreground">未记录开票明细</p>
            )}
        </div>
    )
}

export function InvoiceRequestDetails({
    data,
}: {
    data: Omit<InvoiceRequestData, "amount"> | undefined
}) {
    return (
        <div className="flex min-w-0 flex-col gap-6">
            <InvoiceRequestInfoFacts data={data} />
            <InvoiceRequestLinesTable data={data} showHeader />
        </div>
    )
}
