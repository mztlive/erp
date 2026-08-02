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
  ListToolbar,
  MetricFilterItem,
  MetricStrip,
  MoneyValue,
  OptionCombobox,
  PageActions,
  PageHeader,
  QuickPreviewSheet,
  SupplierCombobox,
} from "@/components/business"
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
  useCreateDraftMutation,
  useQueryFormalIdempotencyMutation,
  useRefreshTrialMutation,
  useResolveDifferenceMutation,
  useReviewDecisionMutation,
  useSettlementDetailQuery,
  useSettlementListQuery,
  useSubmitReviewMutation,
} from "@/features/supplier-settlements/queries"
import type {
  DemoRole,
  DifferenceResolution,
  FormalOutcome,
  SettlementDifferenceView,
  SettlementListRow,
  SettlementSection,
} from "@/features/supplier-settlements/types"
import {
  DEMO_ROLE_LABEL,
  DIFF_TYPE_LABEL,
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

type ResultState = {
  status:
    | "succeeded"
    | "failed"
    | "blocked"
    | "rejected"
    | "unknown"
    | "processing"
  title: string
  description: string
  reference?: string
  facts?: Array<{ label: string; value: React.ReactNode }>
  pendingIdempotencyKey?: string
  payableNo?: string
  w12Href?: string
} | null

function formatTime(iso?: string) {
  if (!iso) return "—"
  try {
    return new Date(iso).toLocaleString("zh-CN", { hour12: false })
  } catch {
    return iso
  }
}

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
      reference: outcome.operationId,
      pendingIdempotencyKey: outcome.idempotencyKey,
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
        if (next.role !== "finance_prep") params.set("role", next.role)
        if (next.demoFlag) params.set("demoFlag", next.demoFlag)
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
      onOpen={(id) =>
        patchUrl({ statementId: id, section: "overview", preview: undefined })
      }
    />
  )
}

function RoleDemoBar({
  role,
  demoFlag,
  onRole,
  onFlag,
}: {
  role: DemoRole
  demoFlag?: SettlementsUrlState["demoFlag"]
  onRole: (r: DemoRole) => void
  onFlag: (f?: SettlementsUrlState["demoFlag"]) => void
}) {
  return (
    <div className="flex flex-wrap items-center gap-2 rounded-xl border bg-muted/40 px-3 py-2 text-sm">
      <span className="text-muted-foreground">角色演示</span>
      <OptionCombobox
        value={role}
        onValueChange={(v) => {
          if (v == null) return
          onRole(v as DemoRole)
        }}
        options={[
          { value: "finance_prep", label: "财务经办" },
          { value: "finance_review", label: "财务复核" },
          { value: "procurement", label: "采购" },
          { value: "manager", label: "管理层只读" },
        ]}
        className="w-[10rem]"
        size="sm"
        allowClear={false}
      />
      <OptionCombobox
        value={demoFlag ?? "normal"}
        onValueChange={(v) => {
          if (v == null || v === "normal") onFlag(undefined)
          else onFlag(v as SettlementsUrlState["demoFlag"])
        }}
        options={[
          { value: "normal", label: "正常权限" },
          { value: "no-permission", label: "无模块权限" },
          { value: "no-scope", label: "无数据范围" },
          { value: "policy-missing", label: "期间策略缺失" },
        ]}
        className="w-[12rem]"
        size="sm"
        allowClear={false}
      />
      <span className="text-xs text-muted-foreground">
        当前：{DEMO_ROLE_LABEL[role]}
        {role === "procurement"
          ? " · 仅证据，不可结论/确认"
          : role === "finance_prep"
            ? " · 经办结论与提交复核，不可自审"
            : role === "finance_review"
              ? " · 另一人确认/驳回"
              : " · 只读进度"}
      </span>
    </div>
  )
}

