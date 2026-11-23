"use client"

import { useState } from "react"
import Link from "next/link"
import { useQuery } from "@tanstack/react-query"
import {
    ArrowUpRightIcon,
    ClipboardListIcon,
    FileSpreadsheetIcon,
    PlusIcon,
    ReceiptIcon,
    ScaleIcon,
    ShoppingCartIcon,
    TruckIcon,
    WalletCardsIcon,
} from "lucide-react"

import { MoneyValue } from "@/components/business"
import { Button, buttonVariants } from "@/components/ui/button"
import { Skeleton } from "@/components/ui/skeleton"
import { StatusBadge } from "@/components/ui/status-badge"
import {
    Table,
    TableBody,
    TableCell,
    TableHead,
    TableHeader,
    TableRow,
} from "@/components/ui/table"
import { Tabs, TabsList, TabsTrigger } from "@/components/ui/tabs"
import {
    fetchPeriodBasisConfig,
    fetchProfitLossView,
} from "@/features/actual-profit-loss/api"
import type { AccountProfile } from "@/features/auth/api"
import { fetchReceivableScopeList } from "@/features/customer-receivables/api/scoped-view"
import { decodeFulfillmentQueuePage } from "@/features/fulfillment-operations/api/work-queue"
import {
    OPERATION_TYPE_LABEL,
    type FulfillmentOperationType,
} from "@/features/fulfillment-operations/types"
import { fetchPurchaseOrders } from "@/features/purchase-orders/api/purchase-order-queries-api"
import type { PurchaseOrderStatusFilter } from "@/features/purchase-orders/types"
import { fetchInquiries } from "@/features/sales-inquiries/api"
import {
    INQUIRY_STATUS_LABEL,
    INQUIRY_STATUS_TONE,
    type InquirySalesProgress,
    type SalesInquiry,
} from "@/features/sales-inquiries/types"
import { fetchSalesOrders } from "@/features/sales-orders/api/sales-orders-list"
import { fetchSupplierScopeList } from "@/features/supplier-payables/api/scoped-view"
import { apiGet } from "@/lib/api"
import { toAutomationIdSegment } from "@/lib/automation-id"
import { compareDecimal } from "@/lib/fixed-decimal"
import { hasPermission } from "@/lib/permissions"
import { queryKeyRoots } from "@/lib/query-key-roots"
import type { RoleWorkbenchId } from "@/lib/role-workbenches"
import { cn } from "@/lib/utils"
import { buildWorkspaceDetailHref } from "@/lib/workspace-detail-href"
import { overviewIdentity } from "../hooks/use-workspace-overview"
import { workbenchTime } from "./workbench-data-info"

const PANEL = "min-w-0 py-5 pb-8"
const PAGE_SIZE = 6

const TRANSFER_STATUS_LABEL: Record<
    InquirySalesProgress["transfer_status"],
    string
> = {
    not_started: "未转单",
    partial: "部分转单",
    complete: "已转单",
}

const SALES_STATUS_FILTERS = [
    { value: "all", label: "全部" },
    { value: "draft", label: "草稿" },
    { value: "pending_review", label: "审核中" },
    { value: "effective", label: "已生效" },
] as const

const SALES_INQUIRY_FILTERS = [
    { value: "all", label: "全部" },
    { value: "awaiting_sales", label: "待销售处理" },
    { value: "awaiting_sales_supplement", label: "待销售补充" },
    { value: "procurement_processing", label: "采购处理中" },
] as const

const PURCHASE_STATUS_FILTERS: readonly {
    value: PurchaseOrderStatusFilter
    label: string
}[] = [
    { value: "all", label: "全部" },
    { value: "DRAFT", label: "草稿" },
    { value: "PENDING_REVIEW", label: "审批中" },
    { value: "EFFECTIVE", label: "已生效" },
    { value: "PARTIAL", label: "部分执行" },
]

export function RoleWorkbenchActivity({
    workbenchId,
    profile,
    returnTo = `/workspace/roles/${workbenchId}`,
    period,
}: {
    workbenchId: RoleWorkbenchId
    profile: AccountProfile
    returnTo?: string
    period?: { from: string; to: string }
}) {
    if (workbenchId === "WB-01") {
        return <SalesActivityPanel profile={profile} />
    }
    if (workbenchId === "WB-02") {
        return (
            <FulfillmentActivityPanel profile={profile} returnTo={returnTo} />
        )
    }
    if (workbenchId === "WB-03") {
        return <ProcurementActivityPanel profile={profile} />
    }
    if (workbenchId === "WB-05") {
        return <ManagementActivityPanel profile={profile} period={period} />
    }
    return <CashierActivityPanel profile={profile} />
}

function FilterPills<T extends string>({
    idPrefix,
    options,
    value,
    onChange,
}: {
    idPrefix: string
    options: readonly { value: T; label: string }[]
    value: T
    onChange: (next: T) => void
}) {
    return (
        <div
            role="group"
            aria-label="状态筛选"
            className="flex flex-wrap items-center gap-1"
        >
            {options.map((option) => {
                const active = option.value === value
                return (
                    <button
                        key={option.value}
                        id={`${idPrefix}-filter-${toAutomationIdSegment(option.value)}`}
                        type="button"
                        aria-pressed={active}
                        onClick={() => onChange(option.value)}
                        className={cn(
                            "h-control-xs rounded-sm px-2 text-xs font-medium transition-colors focus-visible:outline-2 focus-visible:outline-primary",
                            active
                                ? "bg-sidebar-accent text-sidebar-accent-foreground"
                                : "text-muted-foreground hover:bg-muted hover:text-foreground",
                        )}
                    >
                        {option.label}
                    </button>
                )
            })}
        </div>
    )
}

function ActivityTableSkeleton({ columns }: { columns: number }) {
    return (
        <div
            className="space-y-2.5 p-4"
            role="status"
            aria-label="正在加载业务记录"
        >
            {Array.from({ length: 4 }, (_, rowIndex) => (
                <div
                    key={rowIndex}
                    className="grid items-center gap-3"
                    style={{
                        gridTemplateColumns: `repeat(${columns}, minmax(0, 1fr))`,
                    }}
                >
                    {Array.from({ length: columns }, (_, colIndex) => (
                        <Skeleton key={colIndex} className="h-4 w-full" />
                    ))}
                </div>
            ))}
        </div>
    )
}

function ActivityErrorRow({
    id,
    message,
    onRetry,
}: {
    id: string
    message: string
    onRetry: () => void
}) {
    return (
        <div
            role="status"
            className="flex flex-wrap items-center justify-between gap-2 px-4 py-6 text-xs text-muted-foreground"
        >
            <span>{message}</span>
            <Button id={id} variant="outline" size="xs" onClick={onRetry}>
                重新加载
            </Button>
        </div>
    )
}

function ActivityEmptyState({
    title,
    description,
    actionId,
    actionLabel,
    actionHref,
}: {
    title: string
    description: string
    actionId?: string
    actionLabel?: string
    actionHref?: string
}) {
    return (
        <div className="flex flex-col items-center justify-center gap-1.5 px-4 py-8 text-center">
            <p className="text-body-compact font-medium text-foreground">
                {title}
            </p>
            <p className="text-xs text-muted-foreground">{description}</p>
            {actionId && actionLabel && actionHref ? (
                <Link
                    id={actionId}
                    href={actionHref}
                    prefetch={false}
                    className={cn(
                        buttonVariants({ variant: "outline", size: "xs" }),
                        "mt-2",
                    )}
                >
                    <PlusIcon data-icon="inline-start" aria-hidden="true" />
                    {actionLabel}
                </Link>
            ) : null}
        </div>
    )
}

