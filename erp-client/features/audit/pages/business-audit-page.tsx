"use client"

import { useCallback, useMemo, useRef, useState, type ReactNode } from "react"
import { useRouter, useSearchParams } from "next/navigation"
import {
    BusinessFailureState,
    BusinessStatusBadge,
    DataTable,
    PageScaffold,
    QuickPreviewSheet,
} from "@/components/business"
import {
    ListWorkSurface,
    ListWorkspaceHeader,
    listWorkspaceStyles,
} from "@/components/business/list-workspace"
import { Button } from "@/components/ui/button"
import { AuditLogDetails } from "../components/audit-log-details"
import {
    AuditLogFilterBar,
    type AuditLogFilters,
} from "../components/audit-log-filters"
import { useAuditLogsQuery } from "../hooks/queries"
import { useAuditLogColumns } from "../hooks/use-audit-log-columns"
import {
    auditActionLabel,
    auditObjectLabel,
    auditResultView,
} from "../lib/display"
import type { AuditLogItem, BusinessAuditResult } from "../types"

const FILTER_KEYS = {
    actorAccount: "business_actor",
    action: "business_action",
    eventResult: "business_result",
    resourceNumber: "business_number",
} as const

export function BusinessAuditPage({ views }: { views: ReactNode }) {
    const router = useRouter()
    const searchParams = useSearchParams()
    const queryString = searchParams.toString()
    const applied = useMemo<AuditLogFilters>(() => {
        const params = new URLSearchParams(queryString)
        const result = params.get(FILTER_KEYS.eventResult)
        return {
            actorAccount: params.get(FILTER_KEYS.actorAccount) ?? "",
            action: params.get(FILTER_KEYS.action) ?? "",
            resourceNumber: params.get(FILTER_KEYS.resourceNumber) ?? "",
            eventResult:
                result === "succeeded" ||
                result === "rejected" ||
                result === "unknown"
                    ? result
                    : "",
        }
    }, [queryString])
    const rawPage = Number(searchParams.get("business_page"))
    const rawSize = Number(searchParams.get("business_page_size"))
    const pagination = {
        pageIndex:
            Number.isSafeInteger(rawPage) && rawPage > 0 ? rawPage - 1 : 0,
        pageSize: [10, 20, 50, 100].includes(rawSize) ? rawSize : 20,
    }
    const query = useAuditLogsQuery({
        actor_account: applied.actorAccount.trim() || undefined,
        action: applied.action || undefined,
        resource_number: applied.resourceNumber.trim() || undefined,
        event_result: (applied.eventResult || undefined) as
            | BusinessAuditResult
            | undefined,
        page: pagination.pageIndex + 1,
        page_size: pagination.pageSize,
    })
    const [selected, setSelected] = useState<AuditLogItem | null>(null)
    const lastFocusedRowId = useRef<string | null>(null)
    const openDetails = useCallback((row: AuditLogItem) => {
        lastFocusedRowId.current = row.id
        setSelected(row)
    }, [])
    const columns = useAuditLogColumns(openDetails)
    const patchUrl = (patch: Record<string, string | null>) => {
        const params = new URLSearchParams(queryString)
        params.set("source", "business")
        for (const [key, value] of Object.entries(patch)) {
            if (value) params.set(key, value)
            else params.delete(key)
        }
        router.replace(`/system/audit?${params}`, { scroll: false })
    }
    const applyFilters = (values: AuditLogFilters) => {
        setSelected(null)
        patchUrl({
            ...Object.fromEntries(
                Object.entries(FILTER_KEYS).map(([key, param]) => [
                    param,
                    values[key as keyof AuditLogFilters].trim() || null,
                ]),
            ),
            business_page: null,
        })
    }
    const result = selected ? auditResultView(selected) : null

    return (
        <PageScaffold density="compact" className={listWorkspaceStyles.page}>
            <ListWorkspaceHeader
                eyebrow="系统"
                title="审计查询"
                description="查询业务操作发生时保存的人物、业务编号、执行结果与字段变化。"
            />
            <ListWorkSurface
                ariaLabel="业务操作记录列表"
                views={views}
                toolbar={
                    <AuditLogFilterBar
                        applied={applied}
                        onApply={applyFilters}
                        total={query.data?.total ?? 0}
                        loading={query.isFetching}
                    />
                }
                table={
                    <DataTable
                        idPrefix="business-audit-table"
                        columns={columns}
                        data={query.data?.items ?? []}
                        getRowId={(row) => row.id}
                        rowCount={query.data?.total ?? 0}
                        rowLabel={(row) =>
                            `${auditActionLabel(row)} ${auditObjectLabel(row)}`
                        }
                        pagination={pagination}
                        onPaginationChange={(next) => {
                            setSelected(null)
                            patchUrl({
                                business_page: String(next.pageIndex + 1),
                                business_page_size: String(next.pageSize),
                            })
                        }}
                        pageSizeOptions={[10, 20, 50, 100]}
                        loading={query.isPending}
                        highlightedRowId={selected?.id}
                        onRowPreview={openDetails}
                        manualPagination
                        manualSorting
                        manualFiltering
                        layout="flush"
                        errorState={
                            query.isError ? (
                                <BusinessFailureState
                                    error={query.error}
                                    action={
                                        <Button
                                            id="business-audit-retry"
                                            variant="secondary"
                                            onClick={() => void query.refetch()}
                                        >
                                            重试
                                        </Button>
                                    }
                                />
                            ) : undefined
                        }
                        emptyTitle="没有匹配的业务操作记录"
                        emptyDescription="调整业务编号、操作账号、动作或执行结果后查询。"
                    />
                }
            />
            <QuickPreviewSheet
                idPrefix="business-audit-details"
                size="detail"
                open={Boolean(selected)}
                onOpenChange={(open) => {
                    if (!open) setSelected(null)
                }}
                onOpenChangeComplete={(open) => {
                    if (!open && lastFocusedRowId.current) {
                        document
                            .querySelector<HTMLElement>(
                                `[data-row-id="${CSS.escape(lastFocusedRowId.current)}"]`,
                            )
                            ?.focus()
                    }
                }}
                title={selected ? auditActionLabel(selected) : "业务操作记录"}
                identity={
                    selected
                        ? `业务编号：${auditObjectLabel(selected)}`
                        : undefined
                }
                summary={
                    result ? (
                        <BusinessStatusBadge
                            context="preview"
                            label={result.label}
                            tone={result.tone}
                        />
                    ) : undefined
                }
                footer={
                    <Button
                        id="business-audit-details-dismiss"
                        variant="outline"
                        onClick={() => setSelected(null)}
                    >
                        关闭
                    </Button>
                }
            >
                <div className="min-h-0 flex-1 overflow-y-auto px-7 py-6">
                    {selected ? <AuditLogDetails row={selected} /> : null}
                </div>
            </QuickPreviewSheet>
        </PageScaffold>
    )
}
