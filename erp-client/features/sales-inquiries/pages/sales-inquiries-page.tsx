"use client"

import * as React from "react"
import Link from "next/link"
import { ArrowUpRightIcon } from "lucide-react"
import { useAccountProfileQuery } from "@/features/auth/queries"
import { hasPermission } from "@/lib/permissions"
import {
    InquiryProgress,
    TRANSFER_LABEL,
    VALIDITY_LABEL,
} from "../components/inquiry-progress"
import type { ColumnDef } from "@tanstack/react-table"
import {
    BusinessFailureState,
    DataTable,
    PageScaffold,
    QuickPreviewSheet,
} from "@/components/business"
import {
    ListWorkSurface,
    ListWorkspaceHeader,
    listWorkspaceStyles,
} from "@/components/business/list-workspace"
import { OptionCombobox } from "@/components/business/option-combobox"
import { useAppForm } from "@/components/form"
import { Badge } from "@/components/ui/badge"
import { Button, buttonVariants } from "@/components/ui/button"
import { Input } from "@/components/ui/input"
import { StatusBadge } from "@/components/ui/status-badge"
import { useInquiries } from "../hooks/queries"
import { inquiryDate } from "../lib/format"
import {
    INQUIRY_STATUS_LABEL,
    type InquiryListQuery,
    type SalesInquiry,
    type ProcurementInquiry,
} from "../types"
import { toAutomationIdSegment } from "@/lib/automation-id"
import { InquiryCreateImport } from "../components/inquiry-create-import"

