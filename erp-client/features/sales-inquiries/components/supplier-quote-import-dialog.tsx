"use client"
import { transferJson, transferFileInput } from "@/lib/data-transfer"
import * as React from "react"
import { useMutation } from "@tanstack/react-query"
import { UploadIcon } from "lucide-react"
import { Button } from "@/components/ui/button"
import { selectedFileKey } from "@/lib/file-selection"
import { FileUpload } from "@/components/ui/file-upload"
import { Alert, AlertDescription } from "@/components/ui/alert"
import {
    Dialog,
    DialogContent,
    DialogHeader,
    DialogTitle,
    DialogDescription,
    DialogFooter,
} from "@/components/ui/dialog"
import { getErrorMessage } from "@/lib/api/errors"
import { useSupplierOfferingsForSkusQuery } from "@/features/supplier-offerings/hooks/queries"
import { useInquiryCommand } from "../hooks/use-inquiry-command"
import { useInternalQuotes } from "../hooks/use-internal-quotes"
import { useInquirySkuDetailsQuery } from "../hooks/queries"
import { inquiryPath } from "../api"
import type { ProcurementInquiry } from "../types"
import type { InquiryTask } from "./inquiry-feedback-editor"
import {
    SupplierQuotePreviewPanel,
    type SupplierQuoteImportPreview,
} from "./supplier-quote-preview-panel"

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
            const result = await transferJson<SupplierQuoteImportPreview>(
                "supplier_quote_preview",
                {
                    inquiry_id: inquiry.id,
                    task_id: task.id,
                    expected_version: inquiry.version,
                    task_version: task.version,
                    files: await Promise.all(files.map(transferFileInput)),
                },
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
                        <FileUpload
                            idPrefix="procurement-quote-import"
                            inputId="procurement-quote-import-files"
                            label="选择供应商回填报价表"
                            description="支持 XLSX，最多 20 个文件，合计不超过 50 MB"
                            density="compact"
                            accept=".xlsx"
                            multiple
                            selectedFiles={preview.variables ?? []}
                            selectLabel={
                                preview.variables?.length
                                    ? "重新选择文件"
                                    : "选择文件"
                            }
                            selectedDescription={
                                preview.isPending
                                    ? "正在读取并核对"
                                    : preview.isError
                                      ? "读取失败"
                                      : "已选择 · 确认后导入"
                            }
                            validateFiles={(files) => {
                                if (
                                    new Set(files.map(selectedFileKey)).size !==
                                    files.length
                                )
                                    return "本次选择包含重复文件，请每个文件只选择一次"
                                if (files.length > 20)
                                    return "最多选择 20 个文件"
                                if (
                                    files.reduce(
                                        (total, file) => total + file.size,
                                        0,
                                    ) >
                                    50 * 1024 * 1024
                                )
                                    return "文件合计不能超过 50 MB"
                                return null
                            }}
                            disabled={
                                pending ||
                                !command.ready ||
                                command.unresolved ||
                                details.isPending
                            }
                            onFilesSelected={(files) => {
                                setMessage("")
                                preview.reset()
                                preview.mutate(files)
                            }}
                            onFileRemove={(file) => {
                                const remaining = (
                                    preview.variables ?? []
                                ).filter((item) => item !== file)
                                setMessage("")
                                preview.reset()
                                if (remaining.length) preview.mutate(remaining)
                            }}
                        />
                        <p className="text-xs text-muted-foreground">
                            最多 20 个文件，合计不超过 50
                            MiB。空白行保留为未报价；可供数量为 0
                            时须填写原因。日期格式 YYYY-MM-DD。
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
                            <SupplierQuotePreviewPanel preview={preview.data} />
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
