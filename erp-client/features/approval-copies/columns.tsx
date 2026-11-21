"use client"

import type { ColumnDef } from "@tanstack/react-table"
import {
    BusinessStatusBadge,
    TableIdentityCell,
    TableRowActions,
    TableTextCell,
} from "@/components/business"
import type { StatusTone } from "@/components/ui/status-badge"
import { COPY_TRIGGER_LABELS } from "@/features/approval-processes/copy-rules"
import { isDocumentType } from "@/features/approval-processes/parse"
import { documentTypeLabel as knownDocumentTypeLabel } from "@/features/approval-processes/labels"
import { toAutomationIdSegment } from "@/lib/automation-id"
import type { CopyReceipt } from "./api"

export const documentTypeLabel = (kind: string) =>
    isDocumentType(kind) ? knownDocumentTypeLabel(kind) : "审批单据"

const STATUS: Record<string, { label: string; tone: StatusTone }> = {
    RUNNING: { label: "审批中", tone: "info" },
    APPROVED: { label: "已通过", tone: "success" },
    REJECTED: { label: "已驳回", tone: "destructive" },
    CANCELLED: { label: "已撤回", tone: "void" },
    BLOCKED: { label: "审批受阻", tone: "destructive" },
    ACTIVE: { label: "待审批", tone: "warning" },
    SUPERSEDED: { label: "往轮记录", tone: "neutral" },
}

export function copyColumns(
    onOpen: (row: CopyReceipt) => void,
): ColumnDef<CopyReceipt, unknown>[] {
    return [
        {
            id: "document",
            header: "事项",
            enableHiding: false,
            enableSorting: false,
            meta: { width: "default", label: "事项" },
            cell: ({ row: { original: row } }) => {
                const type = `${documentTypeLabel(row.document_type)}审批`
                return (
                    <TableIdentityCell
                        primary={<span className="num">{row.document_no}</span>}
                        primaryTitle={row.document_no}
                        secondary={
                            <span className="inline-flex max-w-full items-center gap-1.5 align-bottom">
                                <span className="truncate">{type}</span>
                                {!row.read_at ? (
                                    <BusinessStatusBadge
                                        context="list"
                                        tone="info"
                                        label="未读"
                                        appearance="quiet"
                                        className="shrink-0"
                                    />
                                ) : null}
                            </span>
                        }
                        secondaryTitle={type}
                    />
                )
            },
        },
        {
            id: "summary",
            header: "抄送说明",
            enableSorting: false,
            meta: { width: "flex", label: "抄送说明" },
            cell: ({ row: { original: row } }) => {
                const summary = `${COPY_TRIGGER_LABELS[row.trigger]}${row.node_name ? ` · ${row.node_name}` : ""} · 第 ${row.round_no} 轮`
                return (
                    <div className="min-w-0">
                        <TableTextCell
                            value={row.submitter_name}
                            clamp={1}
                            emptyLabel="申请人未记录"
                            emptyVariant="label"
                        />
                        <TableTextCell
                            value={summary}
                            clamp={1}
                            className="num text-xs text-muted-foreground/80"
                        />
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
                const status = STATUS[row.original.instance_status ?? ""]
                return (
                    <BusinessStatusBadge
                        context="list"
                        label={status?.label ?? "状态待核对"}
                        tone={status?.tone ?? "neutral"}
                        appearance={
                            status?.tone === "destructive" ||
                            status?.tone === "warning"
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
                    value={new Date(
                        row.original.copied_at * 1000,
                    ).toLocaleString("zh-CN")}
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
                    <TableRowActions
                        moreId={`copy-row-${id}-more`}
                        moreLabel={`${label} 更多操作`}
                        actions={[
                            {
                                id: `copy-open-${id}`,
                                label: (
                                    <>
                                        <span aria-hidden="true">查看</span>
                                        <span className="sr-only">
                                            查看{label}
                                        </span>
                                    </>
                                ),
                                onClick: () => onOpen(row),
                            },
                        ]}
                    />
                )
            },
        },
    ]
}
