"use client"

import type { ColumnDef } from "@tanstack/react-table"
import { Maximize2Icon } from "lucide-react"

import { BusinessStatusBadge, TableTextCell } from "@/components/business"
import { Button } from "@/components/ui/button"
import { COPY_TRIGGER_LABELS } from "@/features/approval-processes/copy-rules"
import { documentTypeLabel as knownDocumentTypeLabel } from "@/features/approval-processes/labels"
import { isDocumentType } from "@/features/approval-processes/parse"
import { displayUnixSeconds } from "@/features/approval-workflow/display"
import { toAutomationIdSegment } from "@/lib/automation-id"
import { getApprovalInstanceStatusMeta } from "@/lib/task-status-dictionary"
import type { CopyReceipt } from "./api"

export const documentTypeLabel = (kind: string) =>
    isDocumentType(kind) ? knownDocumentTypeLabel(kind) : "审批单据"

/**
 * 紧凑单行抄送列定义（对齐 `density="compact"` 36px 行高）：
 * - 单号（含未读标）、单据类型、抄送说明、审批进度、抄送时间、操作全部单行排列；
 * - 「查看」默认触发原位检查器预览（零跳页），附带「全屏打开」图标按钮。
 */
export function copyColumns(
    onPreview: (row: CopyReceipt) => void,
    onOpenFullPage?: (row: CopyReceipt) => void,
): ColumnDef<CopyReceipt, unknown>[] {
    return [
        {
            id: "document",
            header: "单号",
            enableHiding: false,
            enableSorting: false,
            meta: { width: "default", label: "单号" },
            cell: ({ row: { original: row } }) => (
                <div className="flex max-w-table-column-default min-w-0 items-center gap-1.5">
                    <span
                        className="num min-w-0 truncate font-medium text-foreground"
                        title={row.document_no}
                    >
                        {row.document_no}
                    </span>
                    {!row.read_at ? (
                        <BusinessStatusBadge
                            context="list"
                            tone="info"
                            label="未读"
                            appearance="quiet"
                            className="shrink-0"
                        />
                    ) : null}
                </div>
            ),
        },
        {
            id: "type",
            header: "单据类型",
            enableSorting: false,
            meta: { width: "status", label: "单据类型" },
            cell: ({ row: { original: row } }) => {
                const label = `${documentTypeLabel(row.document_type)}审批`
                return (
                    <span
                        className="block max-w-table-column-status truncate text-xs text-muted-foreground"
                        title={label}
                    >
                        {label}
                    </span>
                )
            },
        },
        {
            id: "summary",
            header: "抄送说明",
            enableSorting: false,
            meta: { width: "flex", label: "抄送说明" },
            cell: ({ row: { original: row } }) => {
                const submitter = row.submitter_name?.trim() || "申请人未记录"
                const detail = `${COPY_TRIGGER_LABELS[row.trigger]}${row.node_name ? ` · ${row.node_name}` : ""} · 第 ${row.round_no} 轮`
                const fullText = `${submitter} · ${detail}`
                return (
                    <div
                        className="flex max-w-table-column-reference min-w-0 items-center gap-1.5"
                        title={fullText}
                    >
                        <span className="max-w-36 shrink-0 truncate font-medium text-foreground">
                            {submitter}
                        </span>
                        <span className="num min-w-0 truncate text-xs text-muted-foreground">
                            {detail}
                        </span>
                    </div>
                )
            },
        },
        {
            id: "status",
            header: "审批进度",
            enableSorting: false,
            meta: { width: "status", label: "审批进度" },
            cell: ({ row }) => {
                const rawStatus = row.original.instance_status
                const status = rawStatus
                    ? getApprovalInstanceStatusMeta(rawStatus)
                    : { label: "状态待核对", tone: "neutral" as const }
                return (
                    <BusinessStatusBadge
                        context="list"
                        label={status.label}
                        tone={status.tone}
                        appearance={
                            status.tone === "destructive" ||
                            status.tone === "warning"
                                ? "badge"
                                : "quiet"
                        }
                    />
                )
            },
        },
        {
            id: "copiedAt",
            header: "抄送时间",
            enableSorting: false,
            meta: { width: "default", numeric: true, label: "抄送时间" },
            cell: ({ row }) => (
                <TableTextCell
                    value={displayUnixSeconds(row.original.copied_at)?.label}
                    numeric
                    clamp={1}
                />
            ),
        },
        {
            id: "action",
            header: "操作",
            enableHiding: false,
            enableSorting: false,
            meta: { width: "status", align: "end", label: "操作" },
            cell: ({ row: { original: row } }) => {
                const id = toAutomationIdSegment(row.id)
                const label = `${documentTypeLabel(row.document_type)} ${row.document_no}`
                return (
                    <div className="inline-flex items-center justify-end gap-0.5">
                        <Button
                            id={`copy-open-${id}`}
                            type="button"
                            variant="ghost"
                            size="xs"
                            className="h-control-sm px-2 text-body-compact font-normal text-muted-foreground group-hover/row:text-primary group-focus-within/row:text-primary hover:bg-sidebar-accent/70 hover:text-primary"
                            aria-label={`查看${label}`}
                            onClick={(event) => {
                                event.stopPropagation()
                                onPreview(row)
                            }}
                        >
                            查看
                        </Button>
                        {onOpenFullPage ? (
                            <Button
                                id={`copy-full-${id}`}
                                type="button"
                                variant="ghost"
                                size="icon-xs"
                                className="size-control-sm text-muted-foreground/80 opacity-80 hover:bg-sidebar-accent/70 hover:text-primary hover:opacity-100"
                                aria-label={`全屏打开${label}`}
                                title="全屏打开"
                                onClick={(event) => {
                                    event.stopPropagation()
                                    onOpenFullPage(row)
                                }}
                            >
                                <Maximize2Icon
                                    className="size-3.5"
                                    aria-hidden="true"
                                />
                            </Button>
                        ) : null}
                    </div>
                )
            },
        },
    ]
}
