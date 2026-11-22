import { MoneyValue } from "@/components/business"
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
import type { InvoiceRequestData } from "../api"
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

export function InvoiceRequestDetails({
    data,
}: {
    data: Omit<InvoiceRequestData, "amount"> | undefined
}) {
    const facts: ReadonlyArray<{
        label: string
        value?: string | null
        numeric?: boolean
        full?: boolean
    }> = [
        { label: "购方开票抬头", value: data?.invoice_title },
        {
            label: "购方纳税人识别号",
            value: data?.tax_number,
            numeric: true,
        },
        { label: "开票主体名称（销方）", value: data?.seller_name },
        {
            label: "开票主体纳税人识别号",
            value: data?.seller_tax_number,
            numeric: true,
        },
        { label: "申请事由", value: data?.reason, full: true },
    ]
    const lines = data?.lines?.length
        ? data.lines
        : data?.invoice_content?.trim()
          ? [{ content: data.invoice_content, quantity: "", unit_price: "" }]
          : []
    const linesTotal = lines.length ? invoiceLinesTotal(lines) : null

    return (
        <div className="flex min-w-0 flex-col gap-6">
            <dl className="grid gap-x-8 gap-y-4 text-body-compact sm:grid-cols-2">
                {facts.map((fact) => (
                    <div
                        key={fact.label}
                        className={cn(fact.full && "sm:col-span-2")}
                    >
                        <dt className="text-xs text-muted-foreground">
                            {fact.label}
                        </dt>
                        <dd
                            className={cn(
                                "mt-1 break-words whitespace-pre-wrap text-foreground",
                                fact.numeric && "num",
                            )}
                        >
                            {fact.value?.trim() || "—"}
                        </dd>
                    </div>
                ))}
            </dl>

            <div className="flex min-w-0 flex-col gap-2.5">
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
                {lines.length ? (
                    <div className="overflow-x-auto border-y border-border/70">
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
                                        <TableCell colSpan={4}>
                                            明细合计
                                        </TableCell>
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
                    <p className="text-xs text-muted-foreground">
                        未记录开票明细
                    </p>
                )}
            </div>
        </div>
    )
}
