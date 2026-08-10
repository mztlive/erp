"use client"

import * as React from "react"
import Link from "next/link"
import { usePathname, useRouter, useSearchParams } from "next/navigation"
import type {
  ColumnDef,
  ColumnPinningState,
  PaginationState,
} from "@tanstack/react-table"
import {
  ArrowLeftIcon,
  CheckIcon,
  ExternalLinkIcon,
  PlusIcon,
  RefreshCwIcon,
  SearchIcon,
  SendIcon,
} from "lucide-react"
import { z } from "zod"

import {
  BusinessDiffPanel,
  BusinessEmptyState,
  BusinessFailureState,
  BusinessStatusBadge,
  BusinessTableFrame,
  DataFreshness,
  DataTable,
  DocumentHeader,
  DocumentTotals,
  FormalActionConfirmDialog,
  FormalActionResult,
  GuardedBusinessAction,
  ListToolbar,
  MetricFilterItem,
  MetricStrip,
  MoneyValue,
  OptionCombobox,
  PageHeader,
  PageScaffold,
  QuickPreviewSheet,
  surfaceInsetClassName,
  surfacePanelClassName,
} from "@/components/business"
import { SupplierSearchCombobox } from "@/features/entity-selectors"
import { formatDateTime } from "@/lib/datetime"
import { getErrorMessage } from "@/lib/api/errors"
import { cn } from "@/lib/utils"
import { useAppForm } from "@/components/form"
import {
  Alert,
  AlertDescription,
  AlertTitle,
} from "@/components/ui/alert"
import { Badge } from "@/components/ui/badge"
import { Button } from "@/components/ui/button"
import {
  Card,
  CardContent,
  CardDescription,
  CardHeader,
  CardTitle,
} from "@/components/ui/card"
import { DatePicker } from "@/components/ui/date-picker"
import {
  Dialog,
  DialogContent,
  DialogDescription,
  DialogFooter,
  DialogHeader,
  DialogTitle,
} from "@/components/ui/dialog"
import {
  InputGroup,
  InputGroupAddon,
  InputGroupInput,
} from "@/components/ui/input-group"
import { Label } from "@/components/ui/label"
import { Separator } from "@/components/ui/separator"
import { Tabs, TabsList, TabsTrigger } from "@/components/ui/tabs"
import { Textarea } from "@/components/ui/textarea"
import {
  useAppendEvidenceMutation,
  useClaimReviewMutation,
  useCreateDraftMutation,
  useRefreshTrialMutation,
  useResolveDifferenceMutation,
  useReviewDecisionMutation,
  useSettlementDetailQuery,
  useSettlementListQuery,
  useSubmitReviewMutation,
} from "@/features/supplier-settlements/queries"
import type {
  DifferenceResolution,
  FormalOutcome,
  SettlementDifferenceView,
  SettlementListRow,
  SettlementSection,
} from "@/features/supplier-settlements/types"
import {
  AUDIT_ACTION_LABEL,
  DIFF_TYPE_LABEL,
  REASON_CODE_LABEL,
  RESOLUTION_LABEL,
  SECTION_LABEL,
  SECTIONS,
  STATUS_LABEL,
  VIEW_LABEL,
} from "@/features/supplier-settlements/types"
import {
  buildSettlementsSearchParams,
  parseSettlementsSearchParams,
  type SettlementsUrlState,
} from "@/features/supplier-settlements/url-state"
import { openWorkspaceLabel } from "@/lib/ui-text"
import { type ResultState } from "@/components/business/feedback"

function outcomeToResult(outcome: FormalOutcome): ResultState {
  const w12Href = outcome.payableNo
    ? `/finance/supplier-accounts?view=payable&sourceType=SUPPLIER_SETTLEMENT&q=${encodeURIComponent(outcome.payableNo)}`
    : undefined
  if (outcome.status === "succeeded") {
    return {
      status: "succeeded",
      title: outcome.title,
      description: outcome.message,
      reference: outcome.reference ?? outcome.payableNo,
      facts: outcome.facts,
      payableNo: outcome.payableNo,
      w12Href,
    }
  }
  if (outcome.status === "unknown") {
    return {
      status: "unknown",
      title: outcome.title,
      description: outcome.message,
    }
  }
  if (outcome.status === "rejected") {
    return {
      status: "rejected",
      title: outcome.title,
      description: outcome.message,
      reference: outcome.reference,
      facts: outcome.facts,
    }
  }
  return {
    status: outcome.status === "blocked" ? "blocked" : "failed",
    title: outcome.title,
    description: outcome.message,
    reference: outcome.reference,
    facts: outcome.facts,
  }
}

function newKey(prefix: string) {
  return `${prefix}_${Date.now()}_${Math.random().toString(36).slice(2, 8)}`
}

function blockerOf(
  blockers: { action: string; message: string; code: string }[],
  action: string
) {
  return blockers.find((b) => b.action === action)
}

export function SupplierSettlementsPage() {
  const router = useRouter()
  const pathname = usePathname()
  const searchParams = useSearchParams()

  const pathMatch = pathname.match(/\/supplier-api\/settlements\/([^/]+)$/)
  const pathStatementId = pathMatch?.[1]

  const urlState = React.useMemo(() => {
    const parsed = parseSettlementsSearchParams(searchParams)
    if (pathStatementId && !parsed.statementId) {
      return { ...parsed, statementId: pathStatementId }
    }
    return parsed
  }, [searchParams, pathStatementId])

  const replaceUrl = React.useCallback(
    (next: SettlementsUrlState) => {
      if (pathStatementId && next.statementId === pathStatementId) {
        const base = `/supplier-api/settlements/${pathStatementId}`
        const params = new URLSearchParams()
        if (next.section !== "overview") params.set("section", next.section)
        if (next.returnTo) params.set("returnTo", next.returnTo)
        const qs = params.toString()
        router.replace(qs ? `${base}?${qs}` : base, { scroll: false })
        return
      }
      const listPath = "/supplier-api/settlements"
      const qs = buildSettlementsSearchParams(next)
      router.replace(`${listPath}${qs}`, { scroll: false })
    },
    [pathStatementId, router]
  )

  const patchUrl = React.useCallback(
    (patch: Partial<SettlementsUrlState>) => {
      replaceUrl({ ...urlState, ...patch })
    },
    [replaceUrl, urlState]
  )

  if (urlState.statementId) {
    return (
      <SettlementCenter
        statementId={urlState.statementId}
        urlState={urlState}
        patchUrl={patchUrl}
        returnTo={urlState.returnTo}
        onBack={() =>
          patchUrl({
            statementId: undefined,
            section: "overview",
            preview: undefined,
          })
        }
      />
    )
  }

  return (
    <SettlementList
      urlState={urlState}
      patchUrl={patchUrl}
      returnTo={urlState.returnTo}
      onOpen={(id) =>
        patchUrl({ statementId: id, section: "overview", preview: undefined })
      }
    />
  )
}

function CrossEntryBanner({ returnTo }: { returnTo: string }) {
  return (
    <Alert>
      <AlertTitle>跨页面进入</AlertTitle>
      <AlertDescription>
        已按来源单据的供应商预填筛选；完成对账结算后请返回来源页。
        {" "}
        <Link className="underline" href={returnTo}>
          返回来源
        </Link>
      </AlertDescription>
    </Alert>
  )
}

