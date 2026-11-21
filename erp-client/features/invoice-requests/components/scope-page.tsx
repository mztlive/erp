"use client"

import * as React from "react"
import { DownloadIcon } from "lucide-react"

import {
    BusinessStatusBadge,
    FundsScopeBanner,
    ListDataTable,
    ListFilterSection,
    ListFilterSections,
    ListSearchField,
    ListWorkspaceFilterBar,
    ListWorkspacePage,
    MoneyValue,
    TableEmptyCell,
    TableIdentityCell,
    TableTextCell,
} from "@/components/business"
import { OrganizationUnitFilter } from "@/features/organization/components/organization-unit-filter"
import { selectedOrgUnitIds } from "@/features/organization/lib/organization-ids"
import { Button } from "@/components/ui/button"
import type { StatusTone } from "@/components/ui/status-badge"
import {
    Table,
    TableBody,
    TableCell,
    TableHead,
    TableHeader,
    TableRow,
} from "@/components/ui/table"
import { useAccountProfileQuery } from "@/features/auth/queries"
import { personDirectoryLabel } from "@/features/entity-selectors/api/person-directory"
import { PersonDirectoryFilter } from "@/features/entity-selectors/components/person-directory-filter"
import { usePersonDirectorySelected } from "@/features/entity-selectors/hooks/person-directory"
import type { ColumnDef } from "@tanstack/react-table"
import { getErrorMessage } from "@/lib/api/errors"
import { restoreListRowFocus } from "@/lib/list-row-focus"
import { hasPermission } from "@/lib/permissions"
import { excelExportText, listFilterText } from "@/lib/ui-text"
import { useSearchHotkey } from "@/lib/use-search-hotkey"
import { useScopeExcelExport } from "@/lib/use-scope-list-export"
import {
    requestStatusLabels,
    type RequestStatus,
} from "@/features/invoice-requests/api"
import type { ScopedInvoiceRequestWire } from "@/features/invoice-requests/scoped"
import { exportInvoiceRequestScopeExcel } from "@/features/invoice-requests/scoped-export"
import { InvoiceRequestDetails } from "./request-details"
import {
    useInvoiceRequestScopeDetailQuery,
    useInvoiceRequestScopeListQuery,
} from "@/features/invoice-requests/hooks/scoped-queries"
import { useInvoiceRequestScopeUrlState } from "@/features/invoice-requests/hooks/use-scope-url-state"

const toolbarPrefix = "invoice-request-scope-toolbar"
const trayFilterKeys = ["applicantUserIds", "handlerUserIds", "orgUnitIds"]
const foldedChipKeys = [...trayFilterKeys, "salesOrderId", "status"]

const requestStatusTones: Record<RequestStatus, StatusTone> = {
    draft: "neutral",
    rejected: "destructive",
    in_approval: "warning",
    approved: "info",
    completed: "success",
}

type ScopedRequestWithOptionalNames = ScopedInvoiceRequestWire & {
    applicant_user_name?: string | null
    applicant_name?: string | null
    created_by_name?: string | null
    handler_user_name?: string | null
    handler_name?: string | null
}

function isInternalIdentifier(value: string): boolean {
    const trimmed = value.trim()
    return (
        /^[0-9a-f]{8}-[0-9a-f]{4}-[0-9a-f]{4}-[0-9a-f]{4}-[0-9a-f]{12}$/i.test(
            trimmed,
        ) || /^[a-z]{2,6}_[a-z0-9_-]{6,}$/i.test(trimmed)
    )
}

function requestStatus(status: string): RequestStatus | undefined {
    const normalized = status.trim().toLowerCase()
    return Object.hasOwn(requestStatusLabels, normalized)
        ? (normalized as RequestStatus)
        : undefined
}

function requestStatusLabel(status: RequestStatus | undefined): string {
    return status ? requestStatusLabels[status] : "状态待确认"
}

