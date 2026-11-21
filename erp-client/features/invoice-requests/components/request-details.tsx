import { MoneyValue } from "@/components/business"
import {
    Table,
    TableBody,
    TableCell,
    TableHead,
    TableHeader,
    TableRow,
} from "@/components/ui/table"
import { formatFixedDisplay } from "@/lib/fixed-decimal"
import type { InvoiceRequestData } from "../api"
import {
    canonicalInvoiceDecimal,
    INVOICE_QUANTITY_SCALE,
    INVOICE_UNIT_PRICE_SCALE,
    invoiceLineAmount,
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
    const facts = [
        ["购方开票抬头", data?.invoice_title],
        ["购方纳税人识别号", data?.tax_number],
        ["开票主体名称（销方）", data?.seller_name],
        ["开票主体纳税人识别号", data?.seller_tax_number],
        ["申请事由", data?.reason],
    ]
    const lines = data?.lines?.length
        ? data.lines
        : data?.invoice_content?.trim()
          ? [{ content: data.invoice_content, quantity: "", unit_price: "" }]
          : []
    return (
        <div className="flex min-w-0 flex-col gap-4">
            <dl className="grid gap-x-6 gap-y-3 text-body-compact sm:grid-cols-2">
                {facts.map(([label, value]) => (
                    <div key={label}>
                        <dt className="text-muted-foreground">{label}</dt>
                        <dd className="mt-1 break-words whitespace-pre-wrap">
                            {value?.trim() || "—"}
                        </dd>
                    </div>
                ))}
            </dl>
            <div className="flex min-w-0 flex-col gap-2">
                <h4 className="text-body-compact font-semibold">开票明细</h4>
                {lines.length ? (
                    <Table aria-label="开票明细" className="min-w-96">
                        <TableHeader>
                            <TableRow>
                                <TableHead>开票内容</TableHead>
                                <TableHead className="text-right">
                                    数量
                                </TableHead>
                                <TableHead className="text-right">
                                    含税单价（元）
                                </TableHead>
                                <TableHead className="text-right">
                                    行金额
                                </TableHead>
                            </TableRow>
                        </TableHeader>
                        <TableBody>
                            {lines.map((line, index) => {
                                const amount = invoiceLineAmount(line)
                                return (
                                    <TableRow key={index}>
                                        <TableCell className="w-full min-w-28 break-words whitespace-pre-wrap">
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
                    </Table>
                ) : (
                    <p className="text-xs text-muted-foreground">
                        未记录开票明细
                    </p>
                )}
            </div>
        </div>
    )
}