function SettlementList({
  urlState,
  patchUrl,
  onOpen,
  returnTo,
}: {
  urlState: SettlementsUrlState
  patchUrl: (patch: Partial<SettlementsUrlState>) => void
  onOpen: (statementId: string) => void
  returnTo?: string
}) {
  const [searchDraft, setSearchDraft] = React.useState(urlState.q ?? "")
  const [createOpen, setCreateOpen] = React.useState(false)
  const [result, setResult] = React.useState<ResultState>(null)
  const [columnPinning] = React.useState<ColumnPinningState>({
    left: ["statementNo"],
    right: ["actions"],
  })
  const createMutation = useCreateDraftMutation()

  React.useEffect(() => {
    setSearchDraft(urlState.q ?? "")
  }, [urlState.q])

  const listQuery = useSettlementListQuery({
    view: urlState.view,
    supplierId: urlState.supplierId,
    periodFrom: urlState.periodFrom,
    periodTo: urlState.periodTo,
    status: urlState.status,
    differenceType: urlState.differenceType,
    q: urlState.q,
    page: urlState.page,
    pageSize: 50,
  })

  const data = listQuery.data
  // D22：分页以 URL 为唯一事实源（page），本地不再持有副本，避免双写漂移；
  // pageSize 固定 50 不入 URL。排序：财务列表不强制加排序（服务端无排序参数），记录在案。
  const pagination = React.useMemo<PaginationState>(
    () => ({
      pageIndex: Math.max(0, urlState.page - 1),
      pageSize: 50,
    }),
    [urlState.page]
  )

  // P4：清除=清全部筛选参数、view 回 pending（保持原清除语义）、分页回第 1 页；
  // 保留 preview/statementId/returnTo 等导航上下文。空态与工具栏常驻清除共用（D22）。
  const hasActiveFilters = Boolean(
    urlState.supplierId ||
      urlState.periodFrom ||
      urlState.periodTo ||
      urlState.status ||
      urlState.differenceType ||
      urlState.q ||
      urlState.view !== "pending"
  )
  const clearFilters = React.useCallback(() => {
    patchUrl({
      view: "pending",
      supplierId: undefined,
      status: undefined,
      differenceType: undefined,
      q: undefined,
      periodFrom: undefined,
      periodTo: undefined,
      page: 1,
    })
  }, [patchUrl])

  React.useEffect(() => {
    const onKey = (event: KeyboardEvent) => {
      if (event.key !== "/" || event.metaKey || event.ctrlKey || event.altKey)
        return
      const target = event.target as HTMLElement | null
      const tag = target?.tagName
      if (
        tag === "INPUT" ||
        tag === "TEXTAREA" ||
        tag === "SELECT" ||
        target?.isContentEditable
      ) {
        return
      }
      event.preventDefault()
      document
        .querySelector<HTMLInputElement>('[data-slot="settlement-list-search"]')
        ?.focus()
    }
    window.addEventListener("keydown", onKey)
    return () => window.removeEventListener("keydown", onKey)
  }, [])

  const previewRow =
    data?.rows.find((r) => r.statementId === urlState.preview) ?? null

  const columns = React.useMemo<ColumnDef<SettlementListRow>[]>(
    () => [
      {
        id: "statementNo",
        accessorFn: (row) => row.statementNo,
        header: "结算单号",
        meta: { label: "结算单号", width: "reference" },
        cell: ({ row }) => (
          <div className="num text-sm font-medium">
            {row.original.statementNo}
          </div>
        ),
      },
      {
        id: "supplier",
        accessorFn: (row) => row.supplierName,
        header: "供应商",
        meta: { label: "供应商" },
        cell: ({ row }) => (
          <span className="text-sm">{row.original.supplierName}</span>
        ),
      },
      {
        id: "period",
        accessorFn: (row) => row.periodLabel,
        header: "期间",
        meta: { label: "期间", width: "status" },
        cell: ({ row }) => (
          <span className="num text-sm">
            {row.original.periodStart} ~ {row.original.periodEnd}
          </span>
        ),
      },
      {
        id: "erpAmount",
        accessorFn: (row) => row.erpAmountGross,
        header: "ERP 金额",
        meta: {
          label: "ERP 计算金额（含税）",
          width: "amount",
          align: "end",
          numeric: true,
        },
        cell: ({ row }) => (
          <MoneyValue value={row.original.erpAmountGross} taxBasis="gross" />
        ),
      },
      {
        id: "supplierAmount",
        accessorFn: (row) => row.supplierAmountGross ?? "",
        header: "账单金额",
        meta: {
          label: "供应商账单金额（含税）",
          width: "amount",
          align: "end",
          numeric: true,
        },
        cell: ({ row }) =>
          row.original.supplierAmountGross != null ? (
            <MoneyValue
              value={row.original.supplierAmountGross}
              taxBasis="gross"
            />
          ) : (
            <span className="text-xs text-muted-foreground">账单未同步</span>
          ),
      },
      {
        id: "difference",
        accessorFn: (row) => row.differenceAmountGross ?? "",
        header: "差异",
        meta: {
          label: "差异金额（含税）",
          width: "amount",
          align: "end",
          numeric: true,
        },
        cell: ({ row }) => (
          <div className="text-right">
            {row.original.differenceAmountGross != null ? (
              <MoneyValue
                value={row.original.differenceAmountGross}
                taxBasis="gross"
              />
            ) : (
              <span className="text-xs text-muted-foreground">—</span>
            )}
            {row.original.differenceDirectionLabel ? (
              <div className="text-tiny text-muted-foreground">
                {row.original.differenceDirectionLabel}
              </div>
            ) : null}
            {row.original.unresolvedDifferenceCount > 0 ? (
              <Badge variant="outline" className="mt-0.5 text-2xs">
                未决 {row.original.unresolvedDifferenceCount}
              </Badge>
            ) : null}
          </div>
        ),
      },
      {
        id: "status",
        accessorFn: (row) => row.statusLabel,
        header: "状态",
        meta: { label: "状态", width: "status" },
        cell: ({ row }) => (
          <BusinessStatusBadge
            context="list"
            label={row.original.statusLabel}
            tone={row.original.statusTone}
          />
        ),
      },
      {
        id: "actors",
        accessorFn: (row) =>
          `${row.preparedByLabel}/${row.reviewedByLabel}`,
        header: "经办/复核",
        meta: { label: "经办/复核" },
        cell: ({ row }) => (
          <div className="text-xs text-muted-foreground">
            <div>经办 {row.original.preparedByLabel}</div>
            <div>复核 {row.original.reviewedByLabel}</div>
          </div>
        ),
      },
      {
        id: "actions",
        header: "操作",
        meta: { label: "操作", width: "status" },
        enableSorting: false,
        cell: ({ row }) => (
          <div className="flex flex-wrap gap-1">
            <Button
              type="button"
              size="sm"
              variant="outline"
              onClick={() =>
                patchUrl({ preview: row.original.statementId })
              }
            >
              预览
            </Button>
            <Button
              type="button"
              size="sm"
              onClick={() => onOpen(row.original.statementId)}
            >
              打开
            </Button>
          </div>
        ),
      },
    ],
    [onOpen, patchUrl]
  )

  const canCreate =
    data?.hasModulePermission && data?.hasDataScope

  const createSchema = z.object({
    supplierId: z.string().min(1, "请选择供应商"),
    periodStart: z.string().min(1, "请选择期间起"),
    periodEnd: z.string().min(1, "请选择期间止"),
  })

  const form = useAppForm({
    defaultValues: {
      supplierId: "",
      periodStart: "",
      periodEnd: "",
    },
    validators: { onChange: createSchema },
    onSubmit: async ({ value }) => {
      const outcome = await createMutation.mutateAsync({
        supplierId: value.supplierId,
        periodStart: value.periodStart,
        periodEnd: value.periodEnd,
        requestId: newKey("req"),
        idempotencyKey: newKey("create"),
      })
      setResult(outcomeToResult(outcome))
      if (outcome.status === "succeeded" && outcome.statementId) {
        setCreateOpen(false)
        form.reset()
        onOpen(outcome.statementId)
      }
    },
  })

  if (listQuery.isPending) {
    return (
      <PageScaffold>
        <div className="h-10 w-56 animate-pulse rounded-lg bg-muted" />
        <div className="h-16 animate-pulse rounded-lg bg-muted" />
        <div className="h-72 animate-pulse rounded-lg bg-muted" />
      </PageScaffold>
    )
  }

  if (listQuery.isError) {
    return (
      <PageScaffold>
        <PageHeader title="API 供应商结算" description="加载失败" />
        <BusinessFailureState
          title="结算列表加载失败"
          error={listQuery.error}
          action={
            <Button type="button" onClick={() => void listQuery.refetch()}>
              重试
            </Button>
          }
        />
      </PageScaffold>
    )
  }

  const empty = data?.emptyReason

  return (
    <PageScaffold>
      <PageHeader
        title="API 供应商结算"
        metadata={
          <DataFreshness
            updatedAt={data?.sourceAsOf ? formatDateTime(data.sourceAsOf, "default") : "—"}
            dateTime={data?.sourceAsOf}
            label="结算数据更新时间"
            state={listQuery.isFetching ? "syncing" : "fresh"}
          />
        }
        actions={
          <div className="flex flex-wrap items-center gap-2">
            <Button
              type="button"
              size="sm"
              variant="ghost"
              className="text-muted-foreground hover:text-foreground"
              onClick={() => void listQuery.refetch()}
            >
              <RefreshCwIcon className="size-3.5" aria-hidden="true" />
              刷新
            </Button>
            <div className="max-sm:hidden">
              <GuardedBusinessAction
                type="button"
                size="sm"
                disabled={!canCreate}
                reason={
                  canCreate
                    ? undefined
                    : "当前账号无模块权限或数据范围"
                }
                onClick={() => setCreateOpen(true)}
              >
                <PlusIcon className="size-3.5" aria-hidden="true" />
                新建结算草稿
              </GuardedBusinessAction>
            </div>
          </div>
        }
      />

      {returnTo ? <CrossEntryBanner returnTo={returnTo} /> : null}

      {result ? (
        <FormalActionResult
          status={
            result.status === "failed" ? "blocked" : result.status
          }
          title={result.title}
          description={result.description}
          reference={result.reference}
          facts={result.facts}
          actions={
            result.w12Href ? (
              <Button
                type="button"
                size="sm"
                render={<Link href={result.w12Href} />}
              >
                打开供应商往来应付
                <ExternalLinkIcon className="size-3.5" />
              </Button>
            ) : null
          }
        />
      ) : null}

      {data?.hasModulePermission && data.hasDataScope ? (
        <MetricStrip columns={4} aria-label="结算快捷筛选">
          {/* 指标与「差异类型」下拉为双入口（D22 保留）：指标点击时同步清 differenceType，
              避免 status 指标与差异类型组合出矛盾空结果；下拉单独选择差异类型不重置指标。 */}
          <MetricFilterItem
            label="待处理"
            value={data.totals.pendingReconcile}
            active={urlState.view === "pending" && !urlState.status}
            onClick={() =>
              patchUrl({
                view: "pending",
                status: undefined,
                differenceType: undefined,
                page: 1,
              })
            }
          />
          <MetricFilterItem
            label="有差异"
            value={data.metrics.hasDifference}
            active={urlState.status === "HAS_DIFFERENCE"}
            onClick={() =>
              patchUrl({
                view: "pending",
                status: "HAS_DIFFERENCE",
                differenceType: undefined,
                page: 1,
              })
            }
          />
          <MetricFilterItem
            label="待复核"
            value={data.metrics.pendingReview}
            active={urlState.status === "PENDING_REVIEW"}
            onClick={() =>
              patchUrl({
                view: "pending",
                status: "PENDING_REVIEW",
                differenceType: undefined,
                page: 1,
              })
            }
          />
          <MetricFilterItem
            label="已确认金额"
            value={
              <MoneyValue
                value={data.metrics.confirmedAmount}
                taxBasis="gross"
              />
            }
            active={urlState.view === "confirmed"}
            onClick={() =>
              patchUrl({
                view: "confirmed",
                status: undefined,
                differenceType: undefined,
                page: 1,
              })
            }
          />
        </MetricStrip>
      ) : null}

      <Tabs
        value={urlState.view}
        onValueChange={(v) =>
          patchUrl({
            view: v as SettlementsUrlState["view"],
            status: undefined,
            differenceType: undefined,
            page: 1,
          })
        }
      >
        <TabsList>
          {(
            Object.keys(VIEW_LABEL) as Array<keyof typeof VIEW_LABEL>
          ).map((k) => (
            <TabsTrigger key={k} value={k}>
              {VIEW_LABEL[k]}
            </TabsTrigger>
          ))}
        </TabsList>
      </Tabs>

      <BusinessTableFrame
        title="结算单列表"
        description={data?.filterSummary ?? "默认待处理"}
        toolbar={
          <ListToolbar
            search={
              <div className="flex items-center gap-2">
                <InputGroup>
                  <InputGroupAddon>
                    <SearchIcon aria-hidden="true" />
                  </InputGroupAddon>
                  <InputGroupInput
                    value={searchDraft}
                    onChange={(e) => setSearchDraft(e.target.value)}
                    onKeyDown={(e) => {
                      if (e.key === "Enter") {
                        patchUrl({
                          q: searchDraft.trim() || undefined,
                          page: 1,
                        })
                      }
                    }}
                    placeholder="结算单号、外部账单号、供应商"
                    aria-label="搜索结算单"
                    data-slot="settlement-list-search"
                  />
                </InputGroup>
                <Button
                  type="button"
                  size="sm"
                  variant="secondary"
                  onClick={() =>
                    patchUrl({
                      q: searchDraft.trim() || undefined,
                      page: 1,
                    })
                  }
                >
                  搜索
                </Button>
              </div>
            }
            filters={
              <>
                <SupplierSearchCombobox
                  value={urlState.supplierId || undefined}
                  onValueChange={(id) =>
                    patchUrl({
                      supplierId: id || undefined,
                      page: 1,
                    })
                  }
                  purpose="filter"
                  className="w-[12rem]"
                  aria-label="供应商"
                  placeholder="全部供应商"
                />
                <OptionCombobox
                  value={urlState.status || null}
                  onValueChange={(v) =>
                    patchUrl({
                      status: v || undefined,
                      page: 1,
                    })
                  }
                  options={[
                    { value: "", label: "全部状态" },
                    ...(
                      Object.keys(STATUS_LABEL) as Array<
                        keyof typeof STATUS_LABEL
                      >
                    ).map((k) => ({
                      value: k,
                      label: STATUS_LABEL[k],
                    })),
                  ]}
                  className="w-[9rem]"
                  size="sm"
                  aria-label="状态"
                  allowClear={false}
                />
                <OptionCombobox
                  value={urlState.differenceType || null}
                  onValueChange={(v) =>
                    patchUrl({
                      differenceType: (v ||
                        undefined) as SettlementsUrlState["differenceType"],
                      page: 1,
                    })
                  }
                  options={[
                    { value: "", label: "全部差异" },
                    ...(
                      Object.keys(DIFF_TYPE_LABEL) as Array<
                        keyof typeof DIFF_TYPE_LABEL
                      >
                    ).map((k) => ({
                      value: k,
                      label: DIFF_TYPE_LABEL[k],
                    })),
                  ]}
                  className="w-[9rem]"
                  size="sm"
                  aria-label="差异类型"
                  allowClear={false}
                />
              </>
            }
            secondary={
              <>
                <label className="flex items-center gap-1 text-xs text-muted-foreground">
                  期间自
                  <DatePicker
                    className="w-[9rem]"
                    value={urlState.periodFrom || undefined}
                    onValueChange={(next) =>
                      patchUrl({
                        periodFrom: next || undefined,
                        page: 1,
                      })
                    }
                  />
                </label>
                <label className="flex items-center gap-1 text-xs text-muted-foreground">
                  至
                  <DatePicker
                    className="w-[9rem]"
                    value={urlState.periodTo || undefined}
                    onValueChange={(next) =>
                      patchUrl({
                        periodTo: next || undefined,
                        page: 1,
                      })
                    }
                  />
                </label>
              </>
            }
            actions={
              <div className="flex items-center gap-2">
                <span className="text-xs text-muted-foreground" aria-live="polite">
                  共 {(data?.total ?? 0).toLocaleString("zh-CN")} 条
                </span>
                {hasActiveFilters ? (
                  <Button
                    type="button"
                    size="xs"
                    variant="ghost"
                    onClick={clearFilters}
                  >
                    清除筛选
                  </Button>
                ) : null}
              </div>
            }
          />
        }
        table={
          empty ? (
            <div className="p-6">
              {empty === "FILTER_NO_RESULT" ? (
                <BusinessEmptyState
                  kind="filter"
                  className="rounded-lg border-0 bg-transparent p-6 shadow-none ring-0"
                  title="当前筛选无结果"
                  description={`筛选摘要：${data?.filterSummary ?? "—"}。可清除筛选回到默认待处理视图。`}
                  action={
                    <Button
                      type="button"
                      variant="secondary"
                      className="rounded-lg shadow-none"
                      onClick={clearFilters}
                    >
                      清除筛选
                    </Button>
                  }
                />
              ) : (
                <BusinessEmptyState
                  kind="no-data"
                  className="rounded-lg border-0 bg-transparent p-6 shadow-none ring-0"
                  title="当前范围没有结算单"
                  description="可选择供应商与期间后重查，或新建结算草稿。"
                  action={
                    canCreate ? (
                      <Button type="button" onClick={() => setCreateOpen(true)}>
                        新建结算草稿
                      </Button>
                    ) : null
                  }
                />
              )}
            </div>
          ) : (
            <DataTable
              data={data?.rows ?? []}
              columns={columns}
              getRowId={(row) => row.statementId}
              rowCount={data?.total ?? 0}
              pagination={pagination}
              onPaginationChange={(next) => {
                // D22：只写 URL，分页由 URL 派生，消除本地/URL 双写漂移
                patchUrl({ page: next.pageIndex + 1 })
              }}
              columnPinning={columnPinning}
              enableColumnPinning
              manualPagination
              layout="flush"
              density="compact"
              onRowPreview={(row) => patchUrl({ preview: row.statementId })}
              onRowOpen={(row) => onOpen(row.statementId)}
            />
          )
        }
      />

      <QuickPreviewSheet
        open={Boolean(urlState.preview)}
        onOpenChange={(open) => {
          if (!open) patchUrl({ preview: undefined })
        }}
        size="detail"
        title={previewRow?.statementNo ?? "结算预览"}
        description={
          previewRow
            ? `${previewRow.supplierName} · ${previewRow.periodLabel}`
            : undefined
        }
      >
        {previewRow ? (
          <div className="space-y-4 p-1">
            <DocumentTotals
              title="金额摘要（含税）"
              items={[
                {
                  id: "erp",
                  label: "ERP 计算金额",
                  value: (
                    <MoneyValue
                      value={previewRow.erpAmountGross}
                      taxBasis="gross"
                    />
                  ),
                  basis: "含税",
                },
                {
                  id: "bill",
                  label: "供应商账单金额",
                  value: previewRow.supplierAmountGross ? (
                    <MoneyValue
                      value={previewRow.supplierAmountGross}
                      taxBasis="gross"
                    />
                  ) : (
                    "账单未同步"
                  ),
                  basis: "含税",
                },
                {
                  id: "diff",
                  label: "差异",
                  value: previewRow.differenceAmountGross ? (
                    <MoneyValue
                      value={previewRow.differenceAmountGross}
                      taxBasis="gross"
                    />
                  ) : (
                    "—"
                  ),
                  warning: previewRow.differenceDirectionLabel,
                },
              ]}
            />
            <div className="flex flex-wrap gap-2 text-sm text-muted-foreground">
              <span>
                经办 {previewRow.preparedByLabel} · 复核{" "}
                {previewRow.reviewedByLabel}
              </span>
              <BusinessStatusBadge
                context="list"
                label={previewRow.statusLabel}
                tone={previewRow.statusTone}
              />
            </div>
            <div className="flex flex-wrap gap-2">
              <Button
                type="button"
                onClick={() => onOpen(previewRow.statementId)}
              >
                查看详情
              </Button>
              {previewRow.unresolvedDifferenceCount > 0 ? (
                <Button
                  type="button"
                  variant="secondary"
                  onClick={() =>
                    patchUrl({
                      statementId: previewRow.statementId,
                      section: "differences",
                      preview: undefined,
                    })
                  }
                >
                  打开差异处理
                </Button>
              ) : null}
            </div>
            <p className="text-xs text-muted-foreground">
              键盘：列表 Enter 打开预览；详情页可继续提交复核并查询处理结果。
            </p>
          </div>
        ) : (
          <div className="flex flex-col items-start gap-3 p-5">
            <p className="text-sm text-muted-foreground">
              未找到预览行，可能已被移出当前筛选范围。
            </p>
            <Button
              type="button"
              size="sm"
              variant="outline"
              onClick={() => patchUrl({ preview: undefined })}
            >
              关闭预览
            </Button>
          </div>
        )}
      </QuickPreviewSheet>

      <Dialog open={createOpen} onOpenChange={setCreateOpen}>
        <DialogContent>
          <DialogHeader>
            <DialogTitle>新建结算草稿</DialogTitle>
            <DialogDescription>
              选择供应商与结算期间，创建后进入待对账。
            </DialogDescription>
          </DialogHeader>
          <form
            className="space-y-3"
            onSubmit={(e) => {
              e.preventDefault()
              void form.handleSubmit()
            }}
          >
            <form.AppField
              name="supplierId"
              children={(field) => (
                <div className="space-y-1.5">
                  <Label htmlFor="supplierId">供应商</Label>
                  <SupplierSearchCombobox
                    value={field.state.value || undefined}
                    onValueChange={(id) => field.handleChange(id ?? "")}
                    placeholder="请选择供应商"
                  />
                </div>
              )}
            />
            <form.AppField
              name="periodStart"
              children={(field) => (
                <div className="space-y-1.5">
                  <Label htmlFor="periodStart">期间起</Label>
                  <DatePicker
                    className="w-full"
                    value={field.state.value || undefined}
                    onValueChange={(next) =>
                      field.handleChange(next ?? "")
                    }
                  />
                </div>
              )}
            />
            <form.AppField
              name="periodEnd"
              children={(field) => (
                <div className="space-y-1.5">
                  <Label htmlFor="periodEnd">期间止</Label>
                  <DatePicker
                    className="w-full"
                    value={field.state.value || undefined}
                    onValueChange={(next) =>
                      field.handleChange(next ?? "")
                    }
                  />
                </div>
              )}
            />
            <DialogFooter>
              <Button
                type="button"
                variant="ghost"
                onClick={() => setCreateOpen(false)}
              >
                取消
              </Button>
              <form.AppForm>
                <form.SubmitButton label="确认创建草稿" />
              </form.AppForm>
            </DialogFooter>
          </form>
        </DialogContent>
      </Dialog>
    </PageScaffold>
  )
}

