"use client"
import * as React from "react"
import { useMutation } from "@tanstack/react-query"
import { UploadIcon } from "lucide-react"
import { Button } from "@/components/ui/button"
import { Input } from "@/components/ui/input"
import { Alert, AlertDescription } from "@/components/ui/alert"
import {
    Dialog,
    DialogContent,
    DialogHeader,
    DialogTitle,
    DialogDescription,
    DialogFooter,
} from "@/components/ui/dialog"
import { MoneyValue } from "@/components/business"
import { multiplyFixed } from "@/lib/fixed-decimal"
import { getErrorMessage } from "@/lib/api/errors"
import { toAutomationIdSegment } from "@/lib/automation-id"
import { useSupplierOfferingsForSkusQuery } from "@/features/supplier-offerings/hooks/queries"
import { useInquiryCommand } from "../hooks/use-inquiry-command"
import { useInternalQuotes } from "../hooks/use-internal-quotes"
import { useInquirySkuDetailsQuery } from "../hooks/queries"
import { inquiryPath } from "../api"
import type { ProcurementInquiry } from "../types"
import type { InquiryTask } from "./inquiry-feedback-editor"

export function SupplierQuoteImportButton({
    inquiry,
    task,
    userId,
    onSaved,
}: {
    inquiry: ProcurementInquiry
    task: InquiryTask
    userId: string
    onSaved: () => void
}) {
    const [open, setOpen] = React.useState(false)
    const [message, setMessage] = React.useState("")
    const quotes = useInternalQuotes(inquiry.id, task.id, open)
    const offerings = useSupplierOfferingsForSkusQuery(
        open ? inquiry.draft.lines.map((line) => line.sku_id) : [],
    )
    const details = useInquirySkuDetailsQuery(
        open ? inquiry.draft.lines.map((line) => line.sku_id) : [],
    )
    const command = useInquiryCommand(userId, inquiry.id)
    const preview = useMutation({
        mutationFn: async (files: File[]) => {
            const [freshQuotes, freshOfferings] = await Promise.all([
                quotes.refetch(),
                offerings.refetch(),
            ])
            if (freshQuotes.error) throw freshQuotes.error
            if (freshOfferings.error) throw freshOfferings.error
            if (details.error) throw details.error
            const { parseSupplierQuoteFiles } =
                await import("../lib/supplier-quote-import")
            const result = await parseSupplierQuoteFiles(
                files,
                inquiry,
                freshOfferings.data ?? [],
                freshQuotes.data ?? [],
                details.units,
            )
            return {
                ...result,
                requirement: inquiry.requirement_version,
                round: inquiry.feedback_round + 1,
                taskVersion: task.version,
                version: inquiry.version,
            }
        },
        meta: { suppressErrorToast: true },
    })
    const changed =
        preview.data &&
        (preview.data.requirement !== inquiry.requirement_version ||
            preview.data.round !== inquiry.feedback_round + 1 ||
            preview.data.taskVersion !== task.version ||
            preview.data.version !== inquiry.version)
    const pending = command.isPending || preview.isPending
    const rows =
        preview.data?.rows.filter((row) => row.status !== "已导入") ?? []
    async function confirm() {
        if (!preview.data || changed) return
        setMessage("")
        try {
            await command.run(
                `${inquiryPath(true, inquiry.id)}/internal-quotes/import`,
                {
                    expected_version: preview.data.version,
                    task_id: task.id,
                    task_version: preview.data.taskVersion,
                    requirement_version: preview.data.requirement,
                    feedback_round: preview.data.round,
                    rows: rows.map(({ source, sku_id, quote }) => ({
                        source,
                        sku_id,
                        quote,
                    })),
                },
            )
            preview.reset()
            setMessage("供应商报价已登记，可在商品行查看报价并采用。")
            onSaved()
        } catch (error) {
            setMessage(getErrorMessage(error, "导入失败，请核对后重试"))
        }
    }
    return (
        <>
            <Button
                id="procurement-inquiry-import-quotes"
                variant="outline"
                onClick={() => setOpen(true)}
            >
                <UploadIcon aria-hidden="true" />
                导入供应商报价
            </Button>
            <Dialog open={open} onOpenChange={setOpen}>
                <DialogContent
                    closeButtonId="procurement-quote-import-close"
                    className="flex max-h-[85svh] flex-col sm:max-w-4xl"
                >
                    <DialogHeader>
                        <DialogTitle>导入供应商报价</DialogTitle>
                        <DialogDescription>
                            选择供应商回填的新版报价表。核对后统一登记内部报价，不覆盖正在填写的供给价。
                        </DialogDescription>
                    </DialogHeader>
                    <div className="min-h-0 space-y-4 overflow-y-auto">
                        <Input
                            id="procurement-quote-import-files"
                            aria-label="供应商回填报价表"
                            type="file"
                            accept=".xlsx"
                            multiple
                            disabled={
                                pending ||
                                !command.ready ||
                                command.unresolved ||
                                details.isPending
                            }
                            onChange={(event) => {
                                setMessage("")
                                preview.reset()
                                const files = Array.from(
                                    event.target.files ?? [],
                                )
                                event.target.value = ""
                                if (files.length) preview.mutate(files)
                            }}
                        />
                        <p className="text-xs text-muted-foreground">
                            最多 20 个文件，合计不超过 50
                            MiB。空白行保留为未报价；可供数量为 0
                            时须填写原因。日期格式
                            YYYY-MM-DD，有效期当天起失效。
                        </p>
                        {(preview.error || message) && (
                            <Alert>
                                <AlertDescription>
                                    {preview.error
                                        ? getErrorMessage(
                                              preview.error,
                                              "文件读取失败",
                                          )
                                        : message}
                                </AlertDescription>
                            </Alert>
                        )}
                        {preview.isPending && (
                            <p role="status">正在读取并核对报价…</p>
                        )}
                        {changed && (
                            <Alert variant="warning">
                                <AlertDescription>
                                    询价或报价记录已变化，请重新选择文件核对。当前预览已保留。
                                </AlertDescription>
                            </Alert>
                        )}
                        {preview.data && (
                            <>
                                <p className="text-sm">
                                    读取 {preview.data.rows.length}{" "}
                                    条回复，未报价 {preview.data.skipped}{" "}
                                    行，已导入{" "}
                                    {preview.data.rows.length - rows.length}{" "}
                                    条。
                                </p>
                                {preview.data.issues.length > 0 && (
                                    <Alert variant="destructive">
                                        <AlertDescription>
                                            <ul>
                                                {preview.data.issues.map(
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
                                    {preview.data.rows.map((row) => (
                                        <div
                                            key={`${row.quote.supplier_id}:${row.quote.line_id}`}
                                            id={`quote-import-preview-${toAutomationIdSegment(row.quote.supplier_id)}-${toAutomationIdSegment(row.quote.line_id)}`}
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
                                                        · 失效日{" "}
                                                        {row.quote.terms
                                                            .valid_to ??
                                                            "未限定"}
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
                                                            : `单价 ${row.previous.terms.unit_cost_gross}，税率 ${multiplyFixed(row.previous.terms.input_tax_rate, "100", { leftMaxScale: 6, rightMaxScale: 0, outputScale: 4 })}%，可供 ${row.previous.terms.available_quantity}，运费 ${row.previous.terms.freight_gross ?? "未确认"}，交期 ${row.previous.terms.expected_delivery_date}，失效日 ${row.previous.terms.valid_to ?? "未限定"}`}
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
                        )}
                    </div>
                    <DialogFooter>
                        {command.unresolved && (
                            <Button
                                id="procurement-quote-import-recover"
                                disabled={pending}
                                onClick={async () => {
                                    try {
                                        await command.recover()
                                        preview.reset()
                                        setMessage("上次提交已核对成功")
                                        onSaved()
                                    } catch (error) {
                                        setMessage(
                                            getErrorMessage(
                                                error,
                                                "核对失败，请重试",
                                            ),
                                        )
                                    }
                                }}
                            >
                                核对上次提交
                            </Button>
                        )}
                        <Button
                            id="procurement-quote-import-confirm"
                            disabled={
                                pending ||
                                !command.ready ||
                                command.unresolved ||
                                Boolean(changed) ||
                                !rows.length ||
                                Boolean(preview.data?.issues.length)
                            }
                            onClick={() => void confirm()}
                        >
                            确认导入 {rows.length || ""} 条
                        </Button>
                    </DialogFooter>
                </DialogContent>
            </Dialog>
        </>
    )
}
