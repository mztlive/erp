"use client"

import { useRef, useState } from "react"
import { DownloadIcon, UploadIcon } from "lucide-react"
import { useAppForm } from "@/components/form"
import { Button } from "@/components/ui/button"
import {
    Dialog,
    DialogContent,
    DialogDescription,
    DialogFooter,
    DialogHeader,
    DialogTitle,
} from "@/components/ui/dialog"
import { Input } from "@/components/ui/input"
import { Label } from "@/components/ui/label"
import { toast } from "@/components/ui/toast"
import { useRegionIndex, type RegionIndex } from "@/features/regions"
import { getErrorMessage } from "@/lib/api/errors"
import { downloadBlob } from "@/lib/download-blob"
import {
    buildDeliveryImportPreview,
    isWorkbookProduct,
    productLabel,
    workbookSnapshot,
    type DeliveryImportPreview,
    type DeliveryImportSource,
    type DeliveryWorkbookContext,
} from "../lib/sales-delivery-import"
import {
    createSalesDeliveryIssueWorkbook,
    createSalesDeliveryWorkbook,
    readPastedDeliveryRows,
    readSalesDeliveryWorkbook,
} from "../lib/sales-delivery-workbook"
import type { SalesOrderCreateFormApi } from "../lib/sales-order-create-form-types"
import type { SalesOrderDraftLineInput } from "../types"
import { SalesDeliveryImportPreview } from "./sales-delivery-import-preview"

const prefix = "sales-delivery-workbook"

export function SalesDeliveryWorkbookActions({
    form,
    lines,
    draftId,
    onApplied,
}: {
    form: SalesOrderCreateFormApi
    lines: SalesOrderDraftLineInput[]
    draftId?: string
    onApplied: () => void
}) {
    const regions = useRegionIndex()
    const editorScope = useRef("")
    const [open, setOpen] = useState(false)
    const [exporting, setExporting] = useState(false)
    const getContext = () => ({
        scope: draftId ? `draft:${draftId}` : editorScope.current,
        customerId: form.getFieldValue("customerId"),
        lines: form.getFieldValue("lineItems"),
    })
    const prepare = () => {
        if (!draftId && !editorScope.current)
            editorScope.current = crypto.randomUUID()
    }
    const download = async (index: RegionIndex) => {
        setExporting(true)
        try {
            prepare()
            const blob = await createSalesDeliveryWorkbook(getContext(), index)
            downloadBlob(blob, "销售单收货安排.xlsx", {
                linkId: `${prefix}-file`,
            })
        } catch (error) {
            toast.add({
                title: "导出收货安排失败",
                description: getErrorMessage(error),
                type: "error",
            })
        } finally {
            setExporting(false)
        }
    }
    if (!lines.some(isWorkbookProduct)) return null
    return (
        <>
            <Button
                id={`${prefix}-export`}
                type="button"
                variant="outline"
                size="sm"
                disabled={exporting || regions.isPending}
                onClick={() => {
                    if (regions.data) void download(regions.data)
                    else {
                        toast.add({
                            title: "正在重试加载地区资料，请稍后重新导出",
                            type: "error",
                        })
                        void regions.refetch()
                    }
                }}
            >
                <DownloadIcon />
                {exporting ? "正在导出…" : "导出收货安排"}
            </Button>
            <Button
                id={`${prefix}-import`}
                type="button"
                variant="outline"
                size="sm"
                onClick={() => {
                    prepare()
                    setOpen(true)
                }}
            >
                <UploadIcon />
                批量导入地址
            </Button>
            {open && (
                <SalesDeliveryWorkbookDialog
                    lines={lines}
                    regions={regions.data}
                    regionsError={regions.isError}
                    onRetryRegions={() => void regions.refetch()}
                    getContext={getContext}
                    onClose={() => setOpen(false)}
                    onApply={(preview) => {
                        const byLine = new Map(
                            preview.groups
                                .filter((group) => group.changes.length)
                                .map((group) => [
                                    group.line.rowKey,
                                    group.allocations,
                                ]),
                        )
                        form.setFieldValue(
                            "lineItems",
                            form.getFieldValue("lineItems").map((line) => {
                                const allocations = byLine.get(line.rowKey)
                                return allocations
                                    ? {
                                          ...line,
                                          deliveryAllocations: allocations,
                                      }
                                    : line
                            }),
                        )
                        onApplied()
                        setOpen(false)
                        toast.add({
                            title: `已更新 ${byLine.size} 项商品的收货安排`,
                            type: "success",
                        })
                    }}
                />
            )}
        </>
    )
}