function SalesActivityPanel({ profile }: { profile: AccountProfile }) {
    const canOrders = hasPermission(profile.permissions, "sales_order:list")
    const canInquiries = hasPermission(
        profile.permissions,
        "sales_inquiry:list",
    )
    const canCreateOrder = hasPermission(
        profile.permissions,
        "sales_order:create",
    )
    const canCreateInquiry =
        hasPermission(profile.permissions, "sales_inquiry:create") &&
        hasPermission(profile.permissions, "sellable_sku:list")

    const defaultTab = canOrders ? "orders" : "inquiries"
    const [tab, setTab] = useState<"orders" | "inquiries">(defaultTab)
    const [orderStatus, setOrderStatus] =
        useState<(typeof SALES_STATUS_FILTERS)[number]["value"]>("all")
    const [inquiryStatus, setInquiryStatus] =
        useState<(typeof SALES_INQUIRY_FILTERS)[number]["value"]>("all")

    const activeTab =
        tab === "orders" && !canOrders
            ? "inquiries"
            : tab === "inquiries" && !canInquiries
              ? "orders"
              : tab

    const identity = overviewIdentity(profile)

    const ordersQuery = useQuery({
        queryKey: [
            ...queryKeyRoots.workspaceHome,
            "sales-activity-orders",
            identity,
            orderStatus,
        ],
        queryFn: () =>
            fetchSalesOrders(
                {
                    ownerUserIds: profile.userid,
                    commercialStatus:
                        orderStatus === "all" ? undefined : orderStatus,
                    page: 1,
                    pageSize: PAGE_SIZE,
                },
                { includeContractDetails: false },
            ),
        enabled: canOrders && activeTab === "orders",
        staleTime: 60_000,
        retry: false,
        meta: { roleWorkbench: true },
    })

    const inquiriesQuery = useQuery({
        queryKey: [
            ...queryKeyRoots.workspaceHome,
            "sales-activity-inquiries",
            identity,
            inquiryStatus,
        ],
        queryFn: () =>
            fetchInquiries(
                {
                    status: inquiryStatus === "all" ? undefined : inquiryStatus,
                    page: 1,
                    page_size: PAGE_SIZE,
                },
                false,
            ),
        enabled: canInquiries && activeTab === "inquiries",
        staleTime: 60_000,
        retry: false,
        meta: { roleWorkbench: true },
    })

    if (!canOrders && !canInquiries) return null

    const inquiryItems = (inquiriesQuery.data?.items ?? []).slice(
        0,
        PAGE_SIZE,
    ) as SalesInquiry[]

    return (
        <section
            className={PANEL}
            aria-labelledby="sales-workbench-activity-title"
        >
            <Tabs
                value={activeTab}
                onValueChange={(next) => {
                    if (next === "orders" || next === "inquiries") setTab(next)
                }}
                className="gap-0"
            >
                <div className="flex flex-wrap items-center justify-between gap-x-4 gap-y-2 border-b border-border/60">
                    <div className="flex items-center gap-4">
                        <h2
                            id="sales-workbench-activity-title"
                            className="sr-only"
                        >
                            跟进中的销售业务
                        </h2>
                        <TabsList variant="line">
                            {canOrders && (
                                <TabsTrigger
                                    id="sales-activity-tab-orders"
                                    value="orders"
                                >
                                    <ShoppingCartIcon
                                        className="size-3.5"
                                        aria-hidden="true"
                                    />
                                    我负责的销售单
                                    {ordersQuery.data &&
                                    ordersQuery.data.emptyReason !==
                                        "no_scope" ? (
                                        <span className="num rounded-full bg-muted px-1.5 text-tiny font-medium text-muted-foreground">
                                            {ordersQuery.data.total}
                                        </span>
                                    ) : null}
                                </TabsTrigger>
                            )}
                            {canInquiries && (
                                <TabsTrigger
                                    id="sales-activity-tab-inquiries"
                                    value="inquiries"
                                >
                                    <FileSpreadsheetIcon
                                        className="size-3.5"
                                        aria-hidden="true"
                                    />
                                    销售询价
                                    {inquiriesQuery.data &&
                                    !inquiriesQuery.data.no_scope ? (
                                        <span className="num rounded-full bg-muted px-1.5 text-tiny font-medium text-muted-foreground">
                                            {inquiriesQuery.data.total}
                                        </span>
                                    ) : null}
                                </TabsTrigger>
                            )}
                        </TabsList>
                    </div>
                    <div className="py-1.5">
                        {activeTab === "orders" ? (
                            <FilterPills
                                idPrefix="sales-activity-orders"
                                options={SALES_STATUS_FILTERS}
                                value={orderStatus}
                                onChange={setOrderStatus}
                            />
                        ) : (
                            <FilterPills
                                idPrefix="sales-activity-inquiries"
                                options={SALES_INQUIRY_FILTERS}
                                value={inquiryStatus}
                                onChange={setInquiryStatus}
                            />
                        )}
                    </div>
                </div>

                {activeTab === "orders" ? (
                    ordersQuery.isPending ? (
                        <ActivityTableSkeleton columns={5} />
                    ) : ordersQuery.isError ? (
                        <ActivityErrorRow
                            id="sales-activity-orders-retry"
                            message="销售单列表读取失败，请重试。"
                            onRetry={() => void ordersQuery.refetch()}
                        />
                    ) : ordersQuery.data?.emptyReason === "no_scope" ? (
                        <ActivityEmptyState
                            title="暂无销售单查看范围"
                            description="当前账号未配置销售单数据查看范围。"
                        />
                    ) : !ordersQuery.data?.items.length ? (
                        <ActivityEmptyState
                            title={
                                orderStatus === "all"
                                    ? "当前没有负责的销售单"
                                    : "当前状态下没有销售单"
                            }
                            description={
                                orderStatus === "all"
                                    ? "可直接新建销售单，或切换至销售询价继续跟进。"
                                    : "可切换上方状态筛选查看其他单据。"
                            }
                            actionId={
                                canCreateOrder
                                    ? "sales-activity-empty-new-order"
                                    : undefined
                            }
                            actionLabel={
                                canCreateOrder ? "新建销售单" : undefined
                            }
                            actionHref={
                                canCreateOrder
                                    ? "/sales/orders?mode=create"
                                    : undefined
                            }
                        />
                    ) : (
                        <div className="overflow-x-auto">
                            <Table>
                                <TableHeader>
                                    <TableRow>
                                        <TableHead className="pl-0">
                                            销售单 / 客户
                                        </TableHead>
                                        <TableHead>状态</TableHead>
                                        <TableHead>履约 / 回款</TableHead>
                                        <TableHead className="text-right">
                                            含税金额
                                        </TableHead>
                                        <TableHead className="w-16 pr-0 text-right">
                                            操作
                                        </TableHead>
                                    </TableRow>
                                </TableHeader>
                                <TableBody>
                                    {ordersQuery.data.items.map((row) => (
                                        <TableRow key={row.id}>
                                            <TableCell className="max-w-64 py-2.5 pl-0">
                                                <Link
                                                    id={`sales-activity-order-${toAutomationIdSegment(row.id)}`}
                                                    href={`/sales/orders/${encodeURIComponent(row.id)}`}
                                                    className="num block truncate font-medium text-foreground hover:text-primary hover:underline focus-visible:outline-2 focus-visible:outline-primary"
                                                >
                                                    {row.documentNumber}
                                                </Link>
                                                <p className="truncate text-xs text-muted-foreground">
                                                    {row.customerName}
                                                </p>
                                            </TableCell>
                                            <TableCell className="py-2.5">
                                                <StatusBadge
                                                    tone={
                                                        row.primaryStatus.tone
                                                    }
                                                    label={
                                                        row.primaryStatus.label
                                                    }
                                                />
                                                {row.primaryStatus
                                                    .ownerUserName ? (
                                                    <p className="mt-0.5 truncate text-xs text-muted-foreground">
                                                        当前：
                                                        {
                                                            row.primaryStatus
                                                                .ownerUserName
                                                        }
                                                    </p>
                                                ) : null}
                                            </TableCell>
                                            <TableCell className="py-2.5">
                                                <div className="flex flex-wrap items-center gap-x-3 gap-y-0.5">
                                                    <StatusBadge
                                                        appearance="quiet"
                                                        tone={
                                                            row.fulfillment.tone
                                                        }
                                                        label={
                                                            row.fulfillment
                                                                .label
                                                        }
                                                    />
                                                    <StatusBadge
                                                        appearance="quiet"
                                                        tone={
                                                            row.collection.tone
                                                        }
                                                        label={
                                                            row.collection.label
                                                        }
                                                    />
                                                </div>
                                            </TableCell>
                                            <TableCell className="num py-2.5 text-right font-medium text-foreground">
                                                <MoneyValue
                                                    value={row.amountGross}
                                                    display="exact"
                                                />
                                            </TableCell>
                                            <TableCell className="py-2.5 pr-0 text-right">
                                                <Link
                                                    id={`sales-activity-order-open-${toAutomationIdSegment(row.id)}`}
                                                    href={`/sales/orders/${encodeURIComponent(row.id)}`}
                                                    className="inline-flex items-center gap-0.5 rounded-sm text-xs font-medium text-primary hover:underline focus-visible:outline-2 focus-visible:outline-primary"
                                                >
                                                    查看
                                                    <ArrowUpRightIcon
                                                        className="size-3"
                                                        aria-hidden="true"
                                                    />
                                                </Link>
                                            </TableCell>
                                        </TableRow>
                                    ))}
                                </TableBody>
                            </Table>
                        </div>
                    )
                ) : inquiriesQuery.isPending ? (
                    <ActivityTableSkeleton columns={5} />
                ) : inquiriesQuery.isError ? (
                    <ActivityErrorRow
                        id="sales-activity-inquiries-retry"
                        message="销售询价列表读取失败，请重试。"
                        onRetry={() => void inquiriesQuery.refetch()}
                    />
                ) : inquiriesQuery.data?.no_scope ? (
                    <ActivityEmptyState
                        title="暂无销售询价查看范围"
                        description="当前账号未配置销售询价数据查看范围。"
                    />
                ) : !inquiryItems.length ? (
                    <ActivityEmptyState
                        title={
                            inquiryStatus === "all"
                                ? "当前没有进行中的销售询价"
                                : "当前状态下没有销售询价"
                        }
                        description={
                            inquiryStatus === "all"
                                ? "需要向采购询价确认货期和价格时，可快速发起新询价。"
                                : "可切换上方状态筛选查看其他询价单。"
                        }
                        actionId={
                            canCreateInquiry
                                ? "sales-activity-empty-new-inquiry"
                                : undefined
                        }
                        actionLabel={
                            canCreateInquiry ? "新建销售询价" : undefined
                        }
                        actionHref={
                            canCreateInquiry
                                ? "/sales/inquiries/new"
                                : undefined
                        }
                    />
                ) : (
                    <div className="overflow-x-auto">
                        <Table>
                            <TableHeader>
                                <TableRow>
                                    <TableHead className="pl-0">
                                        询价单 / 客户
                                    </TableHead>
                                    <TableHead>状态</TableHead>
                                    <TableHead>明细 / 转单</TableHead>
                                    <TableHead>截止时间</TableHead>
                                    <TableHead className="w-16 pr-0 text-right">
                                        操作
                                    </TableHead>
                                </TableRow>
                            </TableHeader>
                            <TableBody>
                                {inquiryItems.map((row) => {
                                    const lineCount =
                                        row.sales_progress?.total_lines ??
                                        row.draft?.lines?.length ??
                                        0
                                    return (
                                        <TableRow key={row.id}>
                                            <TableCell className="max-w-64 py-2.5 pl-0">
                                                <Link
                                                    id={`sales-activity-inquiry-${toAutomationIdSegment(row.id)}`}
                                                    href={`/sales/inquiries/${encodeURIComponent(row.id)}`}
                                                    className="num block truncate font-medium text-foreground hover:text-primary hover:underline focus-visible:outline-2 focus-visible:outline-primary"
                                                >
                                                    {row.inquiry_no}
                                                </Link>
                                                <p className="truncate text-xs text-muted-foreground">
                                                    {row.customer_name}
                                                </p>
                                            </TableCell>
                                            <TableCell className="py-2.5">
                                                <StatusBadge
                                                    tone={
                                                        INQUIRY_STATUS_TONE[
                                                            row.status
                                                        ]
                                                    }
                                                    label={
                                                        INQUIRY_STATUS_LABEL[
                                                            row.status
                                                        ]
                                                    }
                                                />
                                            </TableCell>
                                            <TableCell className="py-2.5 text-xs text-muted-foreground">
                                                <span className="num text-foreground">
                                                    {lineCount} 项
                                                </span>
                                                {row.sales_progress ? (
                                                    <span>
                                                        {" · "}
                                                        {
                                                            TRANSFER_STATUS_LABEL[
                                                                row
                                                                    .sales_progress
                                                                    .transfer_status
                                                            ]
                                                        }
                                                    </span>
                                                ) : null}
                                            </TableCell>
                                            <TableCell className="num py-2.5 text-xs text-muted-foreground">
                                                {row.deadline_at
                                                    ? workbenchTime(
                                                          row.deadline_at,
                                                          true,
                                                      )
                                                    : "—"}
                                            </TableCell>
                                            <TableCell className="py-2.5 pr-0 text-right">
                                                <Link
                                                    id={`sales-activity-inquiry-open-${toAutomationIdSegment(row.id)}`}
                                                    href={`/sales/inquiries/${encodeURIComponent(row.id)}`}
                                                    className="inline-flex items-center gap-0.5 rounded-sm text-xs font-medium text-primary hover:underline focus-visible:outline-2 focus-visible:outline-primary"
                                                >
                                                    查看
                                                    <ArrowUpRightIcon
                                                        className="size-3"
                                                        aria-hidden="true"
                                                    />
                                                </Link>
                                            </TableCell>
                                        </TableRow>
                                    )
                                })}
                            </TableBody>
                        </Table>
                    </div>
                )}

                <div className="flex flex-wrap items-center justify-between gap-2 border-t border-border/60 pt-3 text-xs text-muted-foreground">
                    <span>
                        {activeTab === "orders"
                            ? ordersQuery.data
                                ? `展示最新 ${ordersQuery.data.items.length} 条，共 ${ordersQuery.data.total} 单`
                                : "本人负责的销售单"
                            : inquiriesQuery.data
                              ? `展示最新 ${inquiryItems.length} 条，共 ${inquiriesQuery.data.total} 项`
                              : "授权范围内的销售询价"}
                    </span>
                    <Link
                        id={
                            activeTab === "orders"
                                ? "sales-activity-view-all-orders"
                                : "sales-activity-view-all-inquiries"
                        }
                        href={
                            activeTab === "orders"
                                ? "/sales/orders"
                                : "/sales/inquiries"
                        }
                        className="inline-flex items-center gap-1 rounded-sm font-medium text-primary hover:underline focus-visible:outline-2 focus-visible:outline-primary"
                    >
                        {activeTab === "orders"
                            ? "查看全部销售单"
                            : "查看全部销售询价"}
                        <ArrowUpRightIcon
                            className="size-3.5"
                            aria-hidden="true"
                        />
                    </Link>
                </div>
            </Tabs>
        </section>
    )
}

