"use client"

import { useState } from "react"
import { ChevronDownIcon, CopyIcon } from "lucide-react"

import {
    BusinessFailureState,
    DetailWorkspaceSection,
    MoneyValue,
    detailWorkspaceStyles,
} from "@/components/business"
import { Button } from "@/components/ui/button"
import {
    Collapsible,
    CollapsibleContent,
    CollapsibleTrigger,
} from "@/components/ui/collapsible"
import { StatusBadge } from "@/components/ui/status-badge"
import { toast } from "@/components/ui/toast"
import { toAutomationIdSegment } from "@/lib/automation-id"
import { formatDate, unixSecondsToIsoOrEmpty } from "@/lib/datetime"
import { requestStatusLabels } from "../api"
import type { useInvoiceRequests } from "../hooks/queries"

/** 完整列表确认全部完成后才默认收起，分页或错误不隐藏待办。 */
export function SalesInvoiceRequestRecords({
    list,
    page,
    onPage,
    onView,
    multipleParties,
}: {
    list: ReturnType<typeof useInvoiceRequests>
    page: number
    onPage: (page: number) => void
    onView: (id: string) => void
    multipleParties: boolean
}) {
    const [expanded, setExpanded] = useState<boolean>()
    const allCompleted =
        list.isSuccess &&
        list.data.total > 0 &&
        list.data.total === list.data.items.length &&
        list.data.items.every((request) => request.status === "completed")
    const isOpen = expanded ?? !allCompleted
    return (
        <DetailWorkspaceSection width="full">
            <Collapsible open={isOpen} onOpenChange={setExpanded}>
                <CollapsibleTrigger
                    id="sales-invoice-requests-toggle"
                    className="flex w-full cursor-pointer items-center justify-between gap-3 rounded-md focus-visible:outline-none focus-visible:ring-2 focus-visible:ring-ring"
                >
                    <span className="flex items-center gap-2">
                        <h2 className="text-section-title font-semibold tracking-tight text-foreground">
                            申请记录
                        </h2>
                        {list.data ? (
                            <span className="num rounded-md bg-muted px-1.5 py-0.5 text-xs font-normal text-muted-foreground">
                                {list.data.total} 条
                            </span>
                        ) : null}
                    </span>
                    <span className="flex items-center gap-1.5 text-xs font-normal text-muted-foreground">
                        {isOpen ? "收起" : "展开"}
                        <ChevronDownIcon
                            className={`size-4 transition-transform ${isOpen ? "rotate-180" : ""}`}
                        />
                    </span>
                </CollapsibleTrigger>
                <CollapsibleContent>
                    {list.isPending ? (
                        <p
                            role="status"
                            className="py-4 text-xs text-muted-foreground"
                        >
                            正在读取申请记录…
                        </p>
                    ) : list.isError ? (
                        <div className="pt-3">
                            <BusinessFailureState
                                title="申请记录读取失败"
                                error={list.error}
                                onRetry={() => void list.refetch()}
                            />
                        </div>
                    ) : list.data.items.length ? (
                        <ul className="mt-3 divide-y divide-border/60 border-y border-border/60">
                            {list.data.items.map((request) => (
                                <li
                                    key={request.id}
                                    className="grid min-w-0 grid-cols-1 gap-3 py-3.5 sm:grid-cols-[minmax(0,1fr)_auto]"
                                >
                                    <div className="min-w-0">
                                        <div className="flex flex-wrap items-center gap-2">
                                            <Button
                                                id={`invoice-request-view-${toAutomationIdSegment(request.id)}`}
                                                variant="link"
                                                className="h-auto min-w-0 max-w-52 justify-start p-0"
                                                title={request.request_no}
                                                onClick={() =>
                                                    onView(request.id)
                                                }
                                            >
                                                <span className="num truncate">
                                                    {request.request_no}
                                                </span>
                                            </Button>
                                            <Button
                                                id={`invoice-request-copy-${toAutomationIdSegment(request.id)}`}
                                                variant="ghost"
                                                size="icon-xs"
                                                aria-label="复制完整申请单号"
                                                title="复制完整申请单号"
                                                onClick={async () => {
                                                    try {
                                                        await navigator.clipboard.writeText(
                                                            request.request_no,
                                                        )
                                                        toast.add({
                                                            type: "success",
                                                            title: "申请单号已复制",
                                                        })
                                                    } catch {
                                                        toast.add({
                                                            type: "error",
                                                            title: "复制失败，请打开申请详情复制",
                                                        })
                                                    }
                                                }}
                                            >
                                                <CopyIcon className="size-3.5" />
                                            </Button>
                                            <StatusBadge
                                                label={
                                                    requestStatusLabels[
                                                        request.status
                                                    ]
                                                }
                                                tone={
                                                    request.status ===
                                                    "completed"
                                                        ? "success"
                                                        : request.status ===
                                                            "draft"
                                                          ? "neutral"
                                                          : "warning"
                                                }
                                            />
                                        </div>
                                        <p className="mt-1 text-xs text-muted-foreground">
                                            {request.created_by_name ||
                                                "申请人未记录"}{" "}
                                            ·{" "}
                                            <span className="num">
                                                {request.created_at > 0
                                                    ? formatDate(
                                                          unixSecondsToIsoOrEmpty(
                                                              request.created_at,
                                                          ),
                                                      )
                                                    : "日期未记录"}
                                            </span>
                                        </p>
                                        {multipleParties ? (
                                            <p className="mt-1 text-xs text-muted-foreground wrap-anywhere">
                                                {request.data.invoice_title}
                                            </p>
                                        ) : null}
                                    </div>
                                    <dl className="flex gap-6 text-body-compact sm:text-right">
                                        <div>
                                            <dt className="text-xs text-muted-foreground">
                                                申请金额
                                            </dt>
                                            <dd className="mt-1">
                                                <MoneyValue
                                                    value={request.data.amount}
                                                />
                                            </dd>
                                        </div>
                                        <div>
                                            <dt className="text-xs text-muted-foreground">
                                                已开票
                                            </dt>
                                            <dd className="mt-1">
                                                <MoneyValue
                                                    value={
                                                        request.invoiced_amount
                                                    }
                                                />
                                            </dd>
                                        </div>
                                    </dl>
                                </li>
                            ))}
                        </ul>
                    ) : (
                        <p className="py-4 text-xs text-muted-foreground">
                            暂无开票申请
                        </p>
                    )}
                    {list.data && (list.data.total > 10 || page > 1) ? (
                        <div className="mt-3 flex flex-wrap items-center justify-end gap-2 text-xs text-muted-foreground">
                            <span className="num">
                                第 {page} 页 · 共 {list.data.total} 条
                            </span>
                            <Button
                                id="invoice-request-page-prev"
                                size="sm"
                                variant="ghost"
                                className={detailWorkspaceStyles.subtleButton}
                                disabled={page === 1}
                                onClick={() => onPage(page - 1)}
                            >
                                上一页
                            </Button>
                            <Button
                                id="invoice-request-page-next"
                                size="sm"
                                variant="ghost"
                                className={detailWorkspaceStyles.subtleButton}
                                disabled={page * 10 >= list.data.total}
                                onClick={() => onPage(page + 1)}
                            >
                                下一页
                            </Button>
                        </div>
                    ) : null}
                </CollapsibleContent>
            </Collapsible>
        </DetailWorkspaceSection>
    )
}
