"use client"

import { useState } from "react"
import { ApprovalMaterialPreview } from "./approval-material-preview"
import type { ApprovalMaterials } from "../api/materials"
import { BusinessFailureState } from "@/components/business"
import { Button } from "@/components/ui/button"
import { LoadingButton } from "@/components/ui/loading-button"
import { Spinner } from "@/components/ui/spinner"
import { toAutomationIdSegment } from "@/lib/automation-id"
import { getErrorMessage } from "@/lib/api"
import {
    useApprovalMaterials,
    useDownloadApprovalMaterial,
} from "../hooks/use-approval-materials"

const READABLE_MATERIAL_TYPES = new Set([
    "image/jpeg",
    "image/png",
    "image/webp",
    "application/pdf",
])

/** 审批专用资料预览；不加载普通业务详情，不把冻结摘要当作完整单据。 */
export function ApprovalSubmittedMaterials({
    instanceId,
    enabled,
}: {
    instanceId: string
    enabled: boolean
}) {
    const [preview, setPreview] = useState<
        ApprovalMaterials["attachments"][number] | null
    >(null)
    const query = useApprovalMaterials(instanceId, enabled)
    const download = useDownloadApprovalMaterial(instanceId)
    if (query.isPending || query.isFetching)
        return (
            <div className="flex min-h-64 items-center justify-center gap-2 rounded-lg bg-card p-8">
                <Spinner />
                正在读取提交资料…
            </div>
        )
    if (query.isError)
        return (
            <div className="rounded-lg bg-card p-8">
                <BusinessFailureState
                    id="approval-submitted-materials-retry"
                    title="提交资料读取失败"
                    error={query.error}
                    onRetry={() => void query.refetch()}
                />
            </div>
        )
    const { display, attachments, subject_version: version } = query.data
    const source = display.source
    return (
        <article className="space-y-6 rounded-lg bg-card p-6 shadow-lg sm:p-10">
            <header className="space-y-2 border-b pb-5 pr-8">
                <h2 className="text-xl font-semibold">审批提交资料</h2>
                <p className="text-sm text-muted-foreground">
                    提交版本 {version} ·
                    以下内容保留自此次提交，审批完成后单据的变化不在此处更新。
                </p>
                {(display.counterparty_label || source.customer) && (
                    <p className="font-medium">
                        {display.counterparty_label || source.customer}
                    </p>
                )}
                {source.list_summary && (
                    <p className="text-sm">{source.list_summary}</p>
                )}
            </header>
            {(source.amount_label ||
                source.submitter_name ||
                source.extra_sections.length > 0) && (
                <dl className="grid gap-4 text-sm sm:grid-cols-2">
                    {source.amount_label && (
                        <div>
                            <dt className="text-xs text-muted-foreground">
                                提交金额
                            </dt>
                            <dd className="num mt-1 text-lg font-semibold">
                                {source.amount_label}
                            </dd>
                        </div>
                    )}
                    {source.submitter_name && (
                        <div>
                            <dt className="text-xs text-muted-foreground">
                                申请人
                            </dt>
                            <dd className="mt-1">{source.submitter_name}</dd>
                        </div>
                    )}
                    {source.extra_sections.map((section, index) => (
                        <div key={`${section.label}:${index}`}>
                            <dt className="text-xs text-muted-foreground">
                                {section.label}
                            </dt>
                            <dd
                                className={`mt-1 break-words ${section.numeric ? "num" : ""}`}
                            >
                                {section.value}
                            </dd>
                        </div>
                    ))}
                </dl>
            )}
            <section className="space-y-3">
                <h3 className="font-medium">提交资料摘要</h3>
                {source.lines.length ? (
                    <ul className="divide-y text-sm">
                        {source.lines.map((line, index) => (
                            <li
                                key={`${line.title}:${index}`}
                                className="flex flex-wrap justify-between gap-3 py-3"
                            >
                                <span>{line.title}</span>
                                <span className="text-muted-foreground">
                                    {[line.quantity, line.due_label]
                                        .filter(Boolean)
                                        .join(" · ")}
                                </span>
                            </li>
                        ))}
                    </ul>
                ) : (
                    <p className="text-sm text-muted-foreground">
                        此次提交未保留明细摘要，请核对其他提交字段及附件。
                    </p>
                )}
                {source.more_count > 0 && (
                    <p
                        role="status"
                        className="rounded-md border border-warning-border bg-warning-soft p-3 text-sm text-warning-soft-foreground"
                    >
                        当前仅展示 {source.lines.length} 行摘要，另有{" "}
                        {source.more_count}{" "}
                        行未包含在摘要中。请核对提交附件；资料不足时请发起人补充后重新提交。
                    </p>
                )}
                {display.impact_summary && (
                    <p className="text-sm text-muted-foreground">
                        {display.impact_summary}
                    </p>
                )}
            </section>
            <section className="space-y-3 border-t pt-5">
                <h3 className="font-medium">提交附件</h3>
                {attachments.length ? (
                    <ul className="space-y-2">
                        {attachments.map((file) => (
                            <li
                                key={file.file_asset_id}
                                className="flex flex-wrap items-center justify-between gap-2 text-sm"
                            >
                                <span className="min-w-0 break-all">
                                    {file.file_name}
                                </span>
                                <div className="flex gap-2">
                                    {READABLE_MATERIAL_TYPES.has(
                                        file.content_type,
                                    ) && (
                                        <Button
                                            id={`approval-submitted-material-${toAutomationIdSegment(file.file_asset_id)}-preview`}
                                            type="button"
                                            variant="ghost"
                                            size="sm"
                                            onClick={() => setPreview(file)}
                                        >
                                            查看
                                        </Button>
                                    )}
                                    <LoadingButton
                                        loading={
                                            download.isPending &&
                                            download.variables
                                                ?.file_asset_id ===
                                                file.file_asset_id
                                        }
                                        id={`approval-submitted-material-${toAutomationIdSegment(file.file_asset_id)}-download`}
                                        type="button"
                                        variant="outline"
                                        size="sm"
                                        disabled={download.isPending}
                                        onClick={() => download.mutate(file)}
                                    >
                                        {download.isPending &&
                                        download.variables?.file_asset_id ===
                                            file.file_asset_id
                                            ? "读取中…"
                                            : "下载"}
                                    </LoadingButton>
                                </div>
                            </li>
                        ))}
                    </ul>
                ) : (
                    <p className="text-sm text-muted-foreground">
                        此次提交未保留可读取的附件。
                    </p>
                )}
                {download.isError && (
                    <p role="alert" className="text-sm text-destructive">
                        {getErrorMessage(
                            download.error,
                            "附件读取失败，请重试",
                        )}
                    </p>
                )}
            </section>
            {preview && (
                <ApprovalMaterialPreview
                    key={preview.file_asset_id}
                    instanceId={instanceId}
                    file={preview}
                    onClose={() => setPreview(null)}
                />
            )}
        </article>
    )
}