function ProcurementActivityPanel({ profile }: { profile: AccountProfile }) {
    const canOrders = hasPermission(profile.permissions, "purchase_order:list")
    const canInquiries = hasPermission(
        profile.permissions,
        "procurement_inquiry:list",
    )
    const [tab, setTab] = useState<"orders" | "inquiries">(
        canOrders ? "orders" : "inquiries",
    )
    const [status, setStatus] = useState<PurchaseOrderStatusFilter>("all")

    const activeTab =
        tab === "orders" && !canOrders
            ? "inquiries"
            : tab === "inquiries" && !canInquiries
              ? "orders"
              : tab

    const identity = overviewIdentity(profile)

    const ordersQuery = useQuery({
        queryKey: [
            ...queryKeyRoots.workspaceHome,
            "procurement-activity-orders",
            identity,
            status,
        ],
        queryFn: () =>
            fetchPurchaseOrders({
                ownerUserIds: profile.userid,
                status: status === "all" ? undefined : status,
                page: 1,
                pageSize: PAGE_SIZE,
            }),
        enabled: canOrders && activeTab === "orders",
        staleTime: 60_000,
        retry: false,
        meta: { roleWorkbench: true },
    })

    const inquiriesQuery = useQuery({
        queryKey: [
            ...queryKeyRoots.workspaceHome,
            "procurement-activity-inquiries",
            identity,
        ],
        queryFn: () =>
            fetchInquiries(
                { task_status: "open", page: 1, page_size: PAGE_SIZE },
                true,
            ),
        enabled: canInquiries && activeTab === "inquiries",
        staleTime: 60_000,
        retry: false,
        meta: { roleWorkbench: true },
    })

    if (!canOrders && !canInquiries) return null

    const inquiryItems = (inquiriesQuery.data?.items ?? []).slice(0, PAGE_SIZE)

    return (
        <section
            className={PANEL}
            aria-labelledby="procurement-workbench-activity-title"
        >
            <Tabs
                value={activeTab}
                onValueChange={(next) => {
                    if (next === "orders" || next === "inquiries") setTab(next)
                }}
                className="gap-0"
            >
                <div className="flex flex-wrap items-center justify-between gap-x-4 gap-y-2 border-b border-border/60">
                    <h2
                        id="procurement-workbench-activity-title"
                        className="sr-only"
                    >
                        采购业务明细
                    </h2>
                    <TabsList variant="line">
                        {canOrders && (
                            <TabsTrigger
                                id="procurement-activity-tab-orders"
                                value="orders"
                            >
                                <ClipboardListIcon
                                    className="size-3.5"
                                    aria-hidden="true"
                                />
                                我负责的采购单
                                {ordersQuery.data &&
                                ordersQuery.data.emptyReason !== "no_scope" ? (
                                    <span className="num rounded-full bg-muted px-1.5 text-tiny font-medium text-muted-foreground">
                                        {ordersQuery.data.total}
                                    </span>
                                ) : null}
                            </TabsTrigger>
                        )}
                        {canInquiries && (
                            <TabsTrigger
                                id="procurement-activity-tab-inquiries"
                                value="inquiries"
                            >
                                <FileSpreadsheetIcon
                                    className="size-3.5"
                                    aria-hidden="true"
                                />
                                待处理采购询价
                                {inquiriesQuery.data &&
                                !inquiriesQuery.data.no_scope ? (
                                    <span className="num rounded-full bg-muted px-1.5 text-tiny font-medium text-muted-foreground">
                                        {inquiriesQuery.data.total}
                                    </span>
                                ) : null}
                            </TabsTrigger>
                        )}
                    </TabsList>
                    {activeTab === "orders" && (
                        <div className="py-1.5">
                            <FilterPills
                                idPrefix="procurement-activity-orders"
                                options={PURCHASE_STATUS_FILTERS}
                                value={status}
                                onChange={setStatus}
                            />
                        </div>
                    )}
                </div>

                {activeTab === "orders" ? (
                    ordersQuery.isPending ? (
                        <ActivityTableSkeleton columns={5} />
                    ) : ordersQuery.isError ? (
                        <ActivityErrorRow
                            id="procurement-activity-orders-retry"
                            message="采购单列表读取失败，请重试。"
                            onRetry={() => void ordersQuery.refetch()}
                        />
                    ) : !ordersQuery.data?.rows.length ? (
                        <ActivityEmptyState
                            title="当前没有匹配的采购单"
                            description="可切换状态筛选或进入采购订单列表查看全部单据。"
                        />
                    ) : (
                        <div className="overflow-x-auto">
                            <Table>
                                <TableHeader>
                                    <TableRow>
                                        <TableHead className="pl-0">
                                            采购单 / 供应商
                                        </TableHead>
                                        <TableHead>来源销售单</TableHead>
                                        <TableHead>状态</TableHead>
                                        <TableHead className="text-right">
                                            含税金额
                                        </TableHead>
                                        <TableHead className="w-16 pr-0 text-right">
                                            操作
                                        </TableHead>
                                    </TableRow>
                                </TableHeader>
                                <TableBody>
                                    {ordersQuery.data.rows.map((row) => (
                                        <TableRow key={row.purchaseOrderId}>
                                            <TableCell className="max-w-60 py-2.5 pl-0">
                                                <Link
                                                    id={`procurement-activity-order-${toAutomationIdSegment(row.purchaseOrderId)}`}
                                                    href={`/procurement/orders/${encodeURIComponent(row.purchaseOrderId)}`}
                                                    className="num block truncate font-medium text-foreground hover:text-primary hover:underline focus-visible:outline-2 focus-visible:outline-primary"
                                                >
                                                    {row.purchaseNo ||
                                                        row.draftLabel ||
                                                        "采购单"}
                                                </Link>
                                                <p className="truncate text-xs text-muted-foreground">
                                                    {row.supplierName}
                                                </p>
                                            </TableCell>
                                            <TableCell className="num py-2.5 text-xs text-muted-foreground">
                                                {row.salesOrderNo || "—"}
                                            </TableCell>
                                            <TableCell className="py-2.5">
                                                <StatusBadge
                                                    tone={row.statusTone}
                                                    label={row.statusLabel}
                                                />
                                            </TableCell>
                                            <TableCell className="num py-2.5 text-right font-medium text-foreground">
                                                {row.costMasked ? (
                                                    <span className="text-xs font-normal text-muted-foreground">
                                                        金额受限
                                                    </span>
                                                ) : (
                                                    <MoneyValue
                                                        value={row.grossAmount}
                                                        display="exact"
                                                    />
                                                )}
                                            </TableCell>
                                            <TableCell className="py-2.5 pr-0 text-right">
                                                <Link
                                                    id={`procurement-activity-order-open-${toAutomationIdSegment(row.purchaseOrderId)}`}
                                                    href={`/procurement/orders/${encodeURIComponent(row.purchaseOrderId)}`}
                                                    className="inline-flex items-center gap-0.5 rounded-sm text-xs font-medium text-primary hover:underline focus-visible:outline-2 focus-visible:outline-primary"
                                                >
                                                    查看
                                                    <ArrowUpRightIcon
                                                        className="size-3"
                                                        aria-hidden="true"
                                                    />
                                                </Link>
                                            </TableCell>
                                        </TableRow>
                                    ))}
                                </TableBody>
                            </Table>
                        </div>
                    )
                ) : inquiriesQuery.isPending ? (
                    <ActivityTableSkeleton columns={4} />
                ) : inquiriesQuery.isError ? (
                    <ActivityErrorRow
                        id="procurement-activity-inquiries-retry"
                        message="采购询价列表读取失败，请重试。"
                        onRetry={() => void inquiriesQuery.refetch()}
                    />
                ) : !inquiryItems.length ? (
                    <ActivityEmptyState
                        title="当前没有待处理采购询价"
                        description="新的销售询价分配到您名下后将展示在此处。"
                    />
                ) : (
                    <div className="overflow-x-auto">
                        <Table>
                            <TableHeader>
                                <TableRow>
                                    <TableHead className="pl-0">
                                        询价单 / 客户
                                    </TableHead>
                                    <TableHead>状态</TableHead>
                                    <TableHead>截止时间</TableHead>
                                    <TableHead className="w-16 pr-0 text-right">
                                        操作
                                    </TableHead>
                                </TableRow>
                            </TableHeader>
                            <TableBody>
                                {inquiryItems.map((row) => (
                                    <TableRow key={row.id}>
                                        <TableCell className="max-w-64 py-2.5 pl-0">
                                            <Link
                                                id={`procurement-activity-inquiry-${toAutomationIdSegment(row.id)}`}
                                                href={`/procurement/inquiries/${encodeURIComponent(row.id)}`}
                                                className="num block truncate font-medium text-foreground hover:text-primary hover:underline focus-visible:outline-2 focus-visible:outline-primary"
                                            >
                                                {row.inquiry_no}
                                            </Link>
                                            <p className="truncate text-xs text-muted-foreground">
                                                {row.customer_name}
                                            </p>
                                        </TableCell>
                                        <TableCell className="py-2.5">
                                            <StatusBadge
                                                tone={
                                                    INQUIRY_STATUS_TONE[
                                                        row.status
                                                    ]
                                                }
                                                label={
                                                    INQUIRY_STATUS_LABEL[
                                                        row.status
                                                    ]
                                                }
                                            />
                                        </TableCell>
                                        <TableCell className="num py-2.5 text-xs text-muted-foreground">
                                            {row.deadline_at
                                                ? workbenchTime(
                                                      row.deadline_at,
                                                      true,
                                                  )
                                                : "—"}
                                        </TableCell>
                                        <TableCell className="py-2.5 pr-0 text-right">
                                            <Link
                                                id={`procurement-activity-inquiry-open-${toAutomationIdSegment(row.id)}`}
                                                href={`/procurement/inquiries/${encodeURIComponent(row.id)}`}
                                                className="inline-flex items-center gap-0.5 rounded-sm text-xs font-medium text-primary hover:underline focus-visible:outline-2 focus-visible:outline-primary"
                                            >
                                                处理
                                                <ArrowUpRightIcon
                                                    className="size-3"
                                                    aria-hidden="true"
                                                />
                                            </Link>
                                        </TableCell>
                                    </TableRow>
                                ))}
                            </TableBody>
                        </Table>
                    </div>
                )}

                <div className="flex flex-wrap items-center justify-between gap-2 border-t border-border/60 pt-3 text-xs text-muted-foreground">
                    <span>
                        {activeTab === "orders"
                            ? ordersQuery.data
                                ? `展示最新 ${ordersQuery.data.rows.length} 条，共 ${ordersQuery.data.total} 单`
                                : "本人负责的采购单"
                            : inquiriesQuery.data
                              ? `展示最新 ${inquiryItems.length} 条，共 ${inquiriesQuery.data.total} 项`
                              : "本人待处理的采购询价"}
                    </span>
                    <Link
                        id={
                            activeTab === "orders"
                                ? "procurement-activity-view-all-orders"
                                : "procurement-activity-view-all-inquiries"
                        }
                        href={
                            activeTab === "orders"
                                ? "/procurement/orders"
                                : "/procurement/inquiries"
                        }
                        className="inline-flex items-center gap-1 rounded-sm font-medium text-primary hover:underline focus-visible:outline-2 focus-visible:outline-primary"
                    >
                        {activeTab === "orders"
                            ? "查看全部采购订单"
                            : "查看全部采购询价"}
                        <ArrowUpRightIcon
                            className="size-3.5"
                            aria-hidden="true"
                        />
                    </Link>
                </div>
            </Tabs>
        </section>
    )
}

