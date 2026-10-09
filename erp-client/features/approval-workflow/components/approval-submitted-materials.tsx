"use client"

import { ApprovalSalesOrderPaper } from "./approval-sales-order-paper"
import { BusinessFailureState } from "@/components/business"
import { Button } from "@/components/ui/button"
import { LoadingButton } from "@/components/ui/loading-button"
import { Spinner } from "@/components/ui/spinner"
import { toAutomationIdSegment } from "@/lib/automation-id"
import { getErrorMessage } from "@/lib/api"
import { displayBusinessText } from "@/lib/display-name"
import {
    useApprovalMaterials,
    useDownloadApprovalMaterial,
} from "../hooks/use-approval-materials"
import { displayActorName, displayReadableName } from "../display"

const sectionValue = (section: {
    value: string
    object_id: string | null
    numeric: boolean
}) =>
    !section.numeric && section.object_id?.trim() === section.value.trim()
        ? "未记录名称"
        : section.value

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
    view = "all",
    sourceRevisionId,
}: {
    instanceId: string
    enabled: boolean
    view?: "all" | "source-sales" | "attachments"
    sourceRevisionId?: string
}) {
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
    const {
        display,
        attachments,
        sales_order: salesOrder,
        subject_version: version,
    } = query.data
    const source = display.source
    const submitterName = displayActorName(
        source.submitter_name,
        salesOrder?.submitted_by,
    )
    const counterpartyName =
        displayReadableName(display.counterparty_label) ||
        displayReadableName(source.customer)
    return (
        <article className="space-y-6 rounded-lg bg-card p-6 shadow-lg sm:p-10">
            <header className="space-y-2 border-b pb-5 pr-8">
                <h2 className="text-xl font-semibold">
                    {view === "source-sales"
                        ? "来源销售资料"
                        : view === "attachments"
                          ? "合同、凭证及附件"
                          : salesOrder
                            ? "销售单提交预览"
                            : "审批提交资料"}
                </h2>
                <p className="text-sm text-muted-foreground">
                    提交版本 {version} ·
                    以下内容保留自此次提交，审批完成后单据的变化不在此处更新。
                </p>
                {view === "all" && !salesOrder && counterpartyName && (
                    <p className="font-medium">{counterpartyName}</p>
                )}
                {view === "all" && !salesOrder && source.list_summary && (
                    <p className="text-sm">{source.list_summary}</p>
                )}
            </header>
            {view !== "all" ? null : salesOrder ? (
                <ApprovalSalesOrderPaper
                    submission={salesOrder}
                    documentNo={
                        displayBusinessText(
                            query.data.document_no,
                            query.data.document_id,
                        ) || "未记录单据编号"
                    }
                    submitterName={submitterName ?? null}
                />
            ) : (
                <>
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
                                    <dd className="mt-1">
                                        {submitterName || "未记录姓名"}
                                    </dd>
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
                                        {sectionValue(section)}
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
                                        <span>
                                            {displayReadableName(line.title) ||
                                                "未记录商品名称"}
                                        </span>
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
                </>
            )}
            {view !== "attachments" &&
                (display.source_sales ?? []).length > 0 && (
                    <section className="space-y-4 border-t pt-5">
                        <h3 className="font-medium">关联销售单</h3>
                        {display.source_sales
                            ?.filter(
                                (sales) =>
                                    !sourceRevisionId ||
                                    sales.revision_id === sourceRevisionId,
                            )
                            .map((sales) => (
                                <article
                                    key={sales.revision_id}
                                    className="space-y-3 rounded-lg border p-4 text-sm"
                                >
                                    <div className="flex flex-wrap items-center justify-between gap-2">
                                        <p className="font-medium">
                                            {displayBusinessText(
                                                sales.document_no,
                                                sales.document_id,
                                            ) || "未记录销售单号"}{" "}
                                            · 销售版本 {sales.revision_no}
                                        </p>
                                        <p className="num font-semibold">
                                            <span className="mr-2 text-xs font-normal text-muted-foreground">
                                                整单销售金额
                                            </span>
                                            {sales.source.amount_label}
                                        </p>
                                    </div>
                                    {sales.source.customer && (
                                        <p>
                                            {displayReadableName(
                                                sales.source.customer,
                                            ) || "未记录客户名称"}
                                        </p>
                                    )}
                                    <dl className="grid gap-3 sm:grid-cols-2">
                                        {sales.source.extra_sections.map(
                                            (section) => (
                                                <div key={section.label}>
                                                    <dt className="text-xs text-muted-foreground">
                                                        {section.label}
                                                    </dt>
                                                    <dd className="mt-1 break-words">
                                                        {sectionValue(section)}
                                                    </dd>
                                                </div>
                                            ),
                                        )}
                                    </dl>
                                    <ul className="divide-y">
                                        {sales.source.lines.map(
                                            (line, index) => (
                                                <li
                                                    key={`${line.title}:${index}`}
                                                    className="flex flex-wrap justify-between gap-2 py-2"
                                                >
                                                    <span>
                                                        {displayReadableName(
                                                            line.title,
                                                        ) || "未记录商品名称"}
                                                    </span>
                                                    <span className="num text-muted-foreground">
                                                        {line.quantity}
                                                    </span>
                                                </li>
                                            ),
                                        )}
                                    </ul>
                                    {sales.source.more_count > 0 && (
                                        <p className="text-muted-foreground">
                                            另有 {sales.source.more_count}{" "}
                                            行未展示，请结合销售合同或凭证核对。
                                        </p>
                                    )}
                                </article>
                            ))}
                    </section>
                )}
            <section className="space-y-3 border-t pt-5">
                <h3 className="font-medium">
                    {display.source_sales?.length
                        ? "采购附件及关联销售合同、凭证"
                        : "提交合同、凭证及附件"}
                </h3>
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
                                            variant="ghost"
                                            size="sm"
                                            render={
                                                <a
                                                    href={`/approval-materials?${new URLSearchParams({ instance: instanceId, file: file.file_asset_id, name: file.file_name })}`}
                                                    target="_blank"
                                                    rel="noopener noreferrer"
                                                    aria-label={`查看 ${file.file_name}（新标签页）`}
                                                />
                                            }
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
        </article>
    )
}
