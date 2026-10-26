"use client"

import { useState } from "react"
import Link from "next/link"
import { useMutation, useQuery, useQueryClient } from "@tanstack/react-query"
import { ArrowUpRightIcon } from "lucide-react"
import { z } from "zod"
import { Button } from "@/components/ui/button"
import {
    Dialog,
    DialogContent,
    DialogDescription,
    DialogFooter,
    DialogHeader,
    DialogTitle,
} from "@/components/ui/dialog"
import { StatusBadge } from "@/components/ui/status-badge"
import { getErrorMessage } from "@/lib/api/errors"
import { toAutomationIdSegment } from "@/lib/automation-id"
import {
    decodeTransferJson,
    readTransferResult,
    recoverDataTransfer,
    type DataTransferContext,
    type TransferFile,
} from "@/lib/data-transfer"
import type { BackgroundJobView } from "../api"
import { backgroundJobKeys } from "../queries"
import { TransferResult } from "./transfer-result"

const batchResultSchema = z.object({
    rows: z.array(
        z.object({
            row_id: z.string(),
            status: z.string(),
            message: z.string().nullish(),
            error: z.string().nullish(),
            result: z.record(z.string(), z.unknown()).nullish(),
        }),
    ),
})
type BatchResult = z.infer<typeof batchResultSchema>
type BatchRow = BatchResult["rows"][number]
type RowOutcome = "succeeded" | "prepared" | "failed" | "unknown" | "pending"

function parseBatchResult(file: TransferFile) {
    let decoded: unknown
    try {
        decoded = decodeTransferJson<unknown>(file)
    } catch {
        throw new Error("逐行结果格式无效，请保留任务号并联系管理员核对")
    }
    const parsed = batchResultSchema.safeParse(decoded)
    if (!parsed.success)
        throw new Error("逐行结果格式无效，请保留任务号并联系管理员核对")
    if (
        new Set(parsed.data.rows.map((row) => row.row_id)).size !==
        parsed.data.rows.length
    )
        throw new Error("逐行结果存在重复记录，请保留任务号并联系管理员核对")
    return parsed.data
}

function outcome(row: BatchRow, context: DataTransferContext): RowOutcome {
    const status = row.status.toLowerCase()
    if (
        ["succeeded", "replayed"].includes(status) ||
        (status === "valid" && row.result)
    ) {
        return context.kind === "portal_batch" &&
            context.context.mode === "new_product" &&
            typeof row.result?.status === "string" &&
            row.result.status.toUpperCase() === "DRAFT"
            ? "prepared"
            : "succeeded"
    }
    if (["failed", "invalid", "validation_failed"].includes(status))
        return "failed"
    if (["ready", "valid"].includes(status)) return "pending"
    return "unknown"
}

const outcomeLabels: Record<RowOutcome, string> = {
    succeeded: "已完成",
    prepared: "草稿已准备",
    failed: "未完成",
    unknown: "结果待确认",
    pending: "尚未执行",
}

function rowMessage(row: BatchRow, state: RowOutcome) {
    if (state === "prepared")
        return "新品草稿已准备，请打开申请补充图片并提交。"
    if (row.message || row.error) return row.message || row.error
    return {
        succeeded: "已完成本行操作。",
        failed: "本行未完成，请回业务页面核对资料后重新校验。",
        unknown: "尚未查到确定结果，请核对原操作结果，保留原内容，勿重复提交。",
        pending: "校验通过，尚未执行本行操作。",
    }[state]
}

function resultText(row: BatchRow, fields: string[]) {
    for (const key of fields) {
        const value = row.result?.[key]
        if (typeof value === "string" && value.trim()) return value
    }
    return null
}

function rowDestination(
    row: BatchRow,
    context: DataTransferContext,
    portal: boolean,
) {
    const id = resultText(row, ["offering_id", "application_id", "id"])
    if (!id) return null
    if (context.kind === "supply_batch")
        return `/procurement/supplier-offerings/${encodeURIComponent(id)}`
    if (!portal) return null
    const resource =
        context.context.mode === "availability" ? "offerings" : "applications"
    return `/supplier-portal/${resource}/${encodeURIComponent(id)}`
}

function batchDestination(context: DataTransferContext, portal: boolean) {
    if (!portal) return "/procurement/supplier-offerings"
    if (context.context.mode === "new_product")
        return "/supplier-portal/new-products"
    if (context.context.mode === "quote") return "/supplier-portal/quotes"
    return "/supplier-portal/offerings"
}