function CashierActivityPanel({ profile }: { profile: AccountProfile }) {
    const canReceipts = hasPermission(
        profile.permissions,
        "customer_receipt:list",
    )
    const canPayments = hasPermission(
        profile.permissions,
        "supplier_payment:list",
    )
    const [tab, setTab] = useState<"receipts" | "payments">(
        canReceipts ? "receipts" : "payments",
    )

    const activeTab =
        tab === "receipts" && !canReceipts
            ? "payments"
            : tab === "payments" && !canPayments
              ? "receipts"
              : tab

    const identity = overviewIdentity(profile)

    const receiptsQuery = useQuery({
        queryKey: [
            ...queryKeyRoots.workspaceHome,
            "cashier-activity-receipts",
            identity,
        ],
        queryFn: () =>
            fetchReceivableScopeList({
                view: "receipt",
                page: 1,
                pageSize: PAGE_SIZE,
            }),
        enabled: canReceipts && activeTab === "receipts",
        staleTime: 60_000,
        retry: false,
        meta: { roleWorkbench: true },
    })

    const paymentsQuery = useQuery({
        queryKey: [
            ...queryKeyRoots.workspaceHome,
            "cashier-activity-payments",
            identity,
        ],
        queryFn: () =>
            fetchSupplierScopeList({
                view: "payment",
                page: 1,
                pageSize: PAGE_SIZE,
            }),
        enabled: canPayments && activeTab === "payments",
        staleTime: 60_000,
        retry: false,
        meta: { roleWorkbench: true },
    })

    if (!canReceipts && !canPayments) return null

    return (
        <section
            className={PANEL}
            aria-labelledby="cashier-workbench-activity-title"
        >
            <Tabs
                value={activeTab}
                onValueChange={(next) => {
                    if (next === "receipts" || next === "payments") setTab(next)
                }}
                className="gap-0"
            >
                <div className="flex flex-wrap items-center justify-between gap-2 border-b border-border/60">
                    <h2
                        id="cashier-workbench-activity-title"
                        className="sr-only"
                    >
                        近期资金往来
                    </h2>
                    <TabsList variant="line">
                        {canReceipts && (
                            <TabsTrigger
                                id="cashier-activity-tab-receipts"
                                value="receipts"
                            >
                                <ReceiptIcon
                                    className="size-3.5"
                                    aria-hidden="true"
                                />
                                近期客户回款
                                {receiptsQuery.data?.hasScope ? (
                                    <span className="num rounded-full bg-muted px-1.5 text-tiny font-medium text-muted-foreground">
                                        {receiptsQuery.data.total}
                                    </span>
                                ) : null}
                            </TabsTrigger>
                        )}
                        {canPayments && (
                            <TabsTrigger
                                id="cashier-activity-tab-payments"
                                value="payments"
                            >
                                <WalletCardsIcon
                                    className="size-3.5"
                                    aria-hidden="true"
                                />
                                近期供应商付款
                                {paymentsQuery.data?.hasScope ? (
                                    <span className="num rounded-full bg-muted px-1.5 text-tiny font-medium text-muted-foreground">
                                        {paymentsQuery.data.total}
                                    </span>
                                ) : null}
                            </TabsTrigger>
                        )}
                    </TabsList>
                </div>

                {activeTab === "receipts" ? (
                    receiptsQuery.isPending ? (
                        <ActivityTableSkeleton columns={5} />
                    ) : receiptsQuery.isError ? (
                        <ActivityErrorRow
                            id="cashier-activity-receipts-retry"
                            message="客户回款读取失败，请重试。"
                            onRetry={() => void receiptsQuery.refetch()}
                        />
                    ) : !receiptsQuery.data?.receipts.length ? (
                        <ActivityEmptyState
                            title="当前没有客户回款记录"
                            description="登记客户回款后将在此展示最新资金动态。"
                        />
                    ) : (
                        <div className="overflow-x-auto">
                            <Table>
                                <TableHeader>
                                    <TableRow>
                                        <TableHead className="pl-0">
                                            回款单 / 客户
                                        </TableHead>
                                        <TableHead>到账时间</TableHead>
                                        <TableHead className="text-right">
                                            回款金额
                                        </TableHead>
                                        <TableHead className="text-right">
                                            未分配余额
                                        </TableHead>
                                        <TableHead className="w-16 pr-0 text-right">
                                            操作
                                        </TableHead>
                                    </TableRow>
                                </TableHeader>
                                <TableBody>
                                    {receiptsQuery.data.receipts.map((row) => {
                                        const hasUnallocated =
                                            row.unallocated_amount != null &&
                                            compareDecimal(
                                                row.unallocated_amount,
                                                "0",
                                                2,
                                            ) > 0
                                        const href = `/finance/customer-accounts?view=receipt&previewKind=receipt&previewId=${encodeURIComponent(row.id)}`
                                        return (
                                            <TableRow key={row.id}>
                                                <TableCell className="max-w-56 py-2.5 pl-0">
                                                    <Link
                                                        id={`cashier-activity-receipt-${toAutomationIdSegment(row.id)}`}
                                                        href={href}
                                                        className="num block truncate font-medium text-foreground hover:text-primary hover:underline focus-visible:outline-2 focus-visible:outline-primary"
                                                    >
                                                        {row.receipt_no}
                                                    </Link>
                                                    <p className="truncate text-xs text-muted-foreground">
                                                        {row.customer_name ||
                                                            row.counterparty_party_name ||
                                                            "—"}
                                                    </p>
                                                </TableCell>
                                                <TableCell className="num py-2.5 text-xs text-muted-foreground">
                                                    {workbenchTime(
                                                        row.received_at,
                                                        true,
                                                    )}
                                                </TableCell>
                                                <TableCell className="num py-2.5 text-right text-foreground">
                                                    {row.amount == null ? (
                                                        <span className="text-xs text-muted-foreground">
                                                            受限
                                                        </span>
                                                    ) : (
                                                        <MoneyValue
                                                            value={row.amount}
                                                            display="exact"
                                                        />
                                                    )}
                                                </TableCell>
                                                <TableCell
                                                    className={cn(
                                                        "num py-2.5 text-right font-medium",
                                                        hasUnallocated
                                                            ? "text-primary"
                                                            : "text-muted-foreground",
                                                    )}
                                                >
                                                    {row.unallocated_amount ==
                                                    null ? (
                                                        <span className="text-xs font-normal">
                                                            受限
                                                        </span>
                                                    ) : (
                                                        <MoneyValue
                                                            value={
                                                                row.unallocated_amount
                                                            }
                                                            display="exact"
                                                        />
                                                    )}
                                                </TableCell>
                                                <TableCell className="py-2.5 pr-0 text-right">
                                                    <Link
                                                        id={`cashier-activity-receipt-open-${toAutomationIdSegment(row.id)}`}
                                                        href={href}
                                                        className="inline-flex items-center gap-0.5 rounded-sm text-xs font-medium text-primary hover:underline focus-visible:outline-2 focus-visible:outline-primary"
                                                    >
                                                        查看
                                                        <ArrowUpRightIcon
                                                            className="size-3"
                                                            aria-hidden="true"
                                                        />
                                                    </Link>
                                                </TableCell>
                                            </TableRow>
                                        )
                                    })}
                                </TableBody>
                            </Table>
                        </div>
                    )
                ) : paymentsQuery.isPending ? (
                    <ActivityTableSkeleton columns={5} />
                ) : paymentsQuery.isError ? (
                    <ActivityErrorRow
                        id="cashier-activity-payments-retry"
                        message="供应商付款读取失败，请重试。"
                        onRetry={() => void paymentsQuery.refetch()}
                    />
                ) : !paymentsQuery.data?.payments.length ? (
                    <ActivityEmptyState
                        title="当前没有供应商付款记录"
                        description="完成付款办理后将在此展示最新资金动态。"
                    />
                ) : (
                    <div className="overflow-x-auto">
                        <Table>
                            <TableHeader>
                                <TableRow>
                                    <TableHead className="pl-0">
                                        付款单 / 供应商
                                    </TableHead>
                                    <TableHead>付款时间</TableHead>
                                    <TableHead className="text-right">
                                        付款金额
                                    </TableHead>
                                    <TableHead className="text-right">
                                        未分配余额
                                    </TableHead>
                                    <TableHead className="w-16 pr-0 text-right">
                                        操作
                                    </TableHead>
                                </TableRow>
                            </TableHeader>
                            <TableBody>
                                {paymentsQuery.data.payments.map((row) => {
                                    const hasUnallocated =
                                        row.unallocated_amount != null &&
                                        compareDecimal(
                                            row.unallocated_amount,
                                            "0",
                                            2,
                                        ) > 0
                                    const href = `/finance/supplier-accounts?view=payment&previewKind=payment&detailId=${encodeURIComponent(row.id)}`
                                    return (
                                        <TableRow key={row.id}>
                                            <TableCell className="max-w-56 py-2.5 pl-0">
                                                <Link
                                                    id={`cashier-activity-payment-${toAutomationIdSegment(row.id)}`}
                                                    href={href}
                                                    className="num block truncate font-medium text-foreground hover:text-primary hover:underline focus-visible:outline-2 focus-visible:outline-primary"
                                                >
                                                    {row.payment_no}
                                                </Link>
                                                <p className="truncate text-xs text-muted-foreground">
                                                    {row.supplier_name || "—"}
                                                </p>
                                            </TableCell>
                                            <TableCell className="num py-2.5 text-xs text-muted-foreground">
                                                {workbenchTime(
                                                    row.paid_at,
                                                    true,
                                                )}
                                            </TableCell>
                                            <TableCell className="num py-2.5 text-right text-foreground">
                                                {row.amount == null ? (
                                                    <span className="text-xs text-muted-foreground">
                                                        受限
                                                    </span>
                                                ) : (
                                                    <MoneyValue
                                                        value={row.amount}
                                                        display="exact"
                                                    />
                                                )}
                                            </TableCell>
                                            <TableCell
                                                className={cn(
                                                    "num py-2.5 text-right font-medium",
                                                    hasUnallocated
                                                        ? "text-primary"
                                                        : "text-muted-foreground",
                                                )}
                                            >
                                                {row.unallocated_amount ==
                                                null ? (
                                                    <span className="text-xs font-normal">
                                                        受限
                                                    </span>
                                                ) : (
                                                    <MoneyValue
                                                        value={
                                                            row.unallocated_amount
                                                        }
                                                        display="exact"
                                                    />
                                                )}
                                            </TableCell>
                                            <TableCell className="py-2.5 pr-0 text-right">
                                                <Link
                                                    id={`cashier-activity-payment-open-${toAutomationIdSegment(row.id)}`}
                                                    href={href}
                                                    className="inline-flex items-center gap-0.5 rounded-sm text-xs font-medium text-primary hover:underline focus-visible:outline-2 focus-visible:outline-primary"
                                                >
                                                    查看
                                                    <ArrowUpRightIcon
                                                        className="size-3"
                                                        aria-hidden="true"
                                                    />
                                                </Link>
                                            </TableCell>
                                        </TableRow>
                                    )
                                })}
                            </TableBody>
                        </Table>
                    </div>
                )}

                <div className="flex flex-wrap items-center justify-between gap-2 border-t border-border/60 pt-3 text-xs text-muted-foreground">
                    <span>
                        {activeTab === "receipts"
                            ? receiptsQuery.data
                                ? `展示最新 ${receiptsQuery.data.receipts.length} 条，共 ${receiptsQuery.data.total} 笔`
                                : "授权范围内的客户回款"
                            : paymentsQuery.data
                              ? `展示最新 ${paymentsQuery.data.payments.length} 条，共 ${paymentsQuery.data.total} 笔`
                              : "授权范围内的供应商付款"}
                    </span>
                    <Link
                        id={
                            activeTab === "receipts"
                                ? "cashier-activity-view-all-receipts"
                                : "cashier-activity-view-all-payments"
                        }
                        href={
                            activeTab === "receipts"
                                ? "/finance/customer-accounts?view=receipt"
                                : "/finance/supplier-accounts?view=payment"
                        }
                        className="inline-flex items-center gap-1 rounded-sm font-medium text-primary hover:underline focus-visible:outline-2 focus-visible:outline-primary"
                    >
                        {activeTab === "receipts"
                            ? "查看客户资金往来"
                            : "查看供应商资金往来"}
                        <ArrowUpRightIcon
                            className="size-3.5"
                            aria-hidden="true"
                        />
                    </Link>
                </div>
            </Tabs>
        </section>
    )
}