type LoadedPreview = {
    filename: string
    source: DeliveryImportSource
    result: DeliveryImportPreview
    snapshot: string
    filled: boolean
}

function SalesDeliveryWorkbookDialog({
    lines,
    regions,
    regionsError,
    getContext,
    onRetryRegions,
    onClose,
    onApply,
}: {
    lines: SalesOrderDraftLineInput[]
    regions?: RegionIndex
    regionsError: boolean
    getContext: () => DeliveryWorkbookContext
    onRetryRegions: () => void
    onClose: () => void
    onApply: (result: DeliveryImportPreview) => void
}) {
    const options = lines.flatMap((line, index) =>
        isWorkbookProduct(line)
            ? [{ value: line.rowKey, label: productLabel(line, index) }]
            : [],
    )
    const [busy, setBusy] = useState(false)
    const [error, setError] = useState("")
    const [preview, setPreview] = useState<LoadedPreview | null>(null)
    const form = useAppForm({
        defaultValues: {
            target: options.length === 1 ? options[0].value : "",
            text: "",
        },
        onSubmit: ({ value }) =>
            previewSource("粘贴的地址", (context) =>
                readPastedDeliveryRows(value.text, context, value.target),
            ),
    })
    const stale = Boolean(
        preview && preview.snapshot !== workbookSnapshot(getContext()),
    )
    const canApply = Boolean(
        preview &&
        !stale &&
        !preview.result.issues.length &&
        preview.result.groups.some((group) => group.changes.length) &&
        preview.result.groups.every((group) => group.summary.complete),
    )

    async function previewSource(
        filename: string,
        read: (
            context: DeliveryWorkbookContext,
        ) => DeliveryImportSource | Promise<DeliveryImportSource>,
        filled = false,
    ) {
        if (!regions || busy) return
        const context = getContext()
        const snapshot = workbookSnapshot(context)
        setError("")
        setBusy(true)
        setPreview(null)
        try {
            const source = await read(context)
            if (!source.rows.length) throw new Error("没有填写需要导入的地址")
            const result = await buildDeliveryImportPreview(
                source,
                context.lines,
                regions,
                filled,
            )
            setPreview({ filename, source, result, snapshot, filled })
        } catch (reason) {
            setError(getErrorMessage(reason))
        } finally {
            setBusy(false)
        }
    }

    const downloadIssues = async () => {
        if (!preview) return
        try {
            downloadBlob(
                await createSalesDeliveryIssueWorkbook(preview.result.issues),
                "收货安排问题行.xlsx",
                { linkId: `${prefix}-issues-file` },
            )
        } catch (reason) {
            setError(getErrorMessage(reason))
        }
    }

    return (
        <Dialog
            open
            onOpenChange={(next) => {
                if (!next) onClose()
            }}
        >
            <DialogContent
                className="flex max-h-[90vh] flex-col overflow-hidden sm:max-w-3xl"
                closeButtonId={`${prefix}-close`}
                finalFocus={() => document.getElementById(`${prefix}-import`)}
            >
                <DialogHeader>
                    <DialogTitle>批量导入地址</DialogTitle>
                    <DialogDescription>
                        上传本单导出的收货安排，或选择一个商品后导入客户地址名单。
                    </DialogDescription>
                </DialogHeader>
                <div className="min-h-0 space-y-4 overflow-y-auto px-1">
                    {regionsError && (
                        <div
                            className="flex flex-wrap items-center gap-2 text-destructive"
                            role="alert"
                        >
                            地区资料加载失败。
                            <Button
                                id={`${prefix}-regions-retry`}
                                type="button"
                                variant="outline"
                                size="sm"
                                onClick={onRetryRegions}
                            >
                                重试
                            </Button>
                        </div>
                    )}
                    <form.AppField name="target">
                        {(field) => (
                            <field.SelectField
                                id={`${prefix}-target`}
                                label="客户名单对应商品"
                                options={options}
                                disabled={
                                    busy || !!preview || options.length === 1
                                }
                                placeholder="仅导入客户名单时选择"
                            />
                        )}
                    </form.AppField>
                    <div className="space-y-2">
                        <Label htmlFor={`${prefix}-upload`}>上传 Excel</Label>
                        <Input
                            id={`${prefix}-upload`}
                            type="file"
                            accept=".xlsx"
                            disabled={!regions || busy}
                            aria-describedby={`${prefix}-help`}
                            onChange={(event) => {
                                const file = event.target.files?.[0]
                                event.target.value = ""
                                if (file)
                                    void previewSource(file.name, (context) =>
                                        readSalesDeliveryWorkbook(
                                            file,
                                            context,
                                            form.getFieldValue("target"),
                                        ),
                                    )
                            }}
                        />
                        <p
                            id={`${prefix}-help`}
                            className="text-xs text-muted-foreground"
                        >
                            支持 .xlsx，最多 5
                            MB。导出文件按表内明细匹配，客户名单按上方所选商品导入。
                        </p>
                    </div>
                    {!preview && (
                        <div className="space-y-2">
                            <form.AppField name="text">
                                {(field) => (
                                    <field.TextareaField
                                        id={`${prefix}-paste`}
                                        label="或粘贴 Excel 地址行"
                                        rows={3}
                                        disabled={busy}
                                        description="按收货人、联系电话、省、市、区县、详细地址、配送数量排列。"
                                    />
                                )}
                            </form.AppField>
                            <Button
                                id={`${prefix}-parse`}
                                type="button"
                                variant="outline"
                                size="sm"
                                disabled={!regions || busy}
                                onClick={() => void form.handleSubmit()}
                            >
                                预览粘贴内容
                            </Button>
                        </div>
                    )}
                    {busy && (
                        <p role="status" className="text-body-compact">
                            正在核对地址…
                        </p>
                    )}
                    {error && (
                        <p
                            className="text-body-compact text-destructive"
                            role="alert"
                        >
                            {error}
                        </p>
                    )}
                    {stale && (
                        <p
                            className="text-body-compact text-destructive"
                            role="alert"
                        >
                            页面的商品或收货安排已变化，请重新导入并核对。
                        </p>
                    )}
                    {preview && !stale && regions && (
                        <>
                            {!preview.source.baselines &&
                                !preview.filled &&
                                preview.source.rows.some(
                                    (row) => !row.cells[6],
                                ) && (
                                    <Button
                                        id={`${prefix}-fill-one`}
                                        type="button"
                                        variant="outline"
                                        size="sm"
                                        onClick={() => {
                                            if (
                                                preview.snapshot ===
                                                workbookSnapshot(getContext())
                                            )
                                                void previewSource(
                                                    preview.filename,
                                                    () => preview.source,
                                                    true,
                                                )
                                        }}
                                    >
                                        空白配送数量填 1
                                    </Button>
                                )}
                            <SalesDeliveryImportPreview
                                result={preview.result}
                                regions={regions}
                                filename={preview.filename}
                                prefix={prefix}
                                onDownloadIssues={() => void downloadIssues()}
                            />
                        </>
                    )}
                </div>
                <DialogFooter>
                    {preview && (
                        <Button
                            id={`${prefix}-reset`}
                            type="button"
                            variant="outline"
                            onClick={() => {
                                setPreview(null)
                                setError("")
                            }}
                        >
                            重新导入
                        </Button>
                    )}
                    <Button
                        id={`${prefix}-cancel`}
                        type="button"
                        variant="outline"
                        onClick={onClose}
                    >
                        取消
                    </Button>
                    <Button
                        id={`${prefix}-apply`}
                        type="button"
                        disabled={!canApply || busy}
                        onClick={() => {
                            if (!preview || !canApply || busy) return
                            if (
                                preview.snapshot !==
                                workbookSnapshot(getContext())
                            ) {
                                setError(
                                    "页面的商品或收货安排已变化，请重新导入并核对。",
                                )
                                return
                            }
                            onApply(preview.result)
                        }}
                    >
                        确认应用
                    </Button>
                </DialogFooter>
            </DialogContent>
        </Dialog>
    )
}