function SettlementCenter({
  statementId,
  urlState,
  patchUrl,
  returnTo,
  onBack,
}: {
  statementId: string
  urlState: SettlementsUrlState
  patchUrl: (patch: Partial<SettlementsUrlState>) => void
  returnTo?: string
  onBack: () => void
}) {
  const detailQuery = useSettlementDetailQuery(statementId)
  const refreshMutation = useRefreshTrialMutation()
  const resolveMutation = useResolveDifferenceMutation()
  const evidenceMutation = useAppendEvidenceMutation()
  const submitMutation = useSubmitReviewMutation()
  const decisionMutation = useReviewDecisionMutation()
  const claimMutation = useClaimReviewMutation()

  const [result, setResult] = React.useState<ResultState>(null)
  const [resolveOpen, setResolveOpen] = React.useState(false)
  const [evidenceOpen, setEvidenceOpen] = React.useState(false)
  const [submitOpen, setSubmitOpen] = React.useState(false)
  const [confirmOpen, setConfirmOpen] = React.useState(false)
  const [rejectOpen, setRejectOpen] = React.useState(false)
  const [resolution, setResolution] =
    React.useState<DifferenceResolution>("ERP_ACCEPTED")
  const [reasonCode, setReasonCode] = React.useState("ACCEPT_BILL")
  const [evidenceComment, setEvidenceComment] = React.useState("")
  const [rejectReason, setRejectReason] = React.useState("")
  const resultRef = React.useRef<HTMLDivElement | null>(null)

  const data = detailQuery.data
  const section = urlState.section

  React.useEffect(() => {
    if (result?.status === "succeeded" || result?.status === "unknown") {
      resultRef.current?.focus()
    }
  }, [result])

  // keyboard: d opens differences when center focused
  React.useEffect(() => {
    const onKey = (event: KeyboardEvent) => {
      const target = event.target as HTMLElement | null
      if (
        target &&
        (target.tagName === "INPUT" ||
          target.tagName === "TEXTAREA" ||
          target.tagName === "SELECT" ||
          target.isContentEditable)
      ) {
        return
      }
      if (event.key === "d" && !event.metaKey && !event.ctrlKey) {
        event.preventDefault()
        patchUrl({ section: "differences" })
      }
    }
    window.addEventListener("keydown", onKey)
    return () => window.removeEventListener("keydown", onKey)
  }, [patchUrl])

  if (detailQuery.isPending) {
    return (
      <PageScaffold>
        <div className="h-10 w-56 animate-pulse rounded-lg bg-muted" />
        <div className="h-24 animate-pulse rounded-lg bg-muted" />
        <div className="h-96 animate-pulse rounded-lg bg-muted" />
        <p className="text-sm text-muted-foreground">
          正在加载结算单，请稍候…
        </p>
      </PageScaffold>
    )
  }

  if (detailQuery.isError) {
    return (
      <PageScaffold>
        <Button type="button" variant="ghost" size="sm" onClick={onBack}>
          <ArrowLeftIcon className="size-4" />
          返回列表
        </Button>
        <BusinessFailureState
          title="结算单加载失败"
          error={detailQuery.error}
          action={
            <Button type="button" onClick={() => void detailQuery.refetch()}>
              重试
            </Button>
          }
        />
      </PageScaffold>
    )
  }

  if (!data) {
    return (
      <PageScaffold>
        <Button type="button" variant="ghost" size="sm" onClick={onBack}>
          <ArrowLeftIcon className="size-4" />
          返回列表
        </Button>
        <BusinessEmptyState
          kind="no-data"
          className="rounded-lg border-0 bg-transparent p-6 shadow-none ring-0"
          title="结算单不存在"
          description="该结算单不存在或已被作废。可返回列表重新选择，或检查分享链接是否正确。"
        />
      </PageScaffold>
    )
  }

  const detail = data
  const st = detail.statement
  const allowed = new Set(detail.allowedActions)
  const blockers = detail.actionBlockers
  const activeDiff =
    detail.differences.find((d) => d.differenceId === urlState.diff) ??
    detail.differences[0] ??
    null

  const confirmBlocker = blockerOf(blockers, "CONFIRM")
  const submitBlocker = blockerOf(blockers, "SUBMIT_REVIEW")

  async function onRefresh() {
    try {
      const outcome = await refreshMutation.mutateAsync({
        statementId: st.id,
        expectedLockVersion: st.lockVersion,
        expectedSourceSnapshotHash: st.sourceSnapshotHash,
        requestId: newKey("req"),
        idempotencyKey: newKey("refresh"),
      })
      setResult(outcomeToResult(outcome))
    } catch (error) {
      setResult({
        status: "rejected",
        title: "刷新试算未完成",
        description: getErrorMessage(error, "刷新失败，请稍后重试"),
      })
    }
  }

  async function onResolve() {
    if (!activeDiff) return
    try {
      const outcome = await resolveMutation.mutateAsync({
        statementId: st.id,
        differenceId: activeDiff.differenceId,
        expectedLockVersion: st.lockVersion,
        expectedDifferenceVersion: activeDiff.version,
        resolution,
        reasonCode,
        operationId: newKey("op"),
        idempotencyKey: newKey("resolve"),
      })
      setResult(outcomeToResult(outcome))
      if (outcome.status === "succeeded") setResolveOpen(false)
    } catch (error) {
      setResult({
        status: "rejected",
        title: "结论登记未完成",
        description: getErrorMessage(error, "提交失败，请稍后重试"),
      })
    }
  }

  async function onEvidence() {
    if (!activeDiff) return
    try {
      const outcome = await evidenceMutation.mutateAsync({
        statementId: st.id,
        differenceId: activeDiff.differenceId,
        expectedDifferenceVersion: activeDiff.version,
        opinionCode: "PROCUREMENT_NOTE",
        comment: evidenceComment,
        requestId: newKey("req"),
        idempotencyKey: newKey("ev"),
      })
      setResult(outcomeToResult(outcome))
      if (outcome.status === "succeeded") {
        setEvidenceOpen(false)
        setEvidenceComment("")
      }
    } catch (error) {
      setResult({
        status: "rejected",
        title: "证据保存未完成",
        description: getErrorMessage(error, "保存失败，请稍后重试"),
      })
    }
  }

  async function onSubmitReview() {
    const outcome = await submitMutation.mutateAsync({
      statementId: st.id,
      expectedLockVersion: st.lockVersion,
      subjectHash: st.subjectHash ?? `sh_${st.id}`,
      operationId: newKey("op"),
      idempotencyKey: newKey("submit"),
    })
    setResult(outcomeToResult(outcome))
    if (outcome.status === "succeeded") {
      setSubmitOpen(false)
      patchUrl({ section: "review" })
    }
  }

  async function onConfirm() {
    if (!detail.workItem) {
      setResult({
        status: "blocked",
        title: "无复核任务",
        description: "请先领取任务后再确认",
      })
      return
    }
    const outcome = await decisionMutation.mutateAsync({
      statementId: st.id,
      workItemId: detail.workItem.workItemId,
      expectedSubjectVersion: detail.workItem.subjectVersion,
      expectedLockVersion: st.lockVersion,
      action: "CONFIRM",
      operationId: newKey("op"),
      idempotencyKey: newKey("confirm"),
    })
    setResult(outcomeToResult(outcome))
    if (outcome.status === "succeeded") {
      setConfirmOpen(false)
      patchUrl({ section: "payable" })
    }
  }

  async function onReject() {
    if (!detail.workItem) return
    const outcome = await decisionMutation.mutateAsync({
      statementId: st.id,
      workItemId: detail.workItem.workItemId,
      expectedSubjectVersion: detail.workItem.subjectVersion,
      expectedLockVersion: st.lockVersion,
      action: "REJECT",
      operationId: newKey("op"),
      idempotencyKey: newKey("reject"),
      reasonCode: rejectReason || "NEEDS_MORE_EVIDENCE",
    })
    setResult(outcomeToResult(outcome))
    if (outcome.status === "rejected" || outcome.status === "succeeded") {
      setRejectOpen(false)
    }
  }

  return (
    <PageScaffold>
      <PageHeader
        variant="object-chrome"
        breadcrumbs={[
          {
            id: "api",
            label: "供应商 API",
            href: "/supplier-api/settlements",
          },
          {
            id: "list",
            label: "API 供应商结算",
            href: "/supplier-api/settlements",
          },
          {
            id: "detail",
            label: st.statementNo,
            current: true,
          },
        ]}
        actions={
          <Button type="button" variant="outline" size="sm" onClick={onBack}>
            <ArrowLeftIcon className="size-4" />
            返回列表
          </Button>
        }
      />

      {returnTo ? <CrossEntryBanner returnTo={returnTo} /> : null}

      <DocumentHeader
        density="compact"
        title={`${st.supplierName} · ${st.periodLabel}`}
        documentNumber={st.statementNo}
        primaryStatus={{ label: st.statusLabel, tone: st.statusTone }}
        version={st.lockVersion}
        meta={
          <span className="inline-flex flex-wrap items-center gap-x-1.5 gap-y-0.5">
            <span>
              经办{" "}
              <span className="font-medium text-foreground">
                {st.preparedBy?.displayName ?? "—"}
              </span>
            </span>
            <span className="text-border" aria-hidden="true">
              ·
            </span>
            <span>
              复核{" "}
              <span className="font-medium text-foreground">
                {st.reviewedBy?.displayName ?? "待复核人"}
              </span>
            </span>
            <span className="text-border" aria-hidden="true">
              ·
            </span>
            <span className="text-muted-foreground">
              记录 {formatDateTime(detail.freshness.immutableFactsAsOf, "default")}
            </span>
          </span>
        }
        primaryAction={
          <div className="flex flex-wrap gap-2">
            {allowed.has("REFRESH_TRIAL") ? (
              <Button
                type="button"
                variant="outline"
                size="sm"
                disabled={refreshMutation.isPending}
                onClick={() => void onRefresh()}
              >
                <RefreshCwIcon className="size-3.5" />
                刷新试算
              </Button>
            ) : null}
            {allowed.has("SUBMIT_REVIEW") ? (
              <Button
                type="button"
                size="sm"
                onClick={() => setSubmitOpen(true)}
              >
                <SendIcon className="size-3.5" />
                提交复核
              </Button>
            ) : submitBlocker ? (
              <GuardedBusinessAction
                type="button"
                size="sm"
                disabled
                reason={submitBlocker.message}
              >
                提交复核
              </GuardedBusinessAction>
            ) : null}
            {allowed.has("CONFIRM") ? (
              <Button
                type="button"
                size="sm"
                onClick={() => setConfirmOpen(true)}
              >
                <CheckIcon className="size-3.5" />
                确认结算
              </Button>
            ) : confirmBlocker ? (
              <GuardedBusinessAction
                type="button"
                size="sm"
                disabled
                reason={confirmBlocker.message}
              >
                确认结算
              </GuardedBusinessAction>
            ) : null}
            {allowed.has("REJECT") ? (
              <Button
                type="button"
                size="sm"
                variant="outline"
                onClick={() => setRejectOpen(true)}
              >
                驳回
              </Button>
            ) : null}
          </div>
        }
      />

      {blockers.filter((b) =>
        ["CONFIRM", "SUBMIT_REVIEW", "SOD_VIOLATION"].includes(b.action) ||
        b.code === "SOD_VIOLATION" ||
        b.code === "BLOCKING_DIFFERENCES"
      ).length > 0 ? (
        <Alert variant="warning">
          <AlertTitle>动作门禁</AlertTitle>
          <AlertDescription>
            <ul className="list-inside list-disc text-sm">
              {blockers
                .filter(
                  (b) =>
                    b.action === "CONFIRM" ||
                    b.action === "SUBMIT_REVIEW" ||
                    b.code === "SOD_VIOLATION" ||
                    b.code === "BLOCKING_DIFFERENCES"
                )
                .map((b) => (
                  <li key={`${b.action}-${b.code}`}>{b.message}</li>
                ))}
            </ul>
          </AlertDescription>
        </Alert>
      ) : null}

      <div ref={resultRef} tabIndex={-1} className="outline-none">
        {result ? (
          <FormalActionResult
            status={
              result.status === "failed" ? "blocked" : result.status
            }
            title={result.title}
            description={result.description}
            reference={result.reference}
            facts={result.facts}
            actions={
              <div className="flex flex-wrap gap-2">
                {result.w12Href ? (
                  <Button
                    type="button"
                    size="sm"
                    render={<Link href={result.w12Href} />}
                  >
                    去供应商往来 处理应付
                    <ExternalLinkIcon className="size-3.5" />
                  </Button>
                ) : null}
              </div>
            }
          />
        ) : null}
      </div>

      <Card size="sm" className={surfacePanelClassName}>
        <CardHeader className="rounded-t-lg border-b border-border/30 py-3">
          <CardTitle className="text-base">金额摘要</CardTitle>
          <CardDescription>
            订单、运费、服务费、退款与 ERP 计算金额、供应商账单金额、差异方向对比 · 全部
            {detail.totals.taxBasisLabel}
          </CardDescription>
        </CardHeader>
        <CardContent className="pt-4">
          <DocumentTotals
            title={null}
            items={[
              {
                id: "order",
                label: "订单结算价",
                value: (
                  <MoneyValue
                    value={detail.totals.orderAmountGross}
                    taxBasis="gross"
                  />
                ),
                basis: "含税",
              },
              {
                id: "freight",
                label: "运费",
                value: (
                  <MoneyValue
                    value={detail.totals.freightGross}
                    taxBasis="gross"
                  />
                ),
                basis: "含税",
              },
              {
                id: "service",
                label: "服务费",
                value: (
                  <MoneyValue
                    value={detail.totals.serviceFeeGross}
                    taxBasis="gross"
                  />
                ),
                basis: "含税",
              },
              {
                id: "refund",
                label: "供应商退款",
                value: (
                  <MoneyValue
                    value={detail.totals.refundGross}
                    taxBasis="gross"
                  />
                ),
                basis: "含税",
              },
              {
                id: "erp",
                label: "ERP 计算金额",
                value: (
                  <MoneyValue
                    value={detail.totals.erpAmountGross}
                    taxBasis="gross"
                  />
                ),
                basis: "含税",
              },
              {
                id: "supplier",
                label: "供应商账单金额",
                value: detail.totals.supplierAmountGross ? (
                  <MoneyValue
                    value={detail.totals.supplierAmountGross}
                    taxBasis="gross"
                  />
                ) : (
                  "账单未同步 · 刷新试算后以 ERP 金额预填"
                ),
                basis: "含税",
              },
              {
                id: "diff",
                label: "差异金额",
                value: detail.totals.differenceAmountGross ? (
                  <MoneyValue
                    value={detail.totals.differenceAmountGross}
                    taxBasis="gross"
                  />
                ) : (
                  "—"
                ),
                warning: detail.totals.differenceDirectionLabel,
                basis: "含税",
              },
              {
                id: "cost",
                label:
                  st.status === "CONFIRMED"
                    ? "已确认成本差额"
                    : "待确认成本差额预览",
                value: (
                  <MoneyValue
                    value={
                      detail.totals.confirmedCostDeltaGross ??
                      detail.totals.pendingCostDeltaGross ??
                      "0.00"
                    }
                    taxBasis="gross"
                  />
                ),
                basis: "含税",
              },
            ]}
          />
        </CardContent>
      </Card>

      <div
        className={cn(
          surfaceInsetClassName,
          "px-3 py-2 text-xs text-muted-foreground"
        )}
      >
        <span className="font-medium text-foreground">来源数据 </span>
        更新时间 {formatDateTime(st.sourceAsOf, "default")}
        {st.externalBillNo ? (
          <>
            {" "}
            · 账单 {st.externalBillNo}（第{" "}
            {String(st.externalBillVersion ?? "").replace(/^v/i, "")} 版）
          </>
        ) : null}
        <span className="ml-2">以下数据仅供参考，不进入结算结果</span>
      </div>

      <div className={cn(surfacePanelClassName, "min-w-0 overflow-hidden")}>
      <Tabs
        value={section}
        onValueChange={(v) =>
          patchUrl({ section: v as SettlementSection })
        }
      >
        <TabsList
          variant="line"
          className="sticky top-0 z-10 h-auto w-full flex-wrap justify-start gap-1 overflow-x-auto rounded-none border-b border-border/30 bg-card/95 px-3 py-1.5 backdrop-blur supports-backdrop-filter:bg-card/80"
        >
          {SECTIONS.map((s) => (
            <TabsTrigger key={s} value={s}>
              {SECTION_LABEL[s]}
              {s === "differences" && detail.differenceSummary.blocking > 0
                ? ` (${detail.differenceSummary.blocking})`
                : null}
            </TabsTrigger>
          ))}
        </TabsList>
      </Tabs>
      <div className="space-y-4 p-3 md:p-4">
      <p className="text-xs text-muted-foreground">
        快捷键 d 可直达差异处理
      </p>

      {section === "overview" ? (
        <Card size="sm" className={cn(surfaceInsetClassName, "shadow-none ring-0")}>
          <CardHeader className="rounded-t-lg border-b border-border/30 py-3">
            <CardTitle className="text-base">概览</CardTitle>
          </CardHeader>
          <CardContent className="space-y-2 pt-4 text-sm">
            <p>
              供应商：{st.supplierName}（记录时，不受后续更名影响）
            </p>
            <p className="num">
              期间：{st.periodStart} ~ {st.periodEnd}
            </p>
            <p>状态：{st.statusLabel}</p>
            <p>
              未决阻断差异：{detail.differenceSummary.blocking} / 差异合计{" "}
              {detail.differenceSummary.total}
            </p>
            <p className="text-muted-foreground">
              账单/订单/成本原值只读，不可在本页改写以消差。
            </p>
            <div className="flex flex-wrap gap-2 pt-2">
              <Button
                type="button"
                size="sm"
                variant="secondary"
                onClick={() => patchUrl({ section: "differences" })}
              >
                打开差异处理
              </Button>
              <Button
                type="button"
                size="sm"
                variant="outline"
                onClick={() => patchUrl({ section: "items" })}
              >
                查看结算明细
              </Button>
            </div>
          </CardContent>
        </Card>
      ) : null}

      {section === "items" ? (
        <Card size="sm" className={cn(surfaceInsetClassName, "shadow-none ring-0")}>
          <CardHeader className="rounded-t-lg border-b border-border/30 py-3">
            <CardTitle className="text-base">结算明细</CardTitle>
            <CardDescription>
              冻结数据 + 不可变完成/取消/退款记录 · 金额只读，不可修改
            </CardDescription>
          </CardHeader>
          <CardContent className="overflow-x-auto pt-0">
            <table className="w-full min-w-[48rem] text-left text-sm">
              <thead className="border-b text-xs text-muted-foreground">
                <tr>
                  <th className="px-2 py-2">供应商订单</th>
                  <th className="px-2 py-2">采购单号</th>
                  <th className="px-2 py-2">外部单号</th>
                  <th className="px-2 py-2">商品</th>
                  <th className="px-2 py-2 text-right">数量</th>
                  <th className="px-2 py-2">记录</th>
                  <th className="px-2 py-2 text-right">订单</th>
                  <th className="px-2 py-2 text-right">运费</th>
                  <th className="px-2 py-2 text-right">服务费</th>
                  <th className="px-2 py-2 text-right">退款</th>
                  <th className="px-2 py-2 text-right">ERP</th>
                  <th className="px-2 py-2 text-right">账单行</th>
                </tr>
              </thead>
              <tbody>
                {detail.items.map((it) => (
                  <tr key={it.itemId} className="border-b border-border/60">
                    <td className="px-2 py-2">
                      <Link
                        href={`/supplier-api/orders?q=${encodeURIComponent(it.supplierOrderNo)}`}
                        className="num font-medium text-primary underline-offset-2 hover:underline"
                      >
                        {it.supplierOrderNo}
                      </Link>
                    </td>
                    <td className="px-2 py-2">
                      {it.purchaseNo ? (
                        <Link
                          href={
                            it.purchaseOrderId
                              ? `/procurement/orders/${it.purchaseOrderId}?returnTo=${encodeURIComponent(`/supplier-api/settlements/${statementId}`)}`
                              : `/procurement/orders?q=${encodeURIComponent(it.purchaseNo)}`
                          }
                          className="num font-medium text-primary underline-offset-2 hover:underline"
                        >
                          {it.purchaseNo}
                        </Link>
                      ) : (
                        <span className="text-xs text-muted-foreground">—</span>
                      )}
                    </td>
                    <td className="num px-2 py-2 text-muted-foreground">
                      {it.externalOrderNo}
                    </td>
                    <td className="px-2 py-2">{it.productName}</td>
                    <td className="num px-2 py-2 text-right">{it.quantity}</td>
                    <td className="px-2 py-2 text-xs">{it.factLabel}</td>
                    <td className="px-2 py-2 text-right">
                      <MoneyValue value={it.orderAmountGross} taxBasis="gross" />
                    </td>
                    <td className="px-2 py-2 text-right">
                      <MoneyValue value={it.freightGross} taxBasis="gross" />
                    </td>
                    <td className="px-2 py-2 text-right">
                      <MoneyValue
                        value={it.serviceFeeGross}
                        taxBasis="gross"
                      />
                    </td>
                    <td className="px-2 py-2 text-right">
                      <MoneyValue value={it.refundGross} taxBasis="gross" />
                    </td>
                    <td className="px-2 py-2 text-right">
                      <MoneyValue value={it.erpAmountGross} taxBasis="gross" />
                    </td>
                    <td className="px-2 py-2 text-right">
                      {it.supplierBillLineGross != null ? (
                        <MoneyValue
                          value={it.supplierBillLineGross}
                          taxBasis="gross"
                        />
                      ) : (
                        "—"
                      )}
                    </td>
                  </tr>
                ))}
                {detail.items.length === 0 ? (
                  <tr>
                    <td
                      colSpan={12}
                      className="px-2 py-6 text-center text-muted-foreground"
                    >
                      暂无明细；可在草稿态刷新试算纳入不可变记录
                    </td>
                  </tr>
                ) : null}
              </tbody>
            </table>
            <p className="mt-2 text-xs text-muted-foreground">
              输入控件未提供金额编辑路径；账单原值与订单记录不可覆盖。
            </p>
          </CardContent>
        </Card>
      ) : null}

      {section === "differences" ? (
        <DifferencesWorkspace
          differences={detail.differences}
          activeDiff={activeDiff}
          onSelect={(id) => patchUrl({ diff: id })}
          allowed={allowed}
          onResolve={() => setResolveOpen(true)}
          onEvidence={() => setEvidenceOpen(true)}
        />
      ) : null}

      {section === "review" ? (
        <Card size="sm" className={cn(surfaceInsetClassName, "shadow-none ring-0")}>
          <CardHeader className="rounded-t-lg border-b border-border/30 py-3">
            <CardTitle className="text-base">复核记录</CardTitle>
            <CardDescription>
              提交 / 驳回 / 确认追加式记录；岗位分离由系统校验
            </CardDescription>
          </CardHeader>
          <CardContent className="space-y-3 pt-4">
            {detail.workItem ? (
              <Alert variant="info">
                <AlertTitle>复核任务</AlertTitle>
                <AlertDescription>
                  {detail.statement.statementNo} · 供应商{" "}
                  {detail.statement.supplierName}
                  {detail.workItem.claimedBy
                    ? ` · 领取人 ${detail.workItem.claimedBy.displayName}`
                    : " · 待领取"}
                </AlertDescription>
                {detail.workItem.claimedBy == null &&
                allowed.has("CLAIM_REVIEW") ? (
                  <Button
                    type="button"
                    size="sm"
                    variant="outline"
                    disabled={claimMutation.isPending}
                    onClick={async () => {
                      try {
                        const outcome = await claimMutation.mutateAsync({
                          statementId: st.id,
                          workItemId: detail.workItem!.workItemId,
                          expectedSubjectVersion:
                            detail.workItem!.subjectVersion,
                          idempotencyKey: newKey("claim"),
                        })
                        setResult(outcomeToResult(outcome))
                      } catch (error) {
                        setResult({
                          status: "rejected",
                          title: "领取任务未完成",
                          description: getErrorMessage(
                            error,
                            "领取失败，请稍后重试",
                          ),
                        })
                      }
                    }}
                  >
                    领取任务
                  </Button>
                ) : null}
              </Alert>
            ) : null}
            {detail.reviewRecords.length === 0 ? (
              <p className="text-sm text-muted-foreground">尚无复核记录</p>
            ) : (
              detail.reviewRecords.map((r) => (
                <div
                  key={r.recordId}
                  className={cn(surfaceInsetClassName, "px-3 py-2 text-sm")}
                >
                  <div className="font-medium">
                    {r.actionLabel} · {r.by.displayName}
                  </div>
                  <div className="text-xs text-muted-foreground">
                    {formatDateTime(r.at, "default")}
                    {r.reasonCode
                      ? ` · ${REASON_CODE_LABEL[r.reasonCode] ?? r.reasonCode}`
                      : ""}
                    {r.comment ? ` · ${r.comment}` : ""}
                  </div>
                </div>
              ))
            )}
          </CardContent>
        </Card>
      ) : null}

      {section === "payable" ? (
        <Card size="sm" className={cn(surfaceInsetClassName, "shadow-none ring-0")}>
          <CardHeader className="rounded-t-lg border-b border-border/30 py-3">
            <CardTitle className="text-base">应付与票款</CardTitle>
            <CardDescription>
              确认后形成唯一应付；付款/进项发票/核销进入供应商往来，不在本页复制
            </CardDescription>
          </CardHeader>
          <CardContent className="space-y-3 pt-4">
            {detail.payable ? (
              <>
                <p className="text-sm">
                  应付编号{" "}
                  <span className="num font-medium">
                    {detail.payable.payableNo}
                  </span>
                </p>
                <p className="text-sm">
                  含税金额{" "}
                  <MoneyValue
                    value={detail.payable.grossAmount}
                    taxBasis="gross"
                  />{" "}
                  · 到期 {detail.payable.dueDate} · {detail.payable.statusLabel}
                </p>
                <Button
                  type="button"
                  size="sm"
                  render={<Link href={detail.payable.w12Href} />}
                >
                  {openWorkspaceLabel("W12")}
                  <ExternalLinkIcon className="size-3.5" />
                </Button>
              </>
            ) : (
              <p className="text-sm text-muted-foreground">
                尚未确认结算，无应付编号。确认成功后此处展示应付与成本差额结果。
              </p>
            )}
          </CardContent>
        </Card>
      ) : null}

      {section === "audit" ? (
        <Card size="sm" className={cn(surfaceInsetClassName, "shadow-none ring-0")}>
          <CardHeader className="rounded-t-lg border-b border-border/30 py-3">
            <CardTitle className="text-base">审计</CardTitle>
          </CardHeader>
          <CardContent className="space-y-2 pt-4">
            {detail.auditEvents.map((e) => (
              <div
                key={e.eventId}
                className={cn(surfaceInsetClassName, "px-3 py-2 text-sm")}
              >
                <div className="flex flex-wrap gap-2">
                  <span className="font-medium">
                    {AUDIT_ACTION_LABEL[e.action] ?? e.summary.split("·")[0]}
                  </span>
                  <span className="text-muted-foreground">{e.actor}</span>
                  {e.auditNo ? (
                    <span className="num text-xs">审计号 {e.auditNo}</span>
                  ) : null}
                </div>
                <p className="text-muted-foreground">{e.summary}</p>
                <p className="text-xs text-muted-foreground">
                  {formatDateTime(e.at, "default")}
                </p>
              </div>
            ))}
          </CardContent>
        </Card>
      ) : null}
      </div>
      </div>

      {/* Resolve difference */}
      <Dialog open={resolveOpen} onOpenChange={setResolveOpen}>
        <DialogContent>
          <DialogHeader>
            <DialogTitle>登记差异处理结论</DialogTitle>
            <DialogDescription>
              财务经办追加式结论；不修改左右证据原值或历史成本。结论一经登记不可撤回，将写入审计并改变待确认成本差额。
            </DialogDescription>
          </DialogHeader>
          <div className="space-y-3">
            <div className="space-y-1.5">
              <Label>受控结论</Label>
              <OptionCombobox
                value={resolution}
                onValueChange={(v) => {
                  if (v) setResolution(v as DifferenceResolution)
                }}
                options={(
                  Object.keys(RESOLUTION_LABEL) as DifferenceResolution[]
                ).map((k) => ({
                  value: k,
                  label: RESOLUTION_LABEL[k],
                }))}
                allowClear={false}
              />
            </div>
            <div className="space-y-1.5">
              <Label>原因码</Label>
              <OptionCombobox
                value={reasonCode}
                onValueChange={(v) => {
                  if (v) setReasonCode(v)
                }}
                options={[
                  { value: "BILL_ALIGNED", label: "账单已对齐" },
                  { value: "ACCEPT_BILL", label: "接受供应商账单" },
                  { value: "NO_BUSINESS_IMPACT", label: "无需业务调整" },
                  { value: "COMPENSATED_ELSEWHERE", label: "已另行补偿" },
                ]}
                allowClear={false}
              />
            </div>
          </div>
          <DialogFooter>
            <Button
              type="button"
              variant="outline"
              onClick={() => setResolveOpen(false)}
            >
              取消
            </Button>
            <Button
              type="button"
              disabled={resolveMutation.isPending}
              onClick={() => void onResolve()}
            >
              提交结论
            </Button>
          </DialogFooter>
        </DialogContent>
      </Dialog>

      <Dialog open={evidenceOpen} onOpenChange={setEvidenceOpen}>
        <DialogContent>
          <DialogHeader>
            <DialogTitle>追加采购协同证据</DialogTitle>
            <DialogDescription>
              只追加供应商证据或业务意见和审计，不改变差异结论、试算金额或成本基线。
            </DialogDescription>
          </DialogHeader>
          <div className="space-y-1.5">
            <Label htmlFor="ev-comment">业务说明</Label>
            <Textarea
              id="ev-comment"
              value={evidenceComment}
              onChange={(e) => setEvidenceComment(e.target.value)}
              rows={3}
            />
          </div>
          <DialogFooter>
            <Button
              type="button"
              variant="outline"
              onClick={() => setEvidenceOpen(false)}
            >
              取消
            </Button>
            <Button
              type="button"
              disabled={evidenceMutation.isPending}
              onClick={() => void onEvidence()}
            >
              保存证据
            </Button>
          </DialogFooter>
        </DialogContent>
      </Dialog>

      <FormalActionConfirmDialog
        open={submitOpen}
        onOpenChange={setSubmitOpen}
        title="提交复核"
        description="将冻结来源更新时间、明细与差异结论，并创建唯一复核待办。"
        actionLabel="提交复核"
        confirmLabel="确认提交"
        fromStatus={{ label: st.statusLabel, tone: st.statusTone }}
        toStatus={{ label: "待复核", tone: "warning" }}
        lockedFields={[
          st.statementNo,
          "来源数据、明细与差异结论已锁定",
        ]}
        effects={["冻结来源数据与差异结论", "创建结算复核待办"]}
        pending={submitMutation.isPending}
        onConfirm={async () => {
          await onSubmitReview()
        }}
      />

      <FormalActionConfirmDialog
        open={confirmOpen}
        onOpenChange={setConfirmOpen}
        title="确认结算（不可逆）"
        description="同一次提交追加成本差额、形成唯一应付并锁定处理结果。经办人不可确认本单。"
        actionLabel="确认结算"
        confirmLabel="确认结算"
        fromStatus={{ label: st.statusLabel, tone: st.statusTone }}
        toStatus={{ label: "已确认", tone: "success" }}
        lockedFields={[
          st.statementNo,
          `应付金额预览 ${st.supplierAmountGross ?? st.erpAmountGross}`,
          `成本差额预览 ${detail.totals.pendingCostDeltaGross ?? "0.00"}`,
          `经办 ${st.preparedBy?.displayName ?? "—"}`,
        ]}
        effects={[
          "追加成本差额记录",
          "形成唯一供应商结算应付",
          "锁定处理结果，不可撤回确认",
        ]}
        irreversibleEffects={["确认后付款/进项发票/核销进入供应商往来"]}
        nextDepartment="供应商往来"
        pending={decisionMutation.isPending}
        onConfirm={async () => {
          await onConfirm()
        }}
      />

      <Dialog open={rejectOpen} onOpenChange={setRejectOpen}>
        <DialogContent>
          <DialogHeader>
            <DialogTitle>驳回复核</DialogTitle>
            <DialogDescription>原因必填，退回经办并保留记录。</DialogDescription>
          </DialogHeader>
          <div className="space-y-1.5">
            <Label>原因码</Label>
            <OptionCombobox
              value={rejectReason || null}
              onValueChange={(v) => setRejectReason(v ?? "")}
              options={[
                { value: "", label: "请选择" },
                { value: "NEEDS_MORE_EVIDENCE", label: "证据不足" },
                { value: "AMOUNT_MISMATCH", label: "金额仍不一致" },
                { value: "OTHER", label: "其他" },
              ]}
              placeholder="请选择"
              allowClear={false}
            />
          </div>
          <DialogFooter>
            <Button
              type="button"
              variant="ghost"
              onClick={() => setRejectOpen(false)}
            >
              取消
            </Button>
            <Button
              type="button"
              disabled={!rejectReason || decisionMutation.isPending}
              onClick={() => void onReject()}
            >
              确认驳回
            </Button>
          </DialogFooter>
        </DialogContent>
      </Dialog>
    </PageScaffold>
  )
}