function resolveScopedPersonName(
    row: ScopedRequestWithOptionalNames,
    role: "applicant" | "handler",
    resolvedNames: ReadonlyMap<string, string>,
): string {
    if (role === "applicant") {
        const inline =
            row.applicant_user_name?.trim() ||
            row.applicant_name?.trim() ||
            row.created_by_name?.trim() ||
            ""
        if (inline) return inline
        const fromDirectory = resolvedNames.get(row.applicant_user_id)
        if (fromDirectory) return fromDirectory
        const rawId = row.applicant_user_id.trim()
        return !rawId || isInternalIdentifier(rawId) ? "当前不可见" : rawId
    }
    if (!row.handler_user_id) return ""
    const inline =
        row.handler_user_name?.trim() || row.handler_name?.trim() || ""
    if (inline) return inline
    const fromDirectory = resolvedNames.get(row.handler_user_id)
    if (fromDirectory) return fromDirectory
    const rawId = row.handler_user_id.trim()
    return !rawId || isInternalIdentifier(rawId) ? "当前不可见" : rawId
}

function scopeColumns(
    resolvedNames: ReadonlyMap<string, string>,
): ColumnDef<ScopedInvoiceRequestWire>[] {
    return [
        {
            id: "doc",
            header: "申请单号",
            meta: { label: "申请单号", width: "flex" },
            cell: ({ row }) => (
                <TableIdentityCell
                    primary={
                        <span className="num">{row.original.request_no}</span>
                    }
                    primaryTitle={row.original.request_no}
                    secondary={row.original.sales_order_no}
                    secondaryTitle={row.original.sales_order_no}
                    numericSecondary
                />
            ),
        },
        {
            id: "amount",
            header: "申请金额",
            meta: {
                label: "申请金额",
                width: "amount",
                align: "end",
                numeric: true,
            },
            cell: ({ row }) => (
                <MoneyValue value={row.original.amount} display="compact" />
            ),
        },
        {
            id: "people",
            header: "申请人 / 当前处理人",
            meta: { label: "经办", width: "default" },
            cell: ({ row }) => {
                const applicantLabel = resolveScopedPersonName(
                    row.original as ScopedRequestWithOptionalNames,
                    "applicant",
                    resolvedNames,
                )
                const handlerLabel = resolveScopedPersonName(
                    row.original as ScopedRequestWithOptionalNames,
                    "handler",
                    resolvedNames,
                )
                return (
                    <div className="flex min-w-0 flex-col">
                        <TableTextCell
                            value={`申请人 ${applicantLabel}`}
                            clamp={1}
                        />
                        {row.original.handler_user_id ? (
                            <TableTextCell
                                value={`当前处理人 ${handlerLabel}`}
                                clamp={1}
                                className="text-xs text-muted-foreground"
                            />
                        ) : (
                            <TableEmptyCell
                                variant="label"
                                label="暂无当前处理人"
                            />
                        )}
                    </div>
                )
            },
        },
        {
            id: "status",
            header: "状态",
            meta: { label: "状态", width: "status" },
            cell: ({ row }) => {
                const status = requestStatus(row.original.status)
                return (
                    <BusinessStatusBadge
                        context="list"
                        label={requestStatusLabel(status)}
                        tone={status ? requestStatusTones[status] : "neutral"}
                        appearance={status === "completed" ? "quiet" : "badge"}
                    />
                )
            },
        },
    ]
}