const FULFILLMENT_LANES: readonly {
    value: FulfillmentOperationType
    label: string
    permission: string
}[] = [
    { value: "RECEIPT", label: "入库", permission: "purchase_receipt:list" },
    { value: "WAREHOUSE_SHIP", label: "仓发", permission: "delivery:list" },
    { value: "SUPPLIER_DIRECT", label: "直发", permission: "delivery:list" },
    {
        value: "ELECTRONIC",
        label: "电子交付",
        permission: "electronic_delivery:list",
    },
    {
        value: "SERVICE",
        label: "线下服务",
        permission: "service_fulfillment:list",
    },
]

function FulfillmentActivityPanel({
    profile,
    returnTo,
}: {
    profile: AccountProfile
    returnTo: string
}) {
    const allowedLanes = FULFILLMENT_LANES.filter((lane) =>
        hasPermission(profile.permissions, lane.permission),
    )
    const [lane, setLane] = useState<"all" | FulfillmentOperationType>("all")
    const filterOptions = [
        { value: "all" as const, label: "全部" },
        ...allowedLanes.map((item) => ({
            value: item.value,
            label: item.label,
        })),
    ]
    const identity = overviewIdentity(profile)
    const canList = hasPermission(profile.permissions, "work_item:list")

    const queueQuery = useQuery({
        queryKey: [
            ...queryKeyRoots.workspaceHome,
            "fulfillment-activity-queue",
            identity,
            lane,
        ],
        queryFn: async () =>
            decodeFulfillmentQueuePage(
                await apiGet<unknown>("/admin/work-items/fulfillment-queue", {
                    operation_types: lane === "all" ? undefined : lane,
                    page: 1,
                    page_size: PAGE_SIZE,
                    timezone: "Asia/Shanghai",
                }),
            ),
        enabled: canList && allowedLanes.length > 0,
        staleTime: 30_000,
        retry: false,
        meta: { roleWorkbench: true },
    })

    if (!canList || allowedLanes.length === 0) return null

    const rows = queueQuery.data?.items ?? []

    return (
        <section
            className={PANEL}
            aria-labelledby="fulfillment-workbench-activity-title"
        >
            <div className="flex flex-wrap items-center justify-between gap-x-4 gap-y-2 border-b border-border/60 pb-2.5">
                <div className="flex items-center gap-2">
                    <span
                        className="flex size-6 shrink-0 items-center justify-center rounded-md bg-teal-soft text-teal-soft-foreground"
                        aria-hidden="true"
                    >
                        <TruckIcon className="size-3.5" />
                    </span>
                    <h2
                        id="fulfillment-workbench-activity-title"
                        className="text-section-title font-semibold tracking-tight text-foreground"
                    >
                        待执行履约作业
                    </h2>
                    {queueQuery.data ? (
                        <span className="num rounded-full bg-muted px-1.5 text-tiny font-medium text-muted-foreground">
                            {queueQuery.data.total}
                        </span>
                    ) : null}
                </div>
                <FilterPills
                    idPrefix="fulfillment-activity-lane"
                    options={filterOptions}
                    value={lane}
                    onChange={setLane}
                />
            </div>

            {queueQuery.isPending ? (
                <ActivityTableSkeleton columns={5} />
            ) : queueQuery.isError ? (
                <ActivityErrorRow
                    id="fulfillment-activity-retry"
                    message="履约作业列表读取失败，请重试。"
                    onRetry={() => void queueQuery.refetch()}
                />
            ) : !rows.length ? (
                <ActivityEmptyState
                    title={
                        lane === "all"
                            ? "当前没有待执行的履约作业"
                            : "当前作业类型下暂无待办"
                    }
                    description="新的收货、发货或交付任务分配到您名下后会显示在这里。"
                />
            ) : (
                <div className="overflow-x-auto">
                    <Table>
                        <TableHeader>
                            <TableRow>
                                <TableHead className="pl-0">
                                    单号 / 摘要
                                </TableHead>
                                <TableHead>作业类型</TableHead>
                                <TableHead>仓库 / 预付门控</TableHead>
                                <TableHead>截止时间</TableHead>
                                <TableHead className="w-16 pr-0 text-right">
                                    操作
                                </TableHead>
                            </TableRow>
                        </TableHeader>
                        <TableBody>
                            {rows.map((row) => {
                                const taskHref = buildWorkspaceDetailHref(
                                    "tasks",
                                    row.work_item_id,
                                    { returnTo },
                                )
                                const docNo =
                                    row.sales_order_no ||
                                    row.purchase_order_no ||
                                    OPERATION_TYPE_LABEL[row.operation_type]
                                return (
                                    <TableRow key={row.work_item_id}>
                                        <TableCell className="max-w-64 py-2.5 pl-0">
                                            <Link
                                                id={`fulfillment-activity-item-${toAutomationIdSegment(row.work_item_id)}`}
                                                href={taskHref}
                                                className="num block truncate font-medium text-foreground hover:text-primary hover:underline focus-visible:outline-2 focus-visible:outline-primary"
                                            >
                                                {docNo}
                                            </Link>
                                            <p className="truncate text-xs text-muted-foreground">
                                                {row.summary}
                                            </p>
                                        </TableCell>
                                        <TableCell className="py-2.5">
                                            <StatusBadge
                                                appearance="quiet"
                                                tone="info"
                                                label={
                                                    OPERATION_TYPE_LABEL[
                                                        row.operation_type
                                                    ]
                                                }
                                            />
                                        </TableCell>
                                        <TableCell className="py-2.5 text-xs text-muted-foreground">
                                            <div className="flex flex-wrap items-center gap-1.5">
                                                {row.warehouse_label ? (
                                                    <span className="truncate text-foreground">
                                                        {row.warehouse_label}
                                                    </span>
                                                ) : null}
                                                {row.gate_state ===
                                                "BLOCKED" ? (
                                                    <StatusBadge
                                                        appearance="quiet"
                                                        tone="warning"
                                                        label="预付待完成"
                                                    />
                                                ) : row.gate_state ===
                                                  "SATISFIED" ? (
                                                    <StatusBadge
                                                        appearance="quiet"
                                                        tone="success"
                                                        label="预付已满足"
                                                    />
                                                ) : !row.warehouse_label ? (
                                                    <span>—</span>
                                                ) : null}
                                            </div>
                                        </TableCell>
                                        <TableCell className="num py-2.5 text-xs text-muted-foreground">
                                            {workbenchTime(row.due_at, true)}
                                        </TableCell>
                                        <TableCell className="py-2.5 pr-0 text-right">
                                            <Link
                                                id={`fulfillment-activity-open-${toAutomationIdSegment(row.work_item_id)}`}
                                                href={taskHref}
                                                className="inline-flex items-center gap-0.5 rounded-sm text-xs font-medium text-primary hover:underline focus-visible:outline-2 focus-visible:outline-primary"
                                            >
                                                处理
                                                <ArrowUpRightIcon
                                                    className="size-3"
                                                    aria-hidden="true"
                                                />
                                            </Link>
                                        </TableCell>
                                    </TableRow>
                                )
                            })}
                        </TableBody>
                    </Table>
                </div>
            )}

            <div className="flex flex-wrap items-center justify-between gap-2 border-t border-border/60 pt-3 text-xs text-muted-foreground">
                <span>
                    {queueQuery.data
                        ? `展示最新 ${rows.length} 条，共 ${queueQuery.data.total} 项`
                        : "本人履约责任队列"}
                </span>
                <Link
                    id="fulfillment-activity-view-all"
                    href="/workspace/tasks?family=fulfillment&type=FULFILLMENT_OPERATION"
                    className="inline-flex items-center gap-1 rounded-sm font-medium text-primary hover:underline focus-visible:outline-2 focus-visible:outline-primary"
                >
                    查看全部履约待办
                    <ArrowUpRightIcon className="size-3.5" aria-hidden="true" />
                </Link>
            </div>
        </section>
    )
}