function DifferencesWorkspace({
  differences,
  activeDiff,
  onSelect,
  allowed,
  onResolve,
  onEvidence,
}: {
  differences: SettlementDifferenceView[]
  activeDiff: SettlementDifferenceView | null
  onSelect: (id: string) => void
  allowed: Set<string>
  onResolve: () => void
  onEvidence: () => void
}) {
  if (differences.length === 0) {
    return (
      <BusinessEmptyState
        kind="no-data"
        className="rounded-lg border-0 bg-transparent p-6 shadow-none ring-0"
        title="无差异"
        description="当前结算单没有差异记录，明细金额核对一致时可直接进入复核。"
      />
    )
  }

  return (
    <div className="grid gap-4 xl:grid-cols-[16rem_minmax(0,1fr)]">
      <Card size="sm" className={cn(surfaceInsetClassName, "shadow-none ring-0")}>
        <CardHeader className="rounded-t-lg border-b border-border/30 py-3">
          <CardTitle className="text-base">差异列表</CardTitle>
        </CardHeader>
        <CardContent className="space-y-1 p-2">
          {differences.map((d) => (
            <button
              key={d.differenceId}
              type="button"
              className={cn(
                "flex w-full flex-col rounded-md px-2 py-2 text-left text-sm hover:bg-foreground/5",
                activeDiff?.differenceId === d.differenceId
                  ? "bg-card font-medium shadow-sm ring-1 ring-foreground/10"
                  : "text-muted-foreground"
              )}
              onClick={() => onSelect(d.differenceId)}
            >
              <span className="font-medium">{d.typeLabel}</span>
              <span className="text-xs text-muted-foreground">
                {d.statusLabel}
                {d.requiresProcurementEvidence ? " · 待举证" : ""}
                {d.blocking ? " · 阻断" : ""}
              </span>
              {d.amountGross ? (
                <span className="mt-0.5">
                  <MoneyValue
                    value={d.amountGross}
                    className="num text-xs font-semibold text-warning-soft-foreground"
                  />
                </span>
              ) : null}
            </button>
          ))}
        </CardContent>
      </Card>

      {activeDiff ? (
        <div className="space-y-4">
          <Card size="sm" className={cn(surfaceInsetClassName, "shadow-none ring-0")}>
            <CardHeader className="rounded-t-lg border-b border-border/30 py-3">
              <div className="flex flex-wrap items-center justify-between gap-2">
                <div>
                  <CardTitle className="text-base">
                    {activeDiff.typeLabel}
                  </CardTitle>
                  <CardDescription>
                    {activeDiff.amountDirectionLabel}
                    {activeDiff.amountGross ? (
                      <span className="mt-1 block text-base font-semibold text-warning-soft-foreground">
                        <MoneyValue
                          value={activeDiff.amountGross}
                          taxBasis="gross"
                        />
                      </span>
                    ) : null}
                  </CardDescription>
                </div>
                <BusinessStatusBadge
                  context="list"
                  label={activeDiff.statusLabel}
                  tone={activeDiff.statusTone}
                />
                {activeDiff.requiresProcurementEvidence ? (
                  <Badge variant="outline">需采购举证</Badge>
                ) : null}
                {activeDiff.blocking ? (
                  <Badge variant="destructive">阻塞</Badge>
                ) : null}
              </div>
            </CardHeader>
            <CardContent className="space-y-3 pt-4">
              <div className="grid gap-2 sm:grid-cols-2 text-sm">
                <div className={cn(surfaceInsetClassName, "p-3")}>
                  <div className="text-xs text-muted-foreground">ERP 侧</div>
                  <div>{activeDiff.erpSideLabel}</div>
                  {activeDiff.erpSideAmount ? (
                    <MoneyValue
                      value={activeDiff.erpSideAmount}
                      taxBasis="gross"
                    />
                  ) : null}
                </div>
                <div className={cn(surfaceInsetClassName, "p-3")}>
                  <div className="text-xs text-muted-foreground">供应商侧</div>
                  <div>{activeDiff.supplierSideLabel}</div>
                  {activeDiff.supplierSideAmount ? (
                    <MoneyValue
                      value={activeDiff.supplierSideAmount}
                      taxBasis="gross"
                    />
                  ) : null}
                </div>
              </div>

              <BusinessDiffPanel
                title="左右证据对比"
                caption="字段级证据；原值只读不可为消差改写"
                changes={activeDiff.leftFields.map((c) => ({
                  id: c.id,
                  field: c.field,
                  before: c.before,
                  after: c.after,
                  note: c.note,
                }))}
              />

              <div>
                <h4 className="mb-2 text-sm font-medium">已登记证据</h4>
                {activeDiff.evidence.length === 0 ? (
                  <p className="text-sm text-muted-foreground">
                    尚无采购/供应商证据
                    {activeDiff.requiresProcurementEvidence
                      ? "（本差异需要采购协同）"
                      : ""}
                  </p>
                ) : (
                  <ul className="space-y-2">
                    {activeDiff.evidence.map((e) => (
                      <li
                        key={e.evidenceId}
                        className={cn(surfaceInsetClassName, "px-3 py-2 text-sm")}
                      >
                        <div className="font-medium">
                          {e.label} · {e.by.displayName}
                        </div>
                        <div className="text-muted-foreground">
                          {e.comment}
                        </div>
                        <div className="text-xs text-muted-foreground">
                  {formatDateTime(e.at, "default")}
                        </div>
                      </li>
                    ))}
                  </ul>
                )}
              </div>

              {activeDiff.resolution ? (
                <Alert variant="success">
                  <AlertTitle>
                    结论：{activeDiff.resolution.resolutionLabel}
                  </AlertTitle>
                  <AlertDescription>
                    {activeDiff.resolution.by.displayName} ·{" "}
                    {formatDateTime(activeDiff.resolution.at, "default")} · 成本预览{" "}
                    <MoneyValue
                      value={activeDiff.resolution.costImpactPreview ?? "0.00"}
                      taxBasis="gross"
                    />
                  </AlertDescription>
                </Alert>
              ) : null}

              <Separator />
              <div className="flex flex-wrap gap-2">
                {allowed.has("APPEND_EVIDENCE") ? (
                  <Button type="button" size="sm" onClick={onEvidence}>
                    追加采购证据
                  </Button>
                ) : null}
                {allowed.has("RESOLVE_DIFFERENCE") &&
                activeDiff.status === "PENDING" ? (
                  <Button type="button" size="sm" onClick={onResolve}>
                    登记结论
                  </Button>
                ) : null}
                {!allowed.has("RESOLVE_DIFFERENCE") ? (
                  <span className="text-xs text-muted-foreground">
                    当前不可登记差异结论
                  </span>
                ) : null}
              </div>
            </CardContent>
          </Card>
        </div>
      ) : null}
    </div>
  )
}