/** 批量结果按原逐行回执展示，待确认行仅通过原任务查询恢复。 */
export function TransferBatchResult({
    job,
    context,
    portal = false,
}: {
    job: BackgroundJobView
    context: DataTransferContext
    portal?: boolean
}) {
    const client = useQueryClient()
    const [open, setOpen] = useState(false)
    const id = `background-jobs-batch-${toAutomationIdSegment(job.id)}`
    const resultKey = ["data-transfer-batch-result", portal, job.id] as const
    const hasResult = Boolean(job.finished_at && job.result_file_asset_id)
    const expired =
        job.result_expires_at !== null &&
        job.result_expires_at <= Date.now() / 1000
    const result = useQuery({
        queryKey: resultKey,
        queryFn: async () =>
            parseBatchResult(await readTransferResult(job.id, portal)),
        enabled: hasResult && !expired,
        retry: false,
        staleTime: 0,
    })
    const recover = useMutation({
        meta: { suppressErrorToast: true },
        mutationFn: async () =>
            parseBatchResult(await recoverDataTransfer(job.id, portal)),
        retry: false,
        onSuccess: async (data) => {
            client.setQueryData(resultKey, data)
            await Promise.all([
                client.invalidateQueries({ queryKey: backgroundJobKeys.all }),
                client.invalidateQueries({
                    queryKey: ["data-transfer-context", portal, job.id],
                }),
                client.invalidateQueries({
                    queryKey: [
                        portal ? "supplier-portal" : "account",
                        "data-transfers",
                    ],
                }),
                client.invalidateQueries({ queryKey: ["supplier-offerings"] }),
                client.invalidateQueries({ queryKey: ["supplier-portal"] }),
            ])
        },
    })
    const rows =
        result.isError || expired || !hasResult ? [] : (result.data?.rows ?? [])
    const counts = {
        succeeded: 0,
        prepared: 0,
        failed: 0,
        unknown: 0,
        pending: 0,
    }
    for (const row of rows) counts[outcome(row, context)] += 1
    const summary = [
        `已完成 ${counts.succeeded} 行`,
        counts.prepared > 0 ? `草稿已准备 ${counts.prepared} 行` : null,
        `未完成 ${counts.failed} 行`,
        counts.unknown > 0 ? `结果待确认 ${counts.unknown} 行` : null,
        counts.pending > 0 ? `尚未执行 ${counts.pending} 行` : null,
    ]
        .filter(Boolean)
        .join("，")

    return (
        <section className="space-y-3">
            <h3 className="font-medium">批量处理结果</h3>
            {!hasResult ? (
                <p className="text-muted-foreground">
                    {job.finished_at
                        ? "暂无可读取的逐行结果，请先核对任务提示。"
                        : "正在后台处理，完成后可查看每行结果。"}
                </p>
            ) : expired ? (
                <p className="text-muted-foreground">
                    结果文件已过期。已完成的业务操作仍然有效，请回业务页面核对，避免重复提交。
                </p>
            ) : result.isPending ? (
                <p className="text-muted-foreground">正在读取逐行结果…</p>
            ) : result.isError ? (
                <div className="space-y-2">
                    <p role="alert" className="text-destructive">
                        {getErrorMessage(
                            result.error,
                            "逐行结果读取失败，请重试",
                        )}
                    </p>
                    <Button
                        id={`${id}-retry`}
                        variant="outline"
                        onClick={() => void result.refetch()}
                    >
                        重新读取结果
                    </Button>
                </div>
            ) : (
                <>
                    <p>{summary}。</p>
                    {counts.prepared > 0 && (
                        <p className="text-muted-foreground">
                            新品草稿准备完成后仍需补充图片并提交申请。
                        </p>
                    )}
                    {counts.unknown > 0 && (
                        <div className="space-y-2">
                            <p className="text-muted-foreground">
                                请先核对原操作结果。此操作只查询原记录，不会重新提交本批业务。
                            </p>
                            {context.can_recover ? (
                                <Button
                                    id={`${id}-recover`}
                                    disabled={recover.isPending}
                                    onClick={() => recover.mutate()}
                                >
                                    {recover.isPending
                                        ? "正在核对…"
                                        : "核对待确认结果"}
                                </Button>
                            ) : (
                                <p className="text-muted-foreground">
                                    {context.resume_reason ||
                                        "当前无法从此任务核对，请保留原内容并回业务页面确认结果。"}
                                </p>
                            )}
                        </div>
                    )}
                    {recover.isError && (
                        <p role="alert" className="text-destructive">
                            {getErrorMessage(
                                recover.error,
                                "原操作结果仍无法确认，请保留任务记录并稍后核对",
                            )}
                        </p>
                    )}
                    {recover.isSuccess && counts.unknown > 0 && (
                        <p role="status" className="text-muted-foreground">
                            仍有 {counts.unknown}{" "}
                            行无法确认，请保留原内容，稍后再次核对或联系业务负责人。
                        </p>
                    )}
                    <Button
                        id={`${id}-details`}
                        variant="outline"
                        onClick={() => setOpen(true)}
                    >
                        查看逐行结果
                    </Button>
                </>
            )}
            <Button
                id={`${id}-open-source`}
                variant="outline"
                render={<Link href={batchDestination(context, portal)} />}
            >
                {portal ? "打开供应商业务页面" : "打开供应商供给"}
                <ArrowUpRightIcon data-icon="inline-end" />
            </Button>
            {hasResult && !expired && (
                <TransferResult
                    jobId={job.id}
                    portal={portal}
                    label="下载逐行结果"
                    idPrefix={`${id}-download`}
                />
            )}
            <Dialog open={open} onOpenChange={setOpen}>
                <DialogContent
                    closeButtonId={`${id}-dialog-close`}
                    className="flex max-h-[85svh] flex-col sm:max-w-3xl"
                >
                    <DialogHeader>
                        <DialogTitle>逐行处理结果</DialogTitle>
                        <DialogDescription>
                            成功行已经完成；失败行回业务页面修正；待确认行先核对原结果，保留原内容。
                        </DialogDescription>
                    </DialogHeader>
                    {result.isError && (
                        <p role="alert" className="text-destructive">
                            {getErrorMessage(
                                result.error,
                                "逐行结果读取失败，请关闭后重新读取",
                            )}
                        </p>
                    )}
                    <ul className="min-h-0 space-y-3 overflow-y-auto">
                        {rows.map((row, index) => {
                            const state = outcome(row, context)
                            const href =
                                state === "succeeded" || state === "prepared"
                                    ? rowDestination(row, context, portal)
                                    : null
                            const rowId = `${id}-row-${toAutomationIdSegment(row.row_id)}`
                            const name = resultText(row, [
                                "sku_name",
                                "product_name",
                                "name",
                                "title",
                            ])
                            const code = resultText(row, [
                                "sku_no",
                                "sku_code",
                                "supplier_sku_code",
                            ])
                            return (
                                <li
                                    key={row.row_id}
                                    className="space-y-2 rounded-lg border p-3"
                                >
                                    <div className="flex flex-wrap items-center justify-between gap-2">
                                        <p className="font-medium">
                                            第 {index + 1} 行
                                            {name ? ` · ${name}` : ""}
                                        </p>
                                        <StatusBadge
                                            tone={
                                                state === "succeeded"
                                                    ? "success"
                                                    : state === "unknown" ||
                                                        state === "prepared"
                                                      ? "warning"
                                                      : state === "failed"
                                                        ? "destructive"
                                                        : "neutral"
                                            }
                                            label={outcomeLabels[state]}
                                        />
                                    </div>
                                    {code && (
                                        <p className="text-sm text-muted-foreground">
                                            商品编码：{code}
                                        </p>
                                    )}
                                    <p className="text-sm">
                                        {rowMessage(row, state)}
                                    </p>
                                    {href && (
                                        <Button
                                            id={`${rowId}-open`}
                                            size="sm"
                                            variant="outline"
                                            render={<Link href={href} />}
                                        >
                                            {state === "prepared"
                                                ? "打开新品申请"
                                                : "查看业务结果"}
                                            <ArrowUpRightIcon data-icon="inline-end" />
                                        </Button>
                                    )}
                                </li>
                            )
                        })}
                    </ul>
                    <DialogFooter>
                        <Button
                            id={`${id}-dialog-done`}
                            variant="outline"
                            onClick={() => setOpen(false)}
                        >
                            关闭
                        </Button>
                    </DialogFooter>
                </DialogContent>
            </Dialog>
        </section>
    )
}
