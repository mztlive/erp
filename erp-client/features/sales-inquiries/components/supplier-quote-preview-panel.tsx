"use client"

import { MoneyValue } from "@/components/business"
import { Alert, AlertDescription } from "@/components/ui/alert"
import { multiplyFixed } from "@/lib/fixed-decimal"
import { toAutomationIdSegment } from "@/lib/automation-id"
import type { ImportedQuote } from "../lib/supplier-quote-import"

export type SupplierQuoteImportPreview = {
    rows: ImportedQuote[]
    issues: string[]
    skipped: number
}

/** 上传预览与后台任务恢复共用的报价核对内容。 */
export function SupplierQuotePreviewPanel({
    preview,
    idPrefix = "quote-import-preview",
}: {
    preview: SupplierQuoteImportPreview
    idPrefix?: string
}) {
    return (
        <>
                                <p className="text-sm">
                                    读取 {preview.rows.length}{" "}
                                    条回复，未报价 {preview.skipped}{" "}
                                    行，已导入{" "}
                                    {preview.rows.filter((row) => row.status === "已导入").length}{" "}
                                    条。
                                </p>
                                {preview.issues.length > 0 && (
                                    <Alert variant="destructive">
                                        <AlertDescription>
                                            <ul>
                                                {preview.issues.map(
                                                    (issue) => (
                                                        <li key={issue}>
                                                            {issue}
                                                        </li>
                                                    ),
                                                )}
                                            </ul>
                                        </AlertDescription>
                                    </Alert>
                                )}
                                <div className="divide-y rounded-lg border">
                                    {preview.rows.map((row) => (
                                        <div
                                            key={`${row.quote.supplier_id}:${row.quote.line_id}`}
                                            id={`${idPrefix}-${toAutomationIdSegment(row.quote.supplier_id)}-${toAutomationIdSegment(row.quote.line_id)}`}
                                            className="space-y-1 p-3 text-sm"
                                        >
                                            <div className="flex justify-between gap-3">
                                                <strong>
                                                    {row.product} ·{" "}
                                                    {row.supplier}
                                                </strong>
                                                <span>{row.status}</span>
                                            </div>
                                            <p>
                                                {row.quote.terms.unavailable ? (
                                                    "无法供货"
                                                ) : (
                                                    <>
                                                        含税单价{" "}
                                                        <MoneyValue
                                                            value={
                                                                row.quote.terms
                                                                    .unit_cost_gross
                                                            }
                                                        />{" "}
                                                        · 税率{" "}
                                                        {multiplyFixed(
                                                            row.quote.terms
                                                                .input_tax_rate,
                                                            "100",
                                                            {
                                                                leftMaxScale: 6,
                                                                rightMaxScale: 0,
                                                                outputScale: 4,
                                                            },
                                                        )}
                                                        % · 可供{" "}
                                                        {
                                                            row.quote.terms
                                                                .available_quantity
                                                        }{" "}
                                                        · 运费{" "}
                                                        <MoneyValue
                                                            value={
                                                                row.quote.terms
                                                                    .freight_gross!
                                                            }
                                                        />{" "}
                                                        · 交期{" "}
                                                        {
                                                            row.quote.terms
                                                                .expected_delivery_date
                                                        }{" "}
                                                    </>
                                                )}
                                            </p>
                                            {row.previous &&
                                                row.status === "新版本" && (
                                                    <p className="text-muted-foreground">
                                                        原回复：
                                                        {row.previous.terms
                                                            .unavailable
                                                            ? "无法供货"
                                                            : `单价 ${row.previous.terms.unit_cost_gross}，税率 ${multiplyFixed(row.previous.terms.input_tax_rate, "100", { leftMaxScale: 6, rightMaxScale: 0, outputScale: 4 })}%，可供 ${row.previous.terms.available_quantity}，运费 ${row.previous.terms.freight_gross ?? "未确认"}，交期 ${row.previous.terms.expected_delivery_date}`}
                                                        ；原备注：
                                                        {row.previous
                                                            .internal_note ||
                                                            "无"}
                                                    </p>
                                                )}
                                            {row.quote.internal_note && (
                                                <p>{row.quote.internal_note}</p>
                                            )}
                                            <p className="text-xs text-muted-foreground">
                                                {row.source}
                                            </p>
                                        </div>
                                    ))}
                                </div>
        </>
    )
}