function SettlementList({
  urlState,
  patchUrl,
  onOpen,
}: {
  urlState: SettlementsUrlState
  patchUrl: (patch: Partial<SettlementsUrlState>) => void
  onOpen: (statementId: string) => void
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
    role: urlState.role,
    demoFlag: urlState.demoFlag,
  })

  const data = listQuery.data
  const [pagination, setPagination] = React.useState<PaginationState>({
    pageIndex: Math.max(0, urlState.page - 1),
    pageSize: 50,
  })

  React.useEffect(() => {
    setPagination((p) => ({
      ...p,
      pageIndex: Math.max(0, urlState.page - 1),
    }))
  }, [urlState.page])

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
          <div className="min-w-[9rem]">
            <div className="num text-sm font-medium">
              {row.original.statementNo}
            </div>
            <div className="text-[11px] text-muted-foreground">
              {row.original.periodLabel}
            </div>
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
              <div className="text-[11px] text-muted-foreground">
                {row.original.differenceDirectionLabel}
              </div>
            ) : null}
            {row.original.unresolvedDifferenceCount > 0 ? (
              <Badge variant="outline" className="mt-0.5 text-[10px]">
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

  const policy = data?.periodPolicy
  const canCreate =
    urlState.role === "finance_prep" &&
    policy?.state === "CONFIGURED" &&
    data?.hasModulePermission &&
    data?.hasDataScope

  const createSchema = z.object({
    supplierId: z.string().min(1, "请选择供应商"),
    periodKey: z.string().min(1, "请选择策略返回的完整期间"),
  })

  const form = useAppForm({
    defaultValues: {
      supplierId: "",
      periodKey: "",
    },
    validators: { onChange: createSchema },
    onSubmit: async ({ value }) => {
      if (!policy || policy.state !== "CONFIGURED") {
        setResult({
          status: "blocked",
          title: "期间策略未配置",
          description:
            "PERIOD_POLICY_UNCONFIGURED：不得新建草稿。请先完成供应商结算期间策略配置。",
        })
        return
      }
      const period = policy.selectablePeriods.find(
        (p) => `${p.periodStart}|${p.periodEnd}` === value.periodKey
      )
      if (!period) {
        setResult({
          status: "blocked",
          title: "期间无效",
          description: "必须选择策略返回的完整周期",
        })
        return
      }
      const outcome = await createMutation.mutateAsync({
        supplierId: value.supplierId,
        periodStart: period.periodStart,
        periodEnd: period.periodEnd,
        periodPolicyId: policy.policyId,
        expectedPeriodPolicyVersion: policy.policyVersion,
        role: urlState.role,
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
      <div className="mx-auto flex w-full max-w-shell flex-col gap-4 p-4 md:p-5">
        <div className="h-10 w-56 animate-pulse rounded-lg bg-muted" />
        <div className="h-16 animate-pulse rounded-xl bg-muted" />
        <div className="h-72 animate-pulse rounded-2xl bg-muted" />
      </div>
    )
  }

  if (listQuery.isError) {
    return (
      <div className="mx-auto flex w-full max-w-shell flex-col gap-4 p-4 md:p-5">
        <PageHeader title="API 供应商结算" description="加载失败" />
        <BusinessFailureState
          kind="system"
          title="结算列表加载失败"
          description="请重试。有缓存时应保留旧列表。"
          action={
            <Button type="button" onClick={() => void listQuery.refetch()}>
              重试
            </Button>
          }
        />
      </div>
    )
  }

  const empty = data?.emptyReason

  return (
    <div className="mx-auto flex w-full max-w-shell flex-col gap-4 p-4 md:p-5">
      <PageHeader
        title="API 供应商结算"
        metadata={
          <DataFreshness
            updatedAt={data?.sourceAsOf ? formatTime(data.sourceAsOf) : "—"}
            dateTime={data?.sourceAsOf}
            label="结算数据更新时间"
            state={listQuery.isFetching ? "stale" : "fresh"}
          />
        }
        actions={
          <PageActions
            actions={[
              {
                actionKey: "refresh",
                label: "刷新",
                icon: RefreshCwIcon,
                variant: "outline",
                onClick: () => void listQuery.refetch(),
              },
              {
                actionKey: "create",
                label: "新建结算草稿",
                icon: PlusIcon,
                mobileVisibility: "hide",
                disabled: !canCreate,
                onClick: () => setCreateOpen(true),
              },
            ]}
          />
        }
      />

      <RoleDemoBar
        role={urlState.role}
        demoFlag={urlState.demoFlag}
        onRole={(r) => patchUrl({ role: r })}
        onFlag={(f) => patchUrl({ demoFlag: f })}
      />

      {policy?.state === "UNCONFIGURED" ? (
        <Alert variant="warning">
          <AlertTitle>期间策略未配置（PERIOD_POLICY_UNCONFIGURED）</AlertTitle>
          <AlertDescription>
            {policy.blocker.message}
            列表仍可显式查询历史结算单，但新建草稿入口已关闭。
          </AlertDescription>
        </Alert>
      ) : null}

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
        <MetricStrip columns={4} aria-label="结算数据更新">
          <MetricFilterItem
            label="待对账"
            value={data.totals.pendingReconcile}
            active={urlState.view === "pending" && !urlState.status}
            onClick={() =>
              patchUrl({
                view: "pending",
                status: undefined,
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
                page: 1,
              })
            }
          />
          <MetricFilterItem
            label="本期已确认金额"
            value={
              <MoneyValue
                value={data.metrics.confirmedAmount}
                taxBasis="gross"
              />
            }
            active={urlState.view === "confirmed"}
            onClick={() =>
              patchUrl({ view: "confirmed", status: undefined, page: 1 })
            }
          />
        </MetricStrip>
      ) : null}

      <BusinessTableFrame
        title="结算单列表"
        description={data?.filterSummary ?? "默认待处理"}
        toolbar={
          <ListToolbar
            search={
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
            }
            filters={
              <div className="flex flex-wrap items-center gap-2">
                <Tabs
                  value={urlState.view}
                  onValueChange={(v) =>
                    patchUrl({
                      view: v as SettlementsUrlState["view"],
                      page: 1,
                    })
                  }
                >
                  <TabsList>
                    {(
                      Object.keys(VIEW_LABEL) as Array<
                        keyof typeof VIEW_LABEL
                      >
                    ).map((k) => (
                      <TabsTrigger key={k} value={k}>
                        {VIEW_LABEL[k]}
                      </TabsTrigger>
                    ))}
                  </TabsList>
                </Tabs>
                <OptionCombobox
                  value={urlState.supplierId || null}
                  onValueChange={(v) =>
                    patchUrl({
                      supplierId: v || undefined,
                      page: 1,
                    })
                  }
                  options={[
                    { value: "", label: "全部供应商" },
                    ...(data?.suppliers ?? []).map((s) => ({
                      value: s.supplierId,
                      label: s.supplierName,
                    })),
                  ]}
                  className="w-[10rem]"
                  size="sm"
                  aria-label="供应商"
                  allowClear={false}
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
                  应用
                </Button>
              </div>
            }
            actions={
              <span className="text-xs text-muted-foreground" aria-live="polite">
                共 {(data?.total ?? 0).toLocaleString("zh-CN")} 条
              </span>
            }
          />
        }
        table={
          empty ? (
            <div className="p-6">
              {empty === "NO_PERMISSION" ? (
                <BusinessEmptyState
                  kind="no-scope"
                  title="无模块权限"
                  description="当前账号没有 API 供应商结算模块权限，入口应隐藏；此处为演示空态，不以 0 单据伪装。"
                />
              ) : empty === "NO_SCOPE" ? (
                <BusinessEmptyState
                  kind="no-scope"
                  title="无数据范围"
                  description="已授权模块但当前组织/供应商范围为空。请申请数据范围后重查，不以 0 条结算单伪装。"
                />
              ) : empty === "FILTER_NO_RESULT" ? (
                <BusinessEmptyState
                  kind="filter"
                  title="当前筛选无结果"
                  description={`筛选摘要：${data?.filterSummary ?? "—"}。可清除筛选回到默认待处理视图。`}
                  action={
                    <Button
                      type="button"
                      variant="outline"
                      onClick={() =>
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
                      }
                    >
                      清除筛选
                    </Button>
                  }
                />
              ) : (
                <BusinessEmptyState
                  kind="no-data"
                  title="当前范围没有结算单"
                  description={
                    policy?.state === "CONFIGURED"
                      ? "期间策略已配置，可由财务经办新建结算草稿。"
                      : "策略缺失时仅可查询历史；请先完成期间策略配置。"
                  }
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
                setPagination(next)
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
          <p className="text-sm text-muted-foreground">未找到预览行</p>
        )}
      </QuickPreviewSheet>

      <Dialog open={createOpen} onOpenChange={setCreateOpen}>
        <DialogContent>
          <DialogHeader>
            <DialogTitle>新建结算草稿</DialogTitle>
            <DialogDescription>
              必须引用服务端当前供应商结算期间策略及版本，并选择其返回的完整周期；策略缺失或版本过期时将拒绝创建。
            </DialogDescription>
          </DialogHeader>
          {policy?.state !== "CONFIGURED" ? (
            <Alert variant="destructive">
              <AlertTitle>无法创建</AlertTitle>
              <AlertDescription>
                PERIOD_POLICY_UNCONFIGURED 或策略不可用，不创建草稿。
              </AlertDescription>
            </Alert>
          ) : (
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
                    <SupplierCombobox
                      value={field.state.value || undefined}
                      onValueChange={(id) => field.handleChange(id ?? "")}
                      suppliers={data?.suppliers ?? []}
                      placeholder="请选择供应商"
                    />
                  </div>
                )}
              />
              <form.AppField
                name="periodKey"
                children={(field) => (
                  <div className="space-y-1.5">
                    <Label htmlFor="periodKey">结算期间（策略周期）</Label>
                    <OptionCombobox
                      id="periodKey"
                      value={field.state.value || null}
                      onValueChange={(v) => field.handleChange(v ?? "")}
                      options={[
                        { value: "", label: "请选择" },
                        ...policy.selectablePeriods.map((p) => ({
                          value: `${p.periodStart}|${p.periodEnd}`,
                          label: p.label,
                        })),
                      ]}
                      placeholder="请选择"
                      allowClear={false}
                    />
                    <p className="text-xs text-muted-foreground">
                      策略 {policy.policyId}@{policy.policyVersion} ·{" "}
                      {policy.timezone}
                    </p>
                  </div>
                )}
              />
              <DialogFooter>
                <Button
                  type="button"
                  variant="outline"
                  onClick={() => setCreateOpen(false)}
                >
                  取消
                </Button>
                <form.AppForm>
                  <form.SubmitButton label="确认创建草稿" />
                </form.AppForm>
              </DialogFooter>
            </form>
          )}
        </DialogContent>
      </Dialog>
    </div>
  )
}

function SettlementCenter({
  statementId,
  urlState,
  patchUrl,
  onBack,
}: {
  statementId: string
  urlState: SettlementsUrlState
  patchUrl: (patch: Partial<SettlementsUrlState>) => void
  onBack: () => void
}) {
  const detailQuery = useSettlementDetailQuery(statementId, urlState.role)
  const refreshMutation = useRefreshTrialMutation()
  const resolveMutation = useResolveDifferenceMutation()
  const evidenceMutation = useAppendEvidenceMutation()
  const submitMutation = useSubmitReviewMutation()
  const decisionMutation = useReviewDecisionMutation()
  const queryIdem = useQueryFormalIdempotencyMutation()

  const [result, setResult] = React.useState<ResultState>(null)
  const [activeDiffId, setActiveDiffId] = React.useState<string | null>(null)
  const [resolveOpen, setResolveOpen] = React.useState(false)
  const [evidenceOpen, setEvidenceOpen] = React.useState(false)
  const [submitOpen, setSubmitOpen] = React.useState(false)
  const [confirmOpen, setConfirmOpen] = React.useState(false)
  const [rejectOpen, setRejectOpen] = React.useState(false)
  const [resolution, setResolution] =
    React.useState<DifferenceResolution>("ERP_ACCEPTED")
  const [reasonCode, setReasonCode] = React.useState("BILL_ALIGNED")
  const [evidenceComment, setEvidenceComment] = React.useState("")
  const [rejectReason, setRejectReason] = React.useState("")
  const [reviewComment, setReviewComment] = React.useState("")
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
      <div className="mx-auto flex w-full max-w-shell flex-col gap-4 p-4 md:p-5">
        <div className="h-10 w-56 animate-pulse rounded-lg bg-muted" />
        <div className="h-24 animate-pulse rounded-xl bg-muted" />
        <div className="h-96 animate-pulse rounded-2xl bg-muted" />
      </div>
    )
  }

  if (detailQuery.isError || !data) {
    return (
      <div className="mx-auto flex w-full max-w-shell flex-col gap-4 p-4 md:p-5">
        <Button type="button" variant="ghost" size="sm" onClick={onBack}>
          <ArrowLeftIcon className="size-4" />
          返回列表
        </Button>
        <BusinessFailureState
          kind="system"
          title="结算单加载失败"
          description="请返回列表或重试。"
          action={
            <Button type="button" onClick={() => void detailQuery.refetch()}>
              重试
            </Button>
          }
        />
      </div>
    )
  }

  const detail = data
  const st = detail.statement
  const allowed = new Set(detail.allowedActions)
  const blockers = detail.actionBlockers
  const activeDiff =
    detail.differences.find((d) => d.differenceId === activeDiffId) ??
    detail.differences[0] ??
    null

  const confirmBlocker = blockerOf(blockers, "CONFIRM")
  const submitBlocker = blockerOf(blockers, "SUBMIT_REVIEW")

  async function onRefresh() {
    const outcome = await refreshMutation.mutateAsync({
      statementId: st.id,
      expectedLockVersion: st.lockVersion,
      expectedSourceSnapshotHash: st.sourceSnapshotHash,
      role: urlState.role,
      requestId: newKey("req"),
      idempotencyKey: newKey("refresh"),
    })
    setResult(outcomeToResult(outcome))
  }

  async function onResolve() {
    if (!activeDiff) return
    const outcome = await resolveMutation.mutateAsync({
      statementId: st.id,
      differenceId: activeDiff.differenceId,
      expectedLockVersion: st.lockVersion,
      expectedDifferenceVersion: activeDiff.version,
      resolution,
      reasonCode,
      role: urlState.role,
      operationId: newKey("op"),
      idempotencyKey: newKey("resolve"),
    })
    setResult(outcomeToResult(outcome))
    if (outcome.status === "succeeded") setResolveOpen(false)
  }

  async function onEvidence() {
    if (!activeDiff) return
    const outcome = await evidenceMutation.mutateAsync({
      statementId: st.id,
      differenceId: activeDiff.differenceId,
      expectedDifferenceVersion: activeDiff.version,
      opinionCode: "PROCUREMENT_NOTE",
      comment: evidenceComment,
      role: urlState.role,
      requestId: newKey("req"),
      idempotencyKey: newKey("ev"),
    })
    setResult(outcomeToResult(outcome))
    if (outcome.status === "succeeded") {
      setEvidenceOpen(false)
      setEvidenceComment("")
    }
  }

  async function onSubmitReview() {
    const cutoff = detail.refreshCutoffPolicy
    if (cutoff.state !== "CONFIGURED") {
      setResult({
        status: "blocked",
        title: "刷新截止策略未配置",
        description: cutoff.blocker.message,
      })
      return
    }
    const outcome = await submitMutation.mutateAsync({
      statementId: st.id,
      expectedLockVersion: st.lockVersion,
      subjectHash: st.subjectHash ?? `sh_${st.id}`,
      refreshCutoffPolicyId: cutoff.policyId,
      expectedRefreshCutoffPolicyVersion: cutoff.policyVersion,
      role: urlState.role,
      operationId: newKey("op"),
      idempotencyKey: newKey("submit"),
      comment: reviewComment || undefined,
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
      claimToken: "demo_claim",
      leaseVersion: detail.workItem.leaseVersion ?? 1,
      expectedSubjectVersion: detail.workItem.subjectVersion,
      expectedSubjectHash: detail.workItem.subjectHash,
      expectedLockVersion: st.lockVersion,
      action: "CONFIRM",
      role: urlState.role,
      operationId: newKey("op"),
      idempotencyKey: newKey("confirm"),
      comment: reviewComment || undefined,
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
      claimToken: "demo_claim",
      leaseVersion: detail.workItem.leaseVersion ?? 1,
      expectedSubjectVersion: detail.workItem.subjectVersion,
      expectedSubjectHash: detail.workItem.subjectHash,
      expectedLockVersion: st.lockVersion,
      action: "REJECT",
      role: urlState.role,
      operationId: newKey("op"),
      idempotencyKey: newKey("reject"),
      reasonCode: rejectReason || "NEEDS_MORE_EVIDENCE",
      comment: reviewComment || undefined,
    })
    setResult(outcomeToResult(outcome))
    if (outcome.status === "rejected" || outcome.status === "succeeded") {
      setRejectOpen(false)
    }
  }

  return (
    <div className="mx-auto flex w-full max-w-shell flex-col gap-4 p-4 md:p-5">
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
            label: "API 结算",
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

      <RoleDemoBar
        role={urlState.role}
        demoFlag={urlState.demoFlag}
        onRole={(r) => patchUrl({ role: r })}
        onFlag={(f) => patchUrl({ demoFlag: f })}
      />

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
              记录 {formatTime(detail.freshness.immutableFactsAsOf)}
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
              <Button type="button" size="sm" disabled title={submitBlocker.message}>
                提交复核（已阻断）
              </Button>
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
              <Button
                type="button"
                size="sm"
                disabled
                title={confirmBlocker.message}
              >
                确认结算（已阻断）
              </Button>
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
        b.code === "BLOCKING_DIFFERENCES" ||
        b.code === "REFRESH_CUTOFF_POLICY_UNCONFIGURED"
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
                    b.code === "BLOCKING_DIFFERENCES" ||
                    b.code === "REFRESH_CUTOFF_POLICY_UNCONFIGURED"
                )
                .map((b) => (
                  <li key={`${b.action}-${b.code}`}>
                    <span className="font-mono text-xs">{b.code}</span> ·{" "}
                    {b.message}
                  </li>
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
                {result.pendingIdempotencyKey ? (
                  <Button
                    type="button"
                    size="sm"
                    variant="outline"
                    disabled={queryIdem.isPending}
                    onClick={async () => {
                      const r = await queryIdem.mutateAsync(
                        result.pendingIdempotencyKey!
                      )
                      if (r) setResult(outcomeToResult(r))
                      else
                        setResult({
                          ...result,
                          description: "仍未知，请稍后用原任务号再查。",
                        })
                    }}
                  >
                    查询处理结果
                  </Button>
                ) : null}
              </div>
            }
          />
        ) : null}
      </div>

      <Card size="sm">
        <CardHeader className="border-b py-3">
          <CardTitle className="text-base">金额摘要</CardTitle>
          <CardDescription>
            订单 / 运费 / 服务费 / 退款 + ERP vs 供应商 + 差异方向 · 全部
            {detail.totals.taxBasisLabel} · 服务端舍入，前端不重算
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
                  "账单未同步（不可用 ERP 代填）"
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

      <div className="rounded-xl border bg-muted/30 px-3 py-2 text-xs text-muted-foreground">
        <span className="font-medium text-foreground">来源数据 </span>
        sourceAsOf={formatTime(st.sourceAsOf)} · sourceSnapshotHash=
        <span className="num font-mono">{st.sourceSnapshotHash}</span>
        {st.subjectHash ? (
          <>
            {" "}
            · subjectHash=
            <span className="num font-mono">{st.subjectHash}</span>
          </>
        ) : null}
        {st.externalBillNo ? (
          <>
            {" "}
            · 账单 {st.externalBillNo}@{st.externalBillVersion}
          </>
        ) : null}
        <span className="ml-2">
          W26 数据仅展示（
          {formatTime(detail.freshness.w26ProjectionUpdatedAt)}
          ），不参与正式取数
        </span>
      </div>

      <Tabs
        value={section}
        onValueChange={(v) =>
          patchUrl({ section: v as SettlementSection })
        }
      >
        <TabsList className="flex h-auto flex-wrap">
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

      {section === "overview" ? (
        <div className="grid gap-4 lg:grid-cols-2">
          <Card size="sm">
            <CardHeader className="border-b py-3">
              <CardTitle className="text-base">概览</CardTitle>
            </CardHeader>
            <CardContent className="space-y-2 pt-4 text-sm">
              <p>
                供应商：{st.supplierName}（记录时，不受后续更名影响）
              </p>
              <p className="num">
                期间：{st.periodStart} ~ {st.periodEnd}
              </p>
              <p>
                状态：{st.statusLabel} · 锁版本 {st.lockVersion}
              </p>
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
          <Card size="sm">
            <CardHeader className="border-b py-3">
              <CardTitle className="text-base">岗位与权限</CardTitle>
            </CardHeader>
            <CardContent className="space-y-2 pt-4 text-sm">
              <p>当前角色：{detail.viewerRoleLabel}</p>
              <p>采购：仅追加证据，不能结论/确认</p>
              <p>财务经办：差异结论与提交复核</p>
              <p>财务复核：须为不同人，确认后形成应付</p>
              <p className="text-muted-foreground">
                前端禁用态仅解释；服务端岗位分离校验为准
              </p>
            </CardContent>
          </Card>
        </div>
      ) : null}

      {section === "items" ? (
        <Card size="sm">
          <CardHeader className="border-b py-3">
            <CardTitle className="text-base">结算明细</CardTitle>
            <CardDescription>
              冻结数据 + 不可变完成/取消/退款记录 · 金额只读（canEditBillOrOrder=
              {String(detail.canEditBillOrOrder)}）
            </CardDescription>
          </CardHeader>
          <CardContent className="overflow-x-auto pt-0">
            <table className="w-full min-w-[48rem] text-left text-sm">
              <thead className="border-b text-xs text-muted-foreground">
                <tr>
                  <th className="px-2 py-2">供应商订单</th>
                  <th className="px-2 py-2">外部单号</th>
                  <th className="px-2 py-2">商品</th>
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
                      <span className="num font-medium">
                        {it.supplierOrderNo}
                      </span>
                    </td>
                    <td className="num px-2 py-2 text-muted-foreground">
                      {it.externalOrderNo}
                    </td>
                    <td className="px-2 py-2">{it.productName}</td>
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
                      colSpan={10}
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
          onSelect={setActiveDiffId}
          role={urlState.role}
          allowed={allowed}
          onResolve={() => setResolveOpen(true)}
          onEvidence={() => setEvidenceOpen(true)}
        />
      ) : null}

      {section === "review" ? (
        <Card size="sm">
          <CardHeader className="border-b py-3">
            <CardTitle className="text-base">复核记录</CardTitle>
            <CardDescription>
              提交 / 驳回 / 确认追加式记录；岗位分离由服务端校验
            </CardDescription>
          </CardHeader>
          <CardContent className="space-y-3 pt-4">
            {detail.workItem ? (
              <Alert variant="info">
                <AlertTitle>复核任务</AlertTitle>
                <AlertDescription>
                  {detail.workItem.workItemId} · subjectHash=
                  <span className="num font-mono">
                    {detail.workItem.subjectHash}
                  </span>
                  {detail.workItem.claimedBy
                    ? ` · 领取人 ${detail.workItem.claimedBy.displayName}`
                    : " · 待领取"}
                </AlertDescription>
              </Alert>
            ) : null}
            {detail.reviewRecords.length === 0 ? (
              <p className="text-sm text-muted-foreground">尚无复核记录</p>
            ) : (
              detail.reviewRecords.map((r) => (
                <div
                  key={r.recordId}
                  className="rounded-lg border px-3 py-2 text-sm"
                >
                  <div className="font-medium">
                    {r.actionLabel} · {r.by.displayName}
                  </div>
                  <div className="text-xs text-muted-foreground">
                    {formatTime(r.at)}
                    {r.reasonCode ? ` · ${r.reasonCode}` : ""}
                    {r.comment ? ` · ${r.comment}` : ""}
                  </div>
                </div>
              ))
            )}
          </CardContent>
        </Card>
      ) : null}

      {section === "payable" ? (
        <Card size="sm">
          <CardHeader className="border-b py-3">
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
        <Card size="sm">
          <CardHeader className="border-b py-3">
            <CardTitle className="text-base">审计</CardTitle>
          </CardHeader>
          <CardContent className="space-y-2 pt-4">
            {detail.auditEvents.map((e) => (
              <div
                key={e.eventId}
                className="rounded-lg border px-3 py-2 text-sm"
              >
                <div className="flex flex-wrap gap-2">
                  <span className="font-medium">{e.action}</span>
                  <span className="text-muted-foreground">{e.actor}</span>
                  {e.auditNo ? (
                    <span className="num text-xs">{e.auditNo}</span>
                  ) : null}
                </div>
                <p className="text-muted-foreground">{e.summary}</p>
                <p className="text-xs text-muted-foreground">
                  {formatTime(e.at)}
                </p>
              </div>
            ))}
          </CardContent>
        </Card>
      ) : null}

      {/* Resolve difference */}
      <Dialog open={resolveOpen} onOpenChange={setResolveOpen}>
        <DialogContent>
          <DialogHeader>
            <DialogTitle>登记差异处理结论</DialogTitle>
            <DialogDescription>
              财务经办追加式结论；不修改左右证据原值或历史成本。ERP
              认可表示接受账单并以成本差额表达。
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
        description="将按刷新截止策略冻结来来源更新时间、明细与差异结论，并创建唯一复核待办。"
        actionLabel="提交复核"
        confirmLabel="确认提交"
        fromStatus={{ label: st.statusLabel, tone: st.statusTone }}
        toStatus={{ label: "待复核", tone: "warning" }}
        lockedFields={[
          st.statementNo,
          `sourceSnapshotHash ${st.sourceSnapshotHash}`,
          `subjectHash ${st.subjectHash ?? "—"}`,
          detail.refreshCutoffPolicy.state === "CONFIGURED"
            ? `截止策略 ${detail.refreshCutoffPolicy.policyId}@${detail.refreshCutoffPolicy.policyVersion}`
            : "截止策略未配置",
        ]}
        effects={["冻结来源数据与差异结论", "创建 SUPPLIER_SETTLEMENT_REVIEW 待办"]}
        pending={submitMutation.isPending}
        onConfirm={async () => {
          await onSubmitReview()
        }}
      />
      {submitOpen ? (
        <div className="sr-only">
          <Label htmlFor="sub-comment">说明（可选）</Label>
          <Textarea
            id="sub-comment"
            value={reviewComment}
            onChange={(e) => setReviewComment(e.target.value)}
            rows={2}
          />
        </div>
      ) : null}

      <FormalActionConfirmDialog
        open={confirmOpen}
        onOpenChange={setConfirmOpen}
        title="确认结算（不可逆）"
        description="同事务追加成本差额、形成唯一应付并锁定处理结果。经办人不可确认本单。"
        actionLabel="确认结算"
        confirmLabel="确认结算"
        fromStatus={{ label: "待复核", tone: "warning" }}
        toStatus={{ label: "已确认", tone: "success" }}
        lockedFields={[
          st.statementNo,
          `应付金额预览 ${st.supplierAmountGross ?? st.erpAmountGross}`,
          `成本差额预览 ${detail.totals.pendingCostDeltaGross ?? "0.00"}`,
          `经办 ${st.preparedBy?.displayName ?? "—"}`,
        ]}
        effects={[
          "追加成本差额 cost_entry",
          "形成唯一供应商结算应付",
          "锁定处理结果，不可撤回确认",
        ]}
        irreversibleEffects={["确认后付款/进项发票/核销进入供应商往来"]}
        nextDepartment="W12 供应商往来"
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
              variant="outline"
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
    </div>
  )
}