/** 开票申请范围页：负责销售、申请人与当前开票处理人分别查询，人员候选走销售目录。 */
export function InvoiceRequestScopePage() {
    const urlState = useInvoiceRequestScopeUrlState()
    useSearchHotkey(urlState.searchInputRef)
    const directoryOrgUnitIds = selectedOrgUnitIds(urlState.orgDraft)
    const listQuery = useInvoiceRequestScopeListQuery(urlState.query)
    const [detailId, setDetailId] = React.useState<string | null>(null)
    const detailQuery = useInvoiceRequestScopeDetailQuery(detailId)
    const data = listQuery.data
    const noScope = Boolean(
        data && (data.emptyReason === "no_scope" || !data.hasScope),
    )
    const rowCount = noScope || listQuery.isError ? 0 : (data?.total ?? 0)
    const rows = React.useMemo(
        () => (noScope ? [] : [...(data?.requests ?? [])]),
        [data?.requests, noScope],
    )
    const missingPersonIds = React.useMemo(() => {
        const ids = new Set<string>()
        const collect = (row: ScopedRequestWithOptionalNames) => {
            if (
                row.applicant_user_id &&
                !row.applicant_user_name?.trim() &&
                !row.applicant_name?.trim() &&
                !row.created_by_name?.trim()
            ) {
                ids.add(row.applicant_user_id)
            }
            if (
                row.handler_user_id &&
                !row.handler_user_name?.trim() &&
                !row.handler_name?.trim()
            ) {
                ids.add(row.handler_user_id)
            }
        }
        for (const row of data?.requests ?? []) {
            collect(row as ScopedRequestWithOptionalNames)
        }
        if (detailQuery.data) {
            collect(detailQuery.data as ScopedRequestWithOptionalNames)
        }
        return [...ids]
    }, [data?.requests, detailQuery.data])
    const account = useAccountProfileQuery()
    const canReadBusiness = hasPermission(
        account.data?.permissions,
        "business_person:list",
    )
    const selectedPeople = usePersonDirectorySelected(
        "business",
        missingPersonIds,
        canReadBusiness && missingPersonIds.length > 0,
    )
    const resolvedNames = React.useMemo(() => {
        const map = new Map<string, string>()
        if (selectedPeople.isSuccess && selectedPeople.data) {
            for (const item of selectedPeople.data.items) {
                map.set(item.id, item.name.trim() || personDirectoryLabel(item))
            }
        }
        return map
    }, [selectedPeople.data, selectedPeople.isSuccess])
    const columns = React.useMemo(
        () => scopeColumns(resolvedNames),
        [resolvedNames],
    )
    const exportExcel = useScopeExcelExport(exportInvoiceRequestScopeExcel)
    const canExport =
        account.isSuccess &&
        hasPermission(account.data.permissions, "sales_invoice_request:list")
    const appliedChips = [
        ...urlState.appliedChips.map((chip) => ({
            ...chip,
            onClear: () => urlState.removeFilter(chip.key),
        })),
        ...(urlState.status
            ? [
                  {
                      key: "status",
                      label: `状态：${requestStatusLabel(requestStatus(urlState.status))}`,
                      onClear: () =>
                          urlState.patchUrl(
                              { status: null, scopeVersion: null, page: null },
                              { replace: true },
                          ),
                  },
              ]
            : []),
    ]
    const hasFoldedChips = appliedChips.some(({ key }) =>
        foldedChipKeys.includes(key),
    )

    return (
        <ListWorkspacePage
            title="开票申请汇总"
            actions={
                canExport
                    ? [
                          {
                              id: "invoice-request-scope-export",
                              actionKey: "export",
                              type: "button",
                              tier: "quiet",
                              icon: DownloadIcon,
                              label: exportExcel.isPending
                                  ? excelExportText.pending
                                  : excelExportText.action,
                              loading: exportExcel.isPending,
                              disabled:
                                  listQuery.isFetching ||
                                  listQuery.isError ||
                                  !data?.hasScope ||
                                  !data.total,
                              onClick: () => exportExcel.mutate(urlState.query),
                              title: "导出当前筛选下的全部开票申请",
                          },
                      ]
                    : undefined
            }
            loading={listQuery.isPending && !data}
            notices={
                <FundsScopeBanner
                    scopeSummary={data?.scopeSummary}
                    asOf={data?.asOf}
                    permissionLimited={
                        data?.requests.some((row) => row.permission_limited) ??
                        false
                    }
                    unassigned={data?.summary?.unassigned}
                />
            }
            surface={{
                ariaLabel: "开票申请范围列表",
                toolbar: (
                    <ListWorkspaceFilterBar
                        morePresentation="popover"
                        idPrefix={toolbarPrefix}
                        formAriaLabel="开票申请范围查询"
                        onSubmit={urlState.applyFilters}
                        queryButtonId={`${toolbarPrefix}-apply`}
                        moreButtonId={`${toolbarPrefix}-more-filters`}
                        clearButtonId={`${toolbarPrefix}-clear-all`}
                        search={
                            <ListSearchField
                                id={`${toolbarPrefix}-search`}
                                searchInputRef={urlState.searchInputRef}
                                value={urlState.searchDraft}
                                onChange={urlState.setSearchDraft}
                                placeholder="申请单号、抬头、事由"
                                aria-label="搜索开票申请范围"
                            />
                        }
                        moreCount={
                            appliedChips.filter(({ key }) =>
                                trayFilterKeys.includes(key),
                            ).length
                        }
                        moreOpen={urlState.panelOpen}
                        onToggleMore={() =>
                            urlState.panelOpen
                                ? urlState.cancelMoreFilters()
                                : urlState.setPanelOpen(true)
                        }
                        morePanelId={`${toolbarPrefix}-more-panel`}
                        morePanelAriaLabel="开票申请范围更多筛选条件"
                        onResetMore={urlState.resetMoreFilters}
                        primaryFilters={
                            urlState.canUseOwner ? (
                                <div className="empty:hidden w-56 max-w-full min-w-0">
                                    <PersonDirectoryFilter
                                        id={`${toolbarPrefix}-sales-owner`}
                                        category="sales"
                                        label="负责销售"
                                        hideLabel
                                        value={urlState.salesOwnerDraft}
                                        onChange={urlState.setSalesOwnerDraft}
                                        orgUnitIds={directoryOrgUnitIds}
                                        includeDescendants={
                                            urlState.descendantsDraft
                                        }
                                    />
                                </div>
                            ) : null
                        }
                        extraPrimary={
                            urlState.hasActiveFilters && !hasFoldedChips ? (
                                <Button
                                    id={`${toolbarPrefix}-clear-all`}
                                    type="button"
                                    variant="ghost"
                                    size="sm"
                                    onClick={urlState.clearFilters}
                                >
                                    {listFilterText.clear}
                                </Button>
                            ) : null
                        }
                        morePanel={
                            urlState.canUseBusiness || urlState.canUseOrg ? (
                                <ListFilterSections>
                                    {urlState.canUseBusiness && (
                                        <ListFilterSection legend="人员">
                                            <PersonDirectoryFilter
                                                id={`${toolbarPrefix}-applicant`}
                                                category="business"
                                                label="申请人"
                                                value={urlState.applicantDraft}
                                                onChange={
                                                    urlState.setApplicantDraft
                                                }
                                                orgUnitIds={directoryOrgUnitIds}
                                                includeDescendants={
                                                    urlState.descendantsDraft
                                                }
                                            />
                                            <PersonDirectoryFilter
                                                id={`${toolbarPrefix}-handler`}
                                                category="business"
                                                label="当前开票处理人"
                                                value={urlState.handlerDraft}
                                                onChange={
                                                    urlState.setHandlerDraft
                                                }
                                                orgUnitIds={directoryOrgUnitIds}
                                                includeDescendants={
                                                    urlState.descendantsDraft
                                                }
                                            />
                                        </ListFilterSection>
                                    )}
                                    {urlState.canUseOrg && (
                                        <ListFilterSection legend="组织">
                                            <OrganizationUnitFilter
                                                compact
                                                id={`${toolbarPrefix}-org`}
                                                label="业务组织"
                                                value={urlState.orgDraft}
                                                onChange={urlState.setOrgDraft}
                                                includeDescendants={
                                                    urlState.descendantsDraft
                                                }
                                                onDescendantsChange={
                                                    urlState.setDescendantsDraft
                                                }
                                            />
                                        </ListFilterSection>
                                    )}
                                </ListFilterSections>
                            ) : null
                        }
                        resultStatus={
                            listQuery.isError
                                ? "查询未完成"
                                : listQuery.isFetching
                                  ? "查询中…"
                                  : null
                        }
                        chips={appliedChips}
                        foldedChipKeys={foldedChipKeys}
                        onClearAll={urlState.clearFilters}
                        hasPendingChanges={urlState.hasPendingChanges}
                        pendingHint="条件已修改，待查询 · 导出仍按已生效条件"
                    />
                ),
                table: (
                    <ListDataTable
                        id="invoice-request-scope-list"
                        caption="开票申请范围列表"
                        entityName="开票申请"
                        data={rows}
                        columns={columns}
                        defaultColumnOrder={[
                            "doc",
                            "amount",
                            "people",
                            "status",
                        ]}
                        defaultColumnPinning={{ left: ["doc"] }}
                        getRowId={(row) => row.id}
                        rowLabel={(row) => row.request_no}
                        onRowPreview={(row) => setDetailId(row.id)}
                        highlightedRowId={detailId ?? undefined}
                        rowCount={rowCount}
                        pagination={urlState.pagination}
                        pageSizeOptions={[urlState.pagination.pageSize]}
                        onPaginationChange={(next) => {
                            if (
                                listQuery.isPlaceholderData ||
                                next.pageIndex === urlState.pagination.pageIndex
                            )
                                return
                            urlState.handlePaginationChange(
                                next,
                                data?.scopeVersion,
                            )
                        }}
                        showPagination={rowCount > 0}
                        loading={listQuery.isFetching}
                        isError={listQuery.isError}
                        error={listQuery.error}
                        onRetry={() => void listQuery.refetch()}
                        retryButtonId="invoice-request-scope-list-retry"
                        noScope={noScope}
                        hasActiveFilters={urlState.hasActiveFilters}
                        onClearFilters={urlState.clearFilters}
                        clearFiltersButtonId="invoice-request-scope-empty-clear-filters"
                        emptyCopy={{
                            noScopeTitle: "暂无可查看的开票申请数据",
                            noScopeDescription:
                                "您当前尚未配置开票申请的数据查看范围，请联系管理员开通权限后再查询。",
                            filterTitle: "无匹配开票申请",
                            filterDescription: "无匹配记录，可清除筛选后重试。",
                            emptyTitle: "暂无开票申请",
                        }}
                    />
                ),
            }}
        >
            {detailId ? (
                <section
                    aria-label="开票申请范围详情"
                    className="min-w-0 rounded-xl border p-4"
                >
                    <div className="flex min-w-0 flex-wrap items-center justify-between gap-2">
                        <h2 className="min-w-0 break-words text-section-title font-semibold tracking-tight">
                            申请详情
                        </h2>
                        <Button
                            id="invoice-request-scope-detail-close"
                            type="button"
                            variant="outline"
                            size="sm"
                            onClick={() => {
                                setDetailId(null)
                                restoreListRowFocus(detailId)
                            }}
                        >
                            关闭
                        </Button>
                    </div>
                    {detailQuery.isPending ? (
                        <p className="mt-2 text-xs text-muted-foreground">
                            正在读取申请详情…
                        </p>
                    ) : detailQuery.isError ? (
                        <div role="alert" className="mt-2 space-y-2">
                            <p className="text-xs text-muted-foreground">
                                {getErrorMessage(
                                    detailQuery.error,
                                    "申请详情读取失败",
                                )}
                            </p>
                            <Button
                                id="invoice-request-scope-detail-retry"
                                type="button"
                                variant="outline"
                                size="sm"
                                onClick={() => void detailQuery.refetch()}
                            >
                                重试
                            </Button>
                        </div>
                    ) : detailQuery.data ? (
                        <div className="mt-2 overflow-x-auto">
                            <Table>
                                <TableHeader>
                                    <TableRow>
                                        {[
                                            "申请单号",
                                            "销售单",
                                            "申请金额",
                                            "申请人",
                                            "当前处理人",
                                            "状态",
                                        ].map((label) => (
                                            <TableHead key={label}>
                                                {label}
                                            </TableHead>
                                        ))}
                                    </TableRow>
                                </TableHeader>
                                <TableBody>
                                    <TableRow>
                                        <TableCell>
                                            {detailQuery.data.request_no}
                                        </TableCell>
                                        <TableCell>
                                            {detailQuery.data.sales_order_no}
                                        </TableCell>
                                        <TableCell>
                                            <MoneyValue
                                                value={detailQuery.data.amount}
                                            />
                                        </TableCell>
                                        <TableCell>
                                            {resolveScopedPersonName(
                                                detailQuery.data,
                                                "applicant",
                                                resolvedNames,
                                            )}
                                        </TableCell>
                                        <TableCell>
                                            {detailQuery.data.handler_user_id
                                                ? resolveScopedPersonName(
                                                      detailQuery.data,
                                                      "handler",
                                                      resolvedNames,
                                                  )
                                                : "—"}
                                        </TableCell>
                                        <TableCell>
                                            {requestStatusLabel(
                                                requestStatus(
                                                    detailQuery.data.status,
                                                ),
                                            )}
                                        </TableCell>
                                    </TableRow>
                                </TableBody>
                            </Table>
                            <div className="mt-4">
                                <InvoiceRequestDetails
                                    data={detailQuery.data.data}
                                />
                            </div>
                            {detailQuery.data.permission_limited ? (
                                <p className="mt-2 text-xs text-muted-foreground">
                                    部分金额受限时仅显示本人权限内金额
                                </p>
                            ) : null}
                        </div>
                    ) : (
                        <p className="mt-2 text-xs text-muted-foreground">
                            未找到该申请，可能已超出当前数据范围。
                        </p>
                    )}
                </section>
            ) : null}
        </ListWorkspacePage>
    )
}
