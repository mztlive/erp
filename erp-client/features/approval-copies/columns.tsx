"use client"

import type { ColumnDef } from "@tanstack/react-table"
import { ChevronRightIcon, MailIcon } from "lucide-react"
import { Badge } from "@/components/ui/badge"
import { Button } from "@/components/ui/button"
import { COPY_TRIGGER_LABELS } from "@/features/approval-processes/copy-rules"
import { isDocumentType } from "@/features/approval-processes/parse"
import { documentTypeLabel as knownDocumentTypeLabel } from "@/features/approval-processes/labels"
import { toAutomationIdSegment } from "@/lib/automation-id"
import type { CopyReceipt } from "./api"

export const documentTypeLabel = (kind: string) =>
    isDocumentType(kind) ? knownDocumentTypeLabel(kind) : "审批单据"

const STATUS: Record<string, string> = {
    RUNNING: "审批中",
    APPROVED: "已通过",
    REJECTED: "已驳回",
    CANCELLED: "已撤回",
    BLOCKED: "审批受阻",
    ACTIVE: "待审批",
    SUPERSEDED: "往轮记录",
}

/** 抄送使用标准事项表格；阅读标记与审批进度分别展示。 */
export function copyColumns(
    onOpen: (row: CopyReceipt) => void,
): ColumnDef<CopyReceipt, unknown>[] {
    return [
        {
            id: "document",
            header: "事项",
            enableHiding: false,
            enableSorting: false,
            meta: { width: "reference", label: "事项" },
            cell: ({ row: { original: row } }) => (
                <div className="flex items-center gap-3 py-1">
                    <MailIcon
                        className="size-4 shrink-0 text-muted-foreground"
                        aria-hidden="true"
                    />
                    <div className="min-w-0 space-y-1.5">
                        <div
                            className="num max-w-64 truncate font-medium"
                            title={row.document_no}
                        >
                            {row.document_no}
                        </div>
                        <div className="flex flex-wrap items-center gap-2 text-xs text-muted-foreground">
                            {documentTypeLabel(row.document_type)}审批
                            {!row.read_at && (
                                <Badge variant="secondary">未读</Badge>
                            )}
                        </div>
                    </div>
                </div>
            ),
        },
        {
            id: "summary",
            header: "抄送说明",
            enableSorting: false,
            meta: { width: "flex", label: "抄送说明" },
            cell: ({ row: { original: row } }) => (
                <div className="space-y-1.5 whitespace-normal">
                    <div>{row.submitter_name || "申请人未记录"}</div>
                    <div className="text-xs text-muted-foreground">
                        {COPY_TRIGGER_LABELS[row.trigger]}
                        {row.node_name ? ` · ${row.node_name}` : ""} · 第{" "}
                        {row.round_no} 轮
                    </div>
                </div>
            ),
        },
        {
            id: "status",
            header: "审批进度",
            enableSorting: false,
            meta: { width: "status", label: "审批进度" },
            cell: ({ row }) =>
                STATUS[row.original.instance_status ?? ""] ?? "状态待核对",
        },
        {
            id: "copiedAt",
            header: "抄送时间",
            enableSorting: false,
            meta: { width: "reference", label: "抄送时间" },
            cell: ({ row }) => (
                <span className="num">
                    {new Date(row.original.copied_at * 1000).toLocaleString(
                        "zh-CN",
                    )}
                </span>
            ),
        },
        {
            id: "action",
            header: "操作",
            enableHiding: false,
            enableSorting: false,
            meta: { width: "quantity", align: "end", label: "操作" },
            cell: ({ row: { original: row } }) => (
                <Button
                    id={`copy-open-${toAutomationIdSegment(row.id)}`}
                    variant="ghost"
                    size="sm"
                    aria-label={`查看${documentTypeLabel(row.document_type)} ${row.document_no}`}
                    onClick={() => onOpen(row)}
                >
                    查看
                    <ChevronRightIcon aria-hidden="true" />
                </Button>
            ),
        },
    ]
}
