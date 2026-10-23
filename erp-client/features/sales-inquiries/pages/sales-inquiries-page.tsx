"use client"

import * as React from "react"
import Link from "next/link"
import { useRouter } from "@/lib/navigation"
import { useAccountProfileQuery } from "@/features/auth/queries"
import { hasPermission } from "@/lib/permissions"
import { TRANSFER_LABEL, VALIDITY_LABEL } from "../components/inquiry-progress"
import type { ColumnDef } from "@tanstack/react-table"
import {
    BusinessFailureState,
    DataTable,
    PageScaffold,
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
    const router = useRouter()
    const profile = useAccountProfileQuery()
    const permissions = profile.data?.permissions ?? []
    const canOpenDetail = hasPermission(
        permissions,
        procurement ? "procurement_inquiry:detail" : "sales_inquiry:detail",
    )
    const [query, setQuery] = React.useState<InquiryListQuery>({
        page: 1,
        page_size: 20,
    })
    const list = useInquiries(query, procurement)
    const root = procurement ? "/procurement/inquiries" : "/sales/inquiries"
    const prefix = procurement ? "procurement-inquiries" : "sales-inquiries"
    const form = useAppForm({
        defaultValues: { q: "", status: "", taskStatus: "" },
        onSubmit: ({ value, formApi }) => {
            setQuery({
                q: value.q.trim() || undefined,
                status: value.status || undefined,
                task_status:
                    value.taskStatus === "open" ||
                    value.taskStatus === "completed"
                        ? value.taskStatus
                        : undefined,
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
            cell: ({ row }) =>
                canOpenDetail ? (
                    <Link
                        id={`${prefix}-${toAutomationIdSegment(row.original.id)}-detail`}
                        className="font-medium underline underline-offset-4"
                        href={href(row.original)}
                    >
                        {row.original.inquiry_no}
                    </Link>
                ) : (
                    <span className="font-medium">
                        {row.original.inquiry_no}
                    </span>
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
    if (procurement)
        columns.splice(3, 0, {
            id: "task_status",
            header: "我的处理进度",
            meta: { label: "我的处理进度", width: "status" },
            cell: ({ row }) => {
                const pending =
                    "work_items" in row.original &&
                    row.original.work_items.some(
                        (task) => task.status === "OPEN",
                    )
                return (
                    <StatusBadge
                        label={pending ? "待我处理" : "已处理"}
                        tone={pending ? "info" : "success"}
                    />
                )
            },
        })
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
                header: "报价状态",
                meta: { label: "报价状态", width: "status" },
                cell: ({ row }) =>
                    "sales_progress" in row.original ? (
                        <StatusBadge
                            label={
                                VALIDITY_LABEL[
                                    row.original.sales_progress.quote_validity
                                ]
                            }
                            tone="neutral"
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
                        ? "处理本人询价需求，查看已发布的反馈记录。"
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
                        {procurement && (
                            <form.Field
                                name="taskStatus"
                                children={(field) => (
                                    <OptionCombobox
                                        id={`${prefix}-task-status`}
                                        aria-label="我的处理进度"
                                        placeholder="全部处理进度"
                                        value={field.state.value || undefined}
                                        options={[
                                            {
                                                value: "open",
                                                label: "待我处理",
                                            },
                                            {
                                                value: "completed",
                                                label: "已处理",
                                            },
                                        ]}
                                        onValueChange={(value) =>
                                            field.handleChange(value ?? "")
                                        }
                                    />
                                )}
                            />
                        )}
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
                                form.reset({
                                    q: "",
                                    status: "",
                                    taskStatus: "",
                                })
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
                        onRowOpen={
                            canOpenDetail
                                ? (row) => router.push(href(row))
                                : undefined
                        }
                        emptyTitle="当前没有询价"
                        emptyDescription={
                            procurement
                                ? "没有符合当前筛选条件的询价，可调整处理进度或状态筛选。"
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
        </PageScaffold>
    )
}