function DifferencesWorkspace({
  differences,
  activeDiff,
  onSelect,
  role,
  allowed,
  onResolve,
  onEvidence,
}: {
  differences: SettlementDifferenceView[]
  activeDiff: SettlementDifferenceView | null
  onSelect: (id: string) => void
  role: DemoRole
  allowed: Set<string>
  onResolve: () => void
  onEvidence: () => void
}) {
  if (differences.length === 0) {
    return (
      <BusinessEmptyState
        kind="no-data"
        title="无差异"
        description="当前结算单没有差异记录，可直接进入复核（在截止策略与明细守恒满足时）。"
      />
    )
  }

  return (
    <div className="grid gap-4 xl:grid-cols-[16rem_minmax(0,1fr)]">
      <Card size="sm">
        <CardHeader className="border-b py-3">
          <CardTitle className="text-base">差异列表</CardTitle>
        </CardHeader>
        <CardContent className="space-y-1 p-2">
          {differences.map((d) => (
            <button
              key={d.differenceId}
              type="button"
              className={`flex w-full flex-col rounded-lg px-2 py-2 text-left text-sm hover:bg-muted ${
                activeDiff?.differenceId === d.differenceId ? "bg-muted" : ""
              }`}
              onClick={() => onSelect(d.differenceId)}
            >
              <span className="font-medium">{d.typeLabel}</span>
              <span className="text-xs text-muted-foreground">
                {d.statusLabel}
                {d.blocking ? " · 阻断" : ""}
              </span>
            </button>
          ))}
        </CardContent>
      </Card>

      {activeDiff ? (
        <div className="space-y-4">
          <Card size="sm">
            <CardHeader className="border-b py-3">
              <div className="flex flex-wrap items-center justify-between gap-2">
                <div>
                  <CardTitle className="text-base">
                    {activeDiff.typeLabel}
                  </CardTitle>
                  <CardDescription>
                    {activeDiff.amountDirectionLabel}
                    {activeDiff.amountGross
                      ? ` · 差异额 ${activeDiff.amountGross}（含税）`
                      : ""}
                  </CardDescription>
                </div>
                <BusinessStatusBadge
                  context="list"
                  label={activeDiff.statusLabel}
                  tone={activeDiff.statusTone}
                />
              </div>
            </CardHeader>
            <CardContent className="space-y-3 pt-4">
              <div className="grid gap-2 sm:grid-cols-2 text-sm">
                <div className="rounded-lg border p-3">
                  <div className="text-xs text-muted-foreground">ERP 侧</div>
                  <div>{activeDiff.erpSideLabel}</div>
                  {activeDiff.erpSideAmount ? (
                    <MoneyValue
                      value={activeDiff.erpSideAmount}
                      taxBasis="gross"
                    />
                  ) : null}
                </div>
                <div className="rounded-lg border p-3">
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
                        className="rounded-lg border px-3 py-2 text-sm"
                      >
                        <div className="font-medium">
                          {e.label} · {e.by.displayName}
                        </div>
                        <div className="text-muted-foreground">
                          {e.comment}
                        </div>
                        <div className="text-xs text-muted-foreground">
                          {formatTime(e.at)}
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
                    {formatTime(activeDiff.resolution.at)} · 成本预览{" "}
                    {activeDiff.resolution.costImpactPreview ?? "0.00"}（含税）
                  </AlertDescription>
                </Alert>
              ) : null}

              <Separator />
              <div className="flex flex-wrap gap-2">
                {role === "procurement" && allowed.has("APPEND_EVIDENCE") ? (
                  <Button type="button" size="sm" onClick={onEvidence}>
                    追加采购证据
                  </Button>
                ) : null}
                {role === "finance_prep" &&
                allowed.has("RESOLVE_DIFFERENCE") &&
                activeDiff.status !== "RESOLVED" ? (
                  <Button type="button" size="sm" onClick={onResolve}>
                    登记结论
                  </Button>
                ) : null}
                {role === "procurement" && !allowed.has("RESOLVE_DIFFERENCE") ? (
                  <span className="text-xs text-muted-foreground">
                    采购不可选择差异结论
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