const MANAGEMENT_ORDER_FILTERS = [
    { value: "all", label: "全部" },
    { value: "pending_review", label: "审核中" },
    { value: "effective", label: "已生效" },
] as const

function ManagementActivityPanel({
    profile,
    period,
}: {
    profile: AccountProfile
    period?: { from: string; to: string }
}) {
    const canProfit = hasPermission(profile.permissions, "cost_entry:list")
    const canOrders = hasPermission(profile.permissions, "sales_order:list")
    const [tab, setTab] = useState<"profit" | "orders">(
        canProfit ? "profit" : "orders",
    )
    const [orderStatus, setOrderStatus] =
        useState<(typeof MANAGEMENT_ORDER_FILTERS)[number]["value"]>("all")

    const activeTab =
        tab === "profit" && !canProfit
            ? "orders"
            : tab === "orders" && !canOrders
              ? "profit"
              : tab

    const identity = overviewIdentity(profile)

    const profitQuery = useQuery({
        queryKey: [
            ...queryKeyRoots.workspaceHome,
            "management-activity-profit",
            identity,
            period?.from,
            period?.to,
        ],
        queryFn: async () => {
            const config = await fetchPeriodBasisConfig()
            if (!config.configuredPeriodBasis || !period) {
                return { unconfigured: true as const, view: null }
            }
            const view = await fetchProfitLossView({
                from: period.from,
                to: period.to,
                periodBasis: config.configuredPeriodBasis,
                scopeId: "authorized",
                coverage: "all",
                dimension: "sales_order",
                sort: "actualProfitLossNet:asc",
                page: 1,
                pageSize: PAGE_SIZE,
            })
            return { unconfigured: false as const, view }
        },
        enabled: canProfit && Boolean(period) && activeTab === "profit",
        staleTime: 60_000,
        retry: false,
        meta: { roleWorkbench: true },
    })

    const ordersQuery = useQuery({
        queryKey: [
            ...queryKeyRoots.workspaceHome,
            "management-activity-orders",
            identity,
            orderStatus,
        ],
        queryFn: () =>
            fetchSalesOrders(
                {
                    commercialStatus:
                        orderStatus === "all" ? undefined : orderStatus,
                    page: 1,
                    pageSize: PAGE_SIZE,
                },
                { includeContractDetails: false },
            ),
        enabled: canOrders && activeTab === "orders",
        staleTime: 60_000,
        retry: false,
        meta: { roleWorkbench: true },
    })

    if (!canProfit && !canOrders) return null

    const profitView = profitQuery.data?.view
    const profitRows = profitView?.rows.items ?? []

    return (
        <section
            className={PANEL}
            aria-labelledby="management-workbench-activity-title"
        >
            <Tabs
                value={activeTab}
                onValueChange={(next) => {
                    if (next === "profit" || next === "orders") setTab(next)
                }}
                className="gap-0"
            >
                <div className="flex flex-wrap items-center justify-between gap-x-4 gap-y-2 border-b border-border/60">
                    <h2
                        id="management-workbench-activity-title"
                        className="sr-only"
                    >
                        经营盈亏与重点订单
                    </h2>
                    <TabsList variant="line">
                        {canProfit && (
                            <TabsTrigger
                                id="management-activity-tab-profit"
                                value="profit"
                            >
                                <ScaleIcon
                                    className="size-3.5"
                                    aria-hidden="true"
                                />
                                经营盈亏关注订单
                                {profitView &&
                                profitView.emptyReason !== "no_scope" ? (
                                    <span className="num rounded-full bg-muted px-1.5 text-tiny font-medium text-muted-foreground">
                                        {profitView.rows.total}
                                    </span>
                                ) : null}
                            </TabsTrigger>
                        )}
                        {canOrders && (
                            <TabsTrigger
                                id="management-activity-tab-orders"
                                value="orders"
                            >
                                <ShoppingCartIcon
                                    className="size-3.5"
                                    aria-hidden="true"
                                />
                                授权范围销售单
                                {ordersQuery.data &&
                                ordersQuery.data.emptyReason !== "no_scope" ? (
                                    <span className="num rounded-full bg-muted px-1.5 text-tiny font-medium text-muted-foreground">
                                        {ordersQuery.data.total}
                                    </span>
                                ) : null}
                            </TabsTrigger>
                        )}
                    </TabsList>
                    {activeTab === "orders" ? (
                        <div className="py-1.5">
                            <FilterPills
                                idPrefix="management-activity-orders"
                                options={MANAGEMENT_ORDER_FILTERS}
                                value={orderStatus}
                                onChange={setOrderStatus}
                            />
                        </div>
                    ) : null}
                </div>

                {activeTab === "profit" ? (
                    profitQuery.isPending ? (
                        <ActivityTableSkeleton columns={5} />
                    ) : profitQuery.isError ? (
                        <ActivityErrorRow
                            id="management-activity-profit-retry"
                            message="经营盈亏订单读取失败，请重试。"
                            onRetry={() => void profitQuery.refetch()}
                        />
                    ) : profitQuery.data?.unconfigured ? (
                        <ActivityEmptyState
                            title="尚未配置经营分析期间口径"
                            description="请先在实际经营盈亏页面配置统计期间口径。"
                        />
                    ) : profitView?.emptyReason === "no_scope" ? (
                        <ActivityEmptyState
                            title="暂无经营盈亏查看范围"
                            description="当前账号未配置经营分析数据查看范围。"
                        />
                    ) : !profitRows.length ? (
                        <ActivityEmptyState
                            title="所选期间暂无可分析订单"
                            description="可调整上方经营统计期间，或进入实际经营盈亏页面查看更多维度。"
                        />
                    ) : (
                        <div className="overflow-x-auto">
                            <Table>
                                <TableHeader>
                                    <TableRow>
                                        <TableHead className="pl-0">
                                            销售单 / 客户
                                        </TableHead>
                                        <TableHead className="text-right">
                                            不含税收入
                                        </TableHead>
                                        <TableHead className="text-right">
                                            实际盈亏
                                        </TableHead>
                                        <TableHead>成本覆盖</TableHead>
                                        <TableHead className="w-16 pr-0 text-right">
                                            操作
                                        </TableHead>
                                    </TableRow>
                                </TableHeader>
                                <TableBody>
                                    {profitRows.map((row) => {
                                        const canOpenOrder =
                                            Boolean(row.objectId) &&
                                            row.allowedDrilldowns.includes(
                                                "W05",
                                            )
                                        const href = canOpenOrder
                                            ? `/sales/orders/${encodeURIComponent(row.objectId!)}`
                                            : "/analytics/profit-loss"
                                        const profit =
                                            profitView?.fieldPermissions
                                                .canViewProfit &&
                                            row.coverageState === "COVERED"
                                                ? row.actualProfitLossNet
                                                : null
                                        const negativeProfit =
                                            profit != null &&
                                            compareDecimal(profit, "0", 2) < 0
                                        return (
                                            <TableRow key={row.rowId}>
                                                <TableCell className="max-w-60 py-2.5 pl-0">
                                                    <Link
                                                        id={`management-activity-profit-${toAutomationIdSegment(row.rowId)}`}
                                                        href={href}
                                                        className="num block truncate font-medium text-foreground hover:text-primary hover:underline focus-visible:outline-2 focus-visible:outline-primary"
                                                    >
                                                        {row.identityLabel}
                                                    </Link>
                                                    <p className="truncate text-xs text-muted-foreground">
                                                        {row.customerLabel ||
                                                            "—"}
                                                    </p>
                                                </TableCell>
                                                <TableCell className="num py-2.5 text-right text-foreground">
                                                    {profitView
                                                        ?.fieldPermissions
                                                        .canViewRevenue &&
                                                    row.netSalesRevenue !=
                                                        null ? (
                                                        <MoneyValue
                                                            value={
                                                                row.netSalesRevenue
                                                            }
                                                            display="exact"
                                                        />
                                                    ) : (
                                                        <span className="text-xs text-muted-foreground">
                                                            受限
                                                        </span>
                                                    )}
                                                </TableCell>
                                                <TableCell
                                                    className={cn(
                                                        "num py-2.5 text-right font-medium",
                                                        negativeProfit
                                                            ? "text-destructive"
                                                            : "text-foreground",
                                                    )}
                                                >
                                                    {profit == null ? (
                                                        <span className="text-xs font-normal text-muted-foreground">
                                                            暂不可用
                                                        </span>
                                                    ) : (
                                                        <MoneyValue
                                                            value={profit}
                                                            display="exact"
                                                        />
                                                    )}
                                                </TableCell>
                                                <TableCell className="py-2.5">
                                                    <StatusBadge
                                                        appearance="quiet"
                                                        tone={
                                                            row.coverageState ===
                                                            "COVERED"
                                                                ? "success"
                                                                : "warning"
                                                        }
                                                        label={
                                                            row.coverageState ===
                                                            "COVERED"
                                                                ? "成本完整"
                                                                : "部分覆盖"
                                                        }
                                                    />
                                                </TableCell>
                                                <TableCell className="py-2.5 pr-0 text-right">
                                                    <Link
                                                        id={`management-activity-profit-open-${toAutomationIdSegment(row.rowId)}`}
                                                        href={href}
                                                        className="inline-flex items-center gap-0.5 rounded-sm text-xs font-medium text-primary hover:underline focus-visible:outline-2 focus-visible:outline-primary"
                                                    >
                                                        查看
                                                        <ArrowUpRightIcon
                                                            className="size-3"
                                                            aria-hidden="true"
                                                        />
                                                    </Link>
                                                </TableCell>
                                            </TableRow>
                                        )
                                    })}
                                </TableBody>
                            </Table>
                        </div>
                    )
                ) : ordersQuery.isPending ? (
                    <ActivityTableSkeleton columns={5} />
                ) : ordersQuery.isError ? (
                    <ActivityErrorRow
                        id="management-activity-orders-retry"
                        message="销售单列表读取失败，请重试。"
                        onRetry={() => void ordersQuery.refetch()}
                    />
                ) : ordersQuery.data?.emptyReason === "no_scope" ? (
                    <ActivityEmptyState
                        title="暂无销售单查看范围"
                        description="当前账号未配置销售单数据查看范围。"
                    />
                ) : !ordersQuery.data?.items.length ? (
                    <ActivityEmptyState
                        title="当前状态下没有销售单"
                        description="可切换状态筛选或查看全部销售单。"
                    />
                ) : (
                    <div className="overflow-x-auto">
                        <Table>
                            <TableHeader>
                                <TableRow>
                                    <TableHead className="pl-0">
                                        销售单 / 客户
                                    </TableHead>
                                    <TableHead>状态</TableHead>
                                    <TableHead>履约 / 回款</TableHead>
                                    <TableHead className="text-right">
                                        含税金额
                                    </TableHead>
                                    <TableHead className="w-16 pr-0 text-right">
                                        操作
                                    </TableHead>
                                </TableRow>
                            </TableHeader>
                            <TableBody>
                                {ordersQuery.data.items.map((row) => (
                                    <TableRow key={row.id}>
                                        <TableCell className="max-w-64 py-2.5 pl-0">
                                            <Link
                                                id={`management-activity-order-${toAutomationIdSegment(row.id)}`}
                                                href={`/sales/orders/${encodeURIComponent(row.id)}`}
                                                className="num block truncate font-medium text-foreground hover:text-primary hover:underline focus-visible:outline-2 focus-visible:outline-primary"
                                            >
                                                {row.documentNumber}
                                            </Link>
                                            <p className="truncate text-xs text-muted-foreground">
                                                {row.customerName}
                                            </p>
                                        </TableCell>
                                        <TableCell className="py-2.5">
                                            <StatusBadge
                                                tone={row.primaryStatus.tone}
                                                label={row.primaryStatus.label}
                                            />
                                        </TableCell>
                                        <TableCell className="py-2.5">
                                            <div className="flex flex-wrap items-center gap-x-3 gap-y-0.5">
                                                <StatusBadge
                                                    appearance="quiet"
                                                    tone={row.fulfillment.tone}
                                                    label={
                                                        row.fulfillment.label
                                                    }
                                                />
                                                <StatusBadge
                                                    appearance="quiet"
                                                    tone={row.collection.tone}
                                                    label={row.collection.label}
                                                />
                                            </div>
                                        </TableCell>
                                        <TableCell className="num py-2.5 text-right font-medium text-foreground">
                                            <MoneyValue
                                                value={row.amountGross}
                                                display="exact"
                                            />
                                        </TableCell>
                                        <TableCell className="py-2.5 pr-0 text-right">
                                            <Link
                                                id={`management-activity-order-open-${toAutomationIdSegment(row.id)}`}
                                                href={`/sales/orders/${encodeURIComponent(row.id)}`}
                                                className="inline-flex items-center gap-0.5 rounded-sm text-xs font-medium text-primary hover:underline focus-visible:outline-2 focus-visible:outline-primary"
                                            >
                                                查看
                                                <ArrowUpRightIcon
                                                    className="size-3"
                                                    aria-hidden="true"
                                                />
                                            </Link>
                                        </TableCell>
                                    </TableRow>
                                ))}
                            </TableBody>
                        </Table>
                    </div>
                )}

                <div className="flex flex-wrap items-center justify-between gap-2 border-t border-border/60 pt-3 text-xs text-muted-foreground">
                    <span>
                        {activeTab === "profit"
                            ? profitView
                                ? `按实际盈亏升序展示 ${profitRows.length} 单，共 ${profitView.rows.total} 单`
                                : "按所选期间汇总实际经营盈亏"
                            : ordersQuery.data
                              ? `展示最新 ${ordersQuery.data.items.length} 条，共 ${ordersQuery.data.total} 单`
                              : "授权范围内的销售单"}
                    </span>
                    <Link
                        id={
                            activeTab === "profit"
                                ? "management-activity-view-all-profit"
                                : "management-activity-view-all-orders"
                        }
                        href={
                            activeTab === "profit"
                                ? "/analytics/profit-loss"
                                : "/sales/orders"
                        }
                        className="inline-flex items-center gap-1 rounded-sm font-medium text-primary hover:underline focus-visible:outline-2 focus-visible:outline-primary"
                    >
                        {activeTab === "profit"
                            ? "查看完整经营盈亏分析"
                            : "查看全部销售单"}
                        <ArrowUpRightIcon
                            className="size-3.5"
                            aria-hidden="true"
                        />
                    </Link>
                </div>
            </Tabs>
        </section>
    )
}