export function SalesInquiriesPage({
    procurement = false,
}: {
    procurement?: boolean
}) {
    const profile = useAccountProfileQuery()
    const permissions = profile.data?.permissions ?? []
    const [previewId, setPreviewId] = React.useState<string | null>(null)
    const lastFocusedRowId = React.useRef<string | null>(null)
    const openPreview = (row: SalesInquiry | ProcurementInquiry) => {
        lastFocusedRowId.current = row.id
        setPreviewId(row.id)
    }
    const [query, setQuery] = React.useState<InquiryListQuery>({
        page: 1,
        page_size: 20,
    })
    const list = useInquiries(query, procurement)
    const preview = list.data?.items.find((row) => row.id === previewId) ?? null
    const root = procurement ? "/procurement/inquiries" : "/sales/inquiries"
    const prefix = procurement ? "procurement-inquiries" : "sales-inquiries"
    const form = useAppForm({
        defaultValues: { q: "", status: "" },
        onSubmit: ({ value, formApi }) => {
            setQuery({
                q: value.q.trim() || undefined,
                status: value.status || undefined,
                page: 1,
                page_size: 20,
            })
            formApi.reset(value)
        },
    })
    const href = (row: SalesInquiry | ProcurementInquiry) =>
        `${root}/${encodeURIComponent(row.id)}${procurement && "work_items" in row && row.work_items[0] ? `?task_id=${encodeURIComponent(row.work_items[0].id)}` : ""}`
    const columns: ColumnDef<SalesInquiry | ProcurementInquiry, unknown>[] = [
        {
            accessorKey: "inquiry_no",
            header: "询价单号",
            meta: { label: "询价单号", width: "reference" },
            cell: ({ row }) => (
                <button
                    type="button"
                    id={`${prefix}-${toAutomationIdSegment(row.original.id)}-preview`}
                    className="font-medium underline underline-offset-4"
                    onClick={() => openPreview(row.original)}
                >
                    {row.original.inquiry_no}
                </button>
            ),
        },
        {
            accessorKey: "customer_name",
            header: "客户",
            meta: { label: "客户" },
        },
        {
            accessorKey: "status",
            header: "处理状态",
            meta: { label: "处理状态", width: "status" },
            cell: ({ row }) => (
                <StatusBadge
                    label={
                        INQUIRY_STATUS_LABEL[row.original.status] ?? "处理中"
                    }
                    tone={
                        row.original.status === "confirmed"
                            ? "success"
                            : row.original.status === "closed"
                              ? "neutral"
                              : "info"
                    }
                />
            ),
        },
        {
            accessorKey: "requirement_version",
            header: "需求版本",
            meta: { label: "需求版本" },
            cell: ({ row }) => (
                <Badge variant="neutral">
                    第 {row.original.requirement_version} 版
                </Badge>
            ),
        },
        {
            accessorKey: "feedback_round",
            header: "反馈轮次",
            meta: { label: "反馈轮次" },
            cell: ({ row }) => (
                <Badge variant="neutral">
                    {row.original.feedback_round
                        ? `第 ${row.original.feedback_round} 轮`
                        : "待首次反馈"}
                </Badge>
            ),
        },
        {
            accessorKey: "deadline_at",
            header: "询价截止",
            meta: { label: "询价截止" },
            cell: ({ row }) => inquiryDate(row.original.deadline_at),
        },
    ]
    if (!procurement)
        columns.splice(
            3,
            0,
            {
                id: "transfer_status",
                header: "转单进度",
                meta: { label: "转单进度", width: "status" },
                cell: ({ row }) =>
                    "sales_progress" in row.original ? (
                        <StatusBadge
                            label={
                                TRANSFER_LABEL[
                                    row.original.sales_progress.transfer_status
                                ]
                            }
                            tone={
                                row.original.sales_progress.transfer_status ===
                                "complete"
                                    ? "success"
                                    : "neutral"
                            }
                        />
                    ) : (
                        "—"
                    ),
            },
            {
                id: "quote_validity",
                header: "报价有效性",
                meta: { label: "报价有效性", width: "status" },
                cell: ({ row }) =>
                    "sales_progress" in row.original ? (
                        <StatusBadge
                            label={
                                VALIDITY_LABEL[
                                    row.original.sales_progress.quote_validity
                                ]
                            }
                            tone={
                                row.original.sales_progress.quote_validity ===
                                    "expired" ||
                                row.original.sales_progress.quote_validity ===
                                    "partially_expired"
                                    ? "warning"
                                    : "neutral"
                            }
                        />
                    ) : (
                        "—"
                    ),
            },
        )
    return (
        <PageScaffold className={listWorkspaceStyles.page}>
            <ListWorkspaceHeader
                eyebrow={procurement ? "采购" : "销售"}
                title={procurement ? "采购询价" : "销售询价"}
                description={
                    procurement
                        ? "按责任需求处理供给结果与额外成本。"
                        : "维护客户需求，跟进采购反馈并按已接受结果开单。"
                }
            >
                {hasPermission(
                    permissions,
                    procurement
                        ? "sales_inquiry:list"
                        : "procurement_inquiry:list",
                ) && (
                    <Link
                        id={`${prefix}-switch`}
                        href={
                            procurement
                                ? "/sales/inquiries"
                                : "/procurement/inquiries"
                        }
                        className={buttonVariants({
                            variant: "outline",
                            size: "sm",
                        })}
                    >
                        {procurement ? "销售询价" : "采购询价"}
                    </Link>
                )}
                {!procurement &&
                    profile.data &&
                    hasPermission(permissions, "sales_inquiry:create") &&
                    hasPermission(permissions, "sales_inquiry:import") &&
                    hasPermission(permissions, "sellable_sku:list") && (
                        <InquiryCreateImport userId={profile.data.userid} />
                    )}
                {!procurement &&
                    hasPermission(permissions, "sales_inquiry:create") &&
                    hasPermission(permissions, "sellable_sku:list") && (
                        <Link
                            id="sales-inquiries-create"
                            href="/sales/inquiries/new"
                            className={buttonVariants({ size: "sm" })}
                        >
                            新建询价
                        </Link>
                    )}
            </ListWorkspaceHeader>
            <ListWorkSurface
                ariaLabel="询价列表"
                toolbar={
                    <form
                        className="flex flex-wrap items-end gap-3"
                        onSubmit={(event) => {
                            event.preventDefault()
                            void form.handleSubmit()
                        }}
                    >
                        <form.Field
                            name="q"
                            children={(field) => (
                                <div className="min-w-56 flex-1">
                                    <label
                                        htmlFor={`${prefix}-search`}
                                        className="sr-only"
                                    >
                                        单号或客户名称
                                    </label>
                                    <Input
                                        id={`${prefix}-search`}
                                        value={field.state.value}
                                        onChange={(event) =>
                                            field.handleChange(
                                                event.target.value,
                                            )
                                        }
                                        placeholder="单号、客户名称"
                                    />
                                </div>
                            )}
                        />
                        <form.Field
                            name="status"
                            children={(field) => (
                                <OptionCombobox
                                    id={`${prefix}-status`}
                                    aria-label="处理状态"
                                    placeholder="全部状态"
                                    value={field.state.value || undefined}
                                    options={Object.entries(
                                        INQUIRY_STATUS_LABEL,
                                    )
                                        .filter(
                                            ([value]) =>
                                                value !==
                                                "awaiting_procurement",
                                        )
                                        .map(([value, label]) => ({
                                            value,
                                            label,
                                        }))}
                                    onValueChange={(value) =>
                                        field.handleChange(value ?? "")
                                    }
                                />
                            )}
                        />
                        <Button
                            id={`${prefix}-query`}
                            type="submit"
                            variant="outline"
                        >
                            查询
                        </Button>
                        <Button
                            id={`${prefix}-reset`}
                            type="button"
                            variant="ghost"
                            onClick={() => {
                                form.reset({ q: "", status: "" })
                                setQuery({ page: 1, page_size: 20 })
                            }}
                        >
                            重置
                        </Button>
                    </form>
                }
                table={
                    <DataTable
                        idPrefix={prefix}
                        data={list.data?.items ?? []}
                        columns={columns}
                        getRowId={(row) => row.id}
                        rowLabel={(row) =>
                            `${row.inquiry_no} ${row.customer_name}`
                        }
                        rowCount={list.data?.total ?? 0}
                        loading={list.isFetching}
                        pagination={{
                            pageIndex: query.page - 1,
                            pageSize: query.page_size,
                        }}
                        pageSizeOptions={[20]}
                        onPaginationChange={(next) =>
                            setQuery((current) => ({
                                ...current,
                                page: next.pageIndex + 1,
                                page_size: next.pageSize,
                            }))
                        }
                        onRowPreview={openPreview}
                        highlightedRowId={preview?.id}
                        emptyTitle="当前没有询价"
                        emptyDescription={
                            procurement
                                ? "有新的责任需求后会显示在这里。"
                                : "导入客户选品文件创建草稿，或新建询价后手动添加商品。"
                        }
                        errorState={
                            list.isError ? (
                                <BusinessFailureState
                                    id={`${prefix}-retry`}
                                    title="询价列表加载失败"
                                    error={list.error}
                                    onRetry={() => void list.refetch()}
                                />
                            ) : undefined
                        }
                    />
                }
            />
            <QuickPreviewSheet
                idPrefix={`${prefix}-preview`}
                open={preview != null}
                onOpenChange={(open) => {
                    if (!open) setPreviewId(null)
                }}
                finalFocus={() =>
                    lastFocusedRowId.current
                        ? document.querySelector<HTMLElement>(
                              `[data-row-id="${CSS.escape(lastFocusedRowId.current)}"]`,
                          )
                        : null
                }
                title={preview?.customer_name ?? "询价"}
                identity={
                    preview ? (
                        <span className="num">
                            询价单号：{preview.inquiry_no}
                        </span>
                    ) : undefined
                }
                summary={
                    preview ? (
                        <StatusBadge
                            label={INQUIRY_STATUS_LABEL[preview.status]}
                            tone="neutral"
                        />
                    ) : undefined
                }
                footer={
                    preview ? (
                        <>
                            <Button
                                id={`${prefix}-preview-dismiss`}
                                type="button"
                                variant="outline"
                                onClick={() => setPreviewId(null)}
                            >
                                关闭
                            </Button>
                            {hasPermission(
                                permissions,
                                procurement
                                    ? "procurement_inquiry:detail"
                                    : "sales_inquiry:detail",
                            ) && (
                                <Button
                                    id={`${prefix}-preview-open-detail`}
                                    render={<Link href={href(preview)} />}
                                >
                                    打开询价资料
                                    <ArrowUpRightIcon data-icon="inline-end" />
                                </Button>
                            )}
                        </>
                    ) : undefined
                }
            >
                {preview && (
                    <div className="space-y-4">
                        <p>需求商品 {preview.draft.lines.length} 项</p>
                        {"sales_progress" in preview && (
                            <InquiryProgress
                                progress={preview.sales_progress}
                            />
                        )}
                        <p className="text-sm text-muted-foreground">
                            需求第 {preview.requirement_version} 版 · 反馈第{" "}
                            {preview.feedback_round} 轮
                        </p>
                        <p className="text-sm text-muted-foreground">
                            询价截止：{inquiryDate(preview.deadline_at)}
                        </p>
                    </div>
                )}
            </QuickPreviewSheet>
        </PageScaffold>
    )
}
