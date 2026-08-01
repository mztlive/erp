"use client"

import * as React from "react"
import Link from "next/link"
import { usePathname, useRouter, useSearchParams } from "next/navigation"
import type { ColumnDef, PaginationState } from "@tanstack/react-table"
import {
  ArrowLeftIcon,
  BanIcon,
  ExternalLinkIcon,
  RefreshCwIcon,
  ShieldAlertIcon,
  TriangleAlertIcon,
} from "lucide-react"

import {
  BackgroundJobProgress,
  BusinessEmptyState,
  BusinessStatusBadge,
  BusinessTableFrame,
  DataFreshness,
  DataTable,
  DocumentHeader,
  FormalActionResult,
  ImportStageIndicator,
  ListToolbar,
  MaintenanceBanner,
  MetricItem,
  MetricStrip,
  PageHeader,
  type ImportStageStates,
} from "@/components/business"
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
import { Input } from "@/components/ui/input"
import { Label } from "@/components/ui/label"
import {
  Select,
  SelectContent,
  SelectItem,
  SelectTrigger,
  SelectValue,
} from "@/components/ui/select"
import { Separator } from "@/components/ui/separator"
import { Tabs, TabsList, TabsTrigger } from "@/components/ui/tabs"
import { useIsMobile } from "@/hooks/use-mobile"
import {
  useConsumptionCutoverQuery,
  useCutoverDemoReadyMutation,
  useMaintenanceFreezeQuery,
  useMigrationFormalMutation,
  useOwnershipMigrationBatchQuery,
  useOwnershipMigrationListQuery,
} from "@/features/ownership-migration/queries"
import type {
  BatchStatus,
  BlockerCode,
  ConfirmationFilter,
  ConfirmationSummary,
  CutoverCheck,
  MigrationFormalResult,
  MigrationItem,
  MigrationWizardStage,
  OwnershipMigrationBatchRow,
  OwnershipMigrationBatchView,
  ViewerRoleDemo,
} from "@/features/ownership-migration/types"
import {
  BATCH_STATUS_LABEL,
  BATCH_STATUS_TONE,
  BLOCKER_CODE_LABEL,
  CONFIRMATION_STATE_LABEL,
  CONFIRMATION_STATE_TONE,
  ITEM_STATUS_LABEL,
  ROLE_LABEL,
  WIZARD_ORDER,
  WIZARD_STAGE_LABEL,
  WIZARD_TO_INDICATOR,
} from "@/features/ownership-migration/types"
import {
  buildOwnershipMigrationSearchParams,
  parseOwnershipMigrationSearchParams,
  type OwnershipMigrationUrlState,
} from "@/features/ownership-migration/url-state"
import { MALL } from "@/mock/ownership-migration"

function formatTime(iso?: string) {
  if (!iso) return "—"
  try {
    return new Intl.DateTimeFormat("zh-CN", {
      dateStyle: "medium",
      timeStyle: "short",
    }).format(new Date(iso))
  } catch {
    return iso
  }
}

function newRequestId(prefix: string) {
  return `${prefix}_${Date.now().toString(36)}_${Math.random().toString(36).slice(2, 8)}`
}

function buildStageStates(current: MigrationWizardStage): ImportStageStates {
  const currentIdx = WIZARD_ORDER.indexOf(current)
  const states: {
    [K in import("@/components/business").ImportStageKey]: {
      status: "pending" | "current" | "complete" | "failed"
      description?: string
    }
  } = {
    upload: { status: "pending", description: WIZARD_STAGE_LABEL.SCOPE },
    mapping: { status: "pending", description: WIZARD_STAGE_LABEL.CONFIRMATIONS },
    validation: { status: "pending", description: WIZARD_STAGE_LABEL.FREEZE_SYNC },
    preview: { status: "pending", description: WIZARD_STAGE_LABEL.BASELINE },
    submission: { status: "pending", description: WIZARD_STAGE_LABEL.EXECUTION },
    result: { status: "pending", description: WIZARD_STAGE_LABEL.COMPLETE },
  }
  for (let i = 0; i < WIZARD_ORDER.length; i += 1) {
    const stage = WIZARD_ORDER[i]!
    const key = WIZARD_TO_INDICATOR[stage]
    let status: "pending" | "current" | "complete" | "failed" = "pending"
    if (i < currentIdx) status = "complete"
    else if (i === currentIdx) status = "current"
    states[key] = { status, description: WIZARD_STAGE_LABEL[stage] }
  }
  return states
}

function ConfirmationBadge({ summary }: { summary: ConfirmationSummary }) {
  return (
    <BusinessStatusBadge
      context="list"
      label={CONFIRMATION_STATE_LABEL[summary.state]}
      tone={CONFIRMATION_STATE_TONE[summary.state]}
    />
  )
}

function RoleDemoBar({
  role,
  onChange,
}: {
  role: ViewerRoleDemo
  onChange: (r: ViewerRoleDemo) => void
}) {
  return (
    <div className="flex flex-wrap items-center gap-2 rounded-xl border bg-muted/40 px-3 py-2 text-sm">
      <span className="text-muted-foreground">演示角色</span>
      <Select
        value={role}
        onValueChange={(v) => {
          if (v == null) return
          onChange(v as ViewerRoleDemo)
        }}
      >
        <SelectTrigger className="h-8 w-[14rem]" size="sm">
          <SelectValue />
        </SelectTrigger>
        <SelectContent>
          {(Object.keys(ROLE_LABEL) as ViewerRoleDemo[]).map((r) => (
            <SelectItem key={r} value={r}>
              {ROLE_LABEL[r]}
            </SelectItem>
          ))}
        </SelectContent>
      </Select>
      <span className="text-xs text-muted-foreground">
        角色仅影响 allowedActions / 掩码，不写入业务事实
      </span>
    </div>
  )
}

function Fact({
  label,
  value,
  mono,
}: {
  label: string
  value: React.ReactNode
  mono?: boolean
}) {
  return (
    <div className="space-y-0.5">
      <div className="text-xs text-muted-foreground">{label}</div>
      <div className={mono ? "num font-mono text-sm" : "text-sm"}>{value}</div>
    </div>
  )
}

export function OwnershipMigrationPage() {
  const router = useRouter()
  const pathname = usePathname()
  const searchParams = useSearchParams()

  const urlState = React.useMemo(
    () => parseOwnershipMigrationSearchParams(searchParams),
    [searchParams]
  )

  const role: ViewerRoleDemo = urlState.role ?? "SYSTEM_ADMIN"

  const replaceUrl = React.useCallback(
    (next: OwnershipMigrationUrlState) => {
      const qs = buildOwnershipMigrationSearchParams(next)
      router.replace(`${pathname}${qs}`, { scroll: false })
    },
    [pathname, router]
  )

  const patchUrl = React.useCallback(
    (patch: Partial<OwnershipMigrationUrlState>) => {
      replaceUrl({ ...urlState, ...patch })
    },
    [replaceUrl, urlState]
  )

  if (urlState.panel === "cutover") {
    return (
      <CutoverView
        urlState={urlState}
        role={role}
        patchUrl={patchUrl}
        replaceUrl={replaceUrl}
      />
    )
  }

  if (urlState.panel === "batch" && urlState.batchId) {
    return (
      <BatchWizardView
        batchId={urlState.batchId}
        urlState={urlState}
        role={role}
        patchUrl={patchUrl}
        replaceUrl={replaceUrl}
      />
    )
  }

  return (
    <OverviewView
      urlState={urlState}
      role={role}
      patchUrl={patchUrl}
      replaceUrl={replaceUrl}
    />
  )
}

function OverviewView({
  urlState,
  role,
  patchUrl,
  replaceUrl,
}: {
  urlState: OwnershipMigrationUrlState
  role: ViewerRoleDemo
  patchUrl: (patch: Partial<OwnershipMigrationUrlState>) => void
  replaceUrl: (next: OwnershipMigrationUrlState) => void
}) {
  const isMobile = useIsMobile()
  const freezeQuery = useMaintenanceFreezeQuery()
  const [qDraft, setQDraft] = React.useState(urlState.q ?? "")

  React.useEffect(() => {
    setQDraft(urlState.q ?? "")
  }, [urlState.q])

  const listQuery = useOwnershipMigrationListQuery({
    mallId: urlState.mall ?? MALL.id,
    customerId: urlState.customer,
    status: urlState.status,
    confirmation: urlState.confirmation,
    blocker: urlState.blocker,
    view: urlState.view,
    q: urlState.q,
    page: urlState.page,
    pageSize: 20,
    role,
  })

  const data = listQuery.data
  const freeze = freezeQuery.data

  const [pagination, setPagination] = React.useState<PaginationState>({
    pageIndex: Math.max(0, urlState.page - 1),
    pageSize: 20,
  })

  React.useEffect(() => {
    setPagination((p) => ({ ...p, pageIndex: Math.max(0, urlState.page - 1) }))
  }, [urlState.page])

  const columns = React.useMemo<ColumnDef<OwnershipMigrationBatchRow>[]>(
    () => [
      {
        id: "batchNo",
        accessorKey: "batchNo",
        header: "批次号",
        cell: ({ row }) => (
          <button
            type="button"
            className="num font-mono text-sm font-medium text-primary underline-offset-2 hover:underline"
            onClick={() =>
              patchUrl({
                panel: "batch",
                batchId: row.original.batchId,
                stage: "SCOPE",
              })
            }
          >
            {row.original.batchNo}
          </button>
        ),
      },
      {
        id: "customer",
        accessorKey: "customerName",
        header: "客户",
        cell: ({ row }) => (
          <div className="space-y-0.5">
            <div className="text-sm font-medium">{row.original.customerName}</div>
            <div className="text-xs text-muted-foreground">
              单客户范围 · {row.original.customerId}
            </div>
          </div>
        ),
      },
      {
        id: "eligible",
        header: "销售单数",
        cell: ({ row }) => (
          <span className="num text-sm">
            可迁 {row.original.eligibleCount}
            {row.original.blockedCount > 0
              ? ` · 阻塞 ${row.original.blockedCount}`
              : ""}
          </span>
        ),
      },
      {
        id: "salesConf",
        header: "销售确认",
        cell: ({ row }) => (
          <ConfirmationBadge summary={row.original.salesConfirmation} />
        ),
      },
      {
        id: "financeConf",
        header: "财务确认",
        cell: ({ row }) => (
          <ConfirmationBadge summary={row.original.financeConfirmation} />
        ),
      },
      {
        id: "baselineConf",
        header: "基线确认",
        cell: ({ row }) => (
          <ConfirmationBadge summary={row.original.baselineConfirmation} />
        ),
      },
      {
        id: "freeze",
        header: "冻结",
        cell: ({ row }) =>
          row.original.freezeActive ? (
            <Badge variant="warning">冻结中</Badge>
          ) : (
            <Badge variant="outline">未冻结</Badge>
          ),
      },
      {
        id: "status",
        header: "批次状态",
        cell: ({ row }) => (
          <BusinessStatusBadge
            context="list"
            label={BATCH_STATUS_LABEL[row.original.status]}
            tone={BATCH_STATUS_TONE[row.original.status]}
          />
        ),
      },
      {
        id: "blocker",
        header: "阻塞/失败",
        cell: ({ row }) => (
          <span className="text-sm text-muted-foreground">
            {row.original.primaryBlockerLabel ??
              row.original.errorSummary ??
              "—"}
          </span>
        ),
      },
      {
        id: "updated",
        header: "最后更新",
        cell: ({ row }) => (
          <span className="text-xs text-muted-foreground">
            {formatTime(row.original.updatedAt)}
          </span>
        ),
      },
      {
        id: "actions",
        header: "操作",
        cell: ({ row }) => (
          <div className="flex flex-wrap gap-1">
            <Button
              type="button"
              size="xs"
              variant="secondary"
              onClick={() =>
                patchUrl({
                  panel: "batch",
                  batchId: row.original.batchId,
                  stage: "SCOPE",
                })
              }
            >
              打开批次
            </Button>
          </div>
        ),
      },
    ],
    [patchUrl]
  )

  const hasFilters = Boolean(
    urlState.customer ||
      urlState.confirmation ||
      urlState.blocker ||
      urlState.q ||
      (urlState.status && urlState.status !== "open") ||
      urlState.view === "my_customers"
  )

  return (
    <div className="mx-auto flex w-full max-w-shell flex-col gap-4 p-4 md:p-5">
      {freeze?.active ? (
        <MaintenanceBanner
          tone="warning"
          icon={ShieldAlertIcon}
          title={`维护冻结中 · ${freeze.sourceMallName}`}
          description={
            <div className="space-y-1 text-sm">
              <p>
                开始于 {formatTime(freeze.startedAt)} · {freeze.scopeLabel}
              </p>
              <p>
                当前阶段：{freeze.stageLabel} · 责任：{freeze.responsibleRole}
              </p>
              <p className="text-muted-foreground">
                冻结动作：{freeze.frozenActions.join("、")}
              </p>
              <p className="font-medium">
                本 Banner 由服务端冻结事实驱动，不可忽略或暂时关闭。
              </p>
            </div>
          }
          action={{
            label: "查看进度",
            onClick: () => {
              /* 已在 W24 */
            },
          }}
        />
      ) : null}

      <PageHeader
        title="主责迁移批次"
        description="按客户迁移正式存量卡券销售单主责（福利商城 → ERP）；原子提交、职责分离、唯一 T。"
        breadcrumbs={[
          {
            id: "gov",
            label: "治理",
            href: "/governance/ownership-migrations",
            current: false,
          },
          { id: "om", label: "主责迁移", current: true },
        ]}
        metadata={
          <DataFreshness
            updatedAt={data?.queriedAt ? formatTime(data.queriedAt) : "刚刚"}
            dateTime={data?.queriedAt ?? new Date().toISOString()}
            state={listQuery.isFetching ? "stale" : "fresh"}
            label="迁移批次"
          />
        }
        actions={
          <div className="flex flex-wrap gap-2">
            <Button
              type="button"
              size="sm"
              variant="outline"
              onClick={() =>
                patchUrl({
                  panel: "cutover",
                  batchId: undefined,
                  stage: undefined,
                })
              }
            >
              切换检查 / 登记 T
            </Button>
            <Button
              type="button"
              size="sm"
              variant="ghost"
              onClick={() => void listQuery.refetch()}
            >
              <RefreshCwIcon className="size-4" />
              刷新
            </Button>
          </div>
        }
      />

      <RoleDemoBar
        role={role}
        onChange={(r) =>
          patchUrl({ role: r === "SYSTEM_ADMIN" ? undefined : r })
        }
      />

      {data && !data.hasModuleAccess ? (
        <BusinessEmptyState
          kind="no-scope"
          title="无 W24 管理权限"
          description="不展示技术批次页。业务用户仍可从全局维护 Banner 查看授权摘要。"
        />
      ) : null}

      {data?.hasModuleAccess && !data.hasCustomerScope ? (
        <BusinessEmptyState
          kind="no-scope"
          title="当前角色无客户数据范围"
          description="「我的客户清单」无数据；不显示全局批次数量与技术指标。"
        />
      ) : null}

      {data?.hasModuleAccess && data.hasCustomerScope ? (
        <>
          <MetricStrip columns={4} aria-label="迁移总览指标">
            <MetricItem
              label="待准备客户"
              value={data.metrics.pendingPrepare}
            />
            <MetricItem label="待销售确认" value={data.metrics.pendingSales} />
            <MetricItem label="待财务确认" value={data.metrics.pendingFinance} />
            <MetricItem
              label="待基线确认"
              value={data.metrics.pendingBaseline}
            />
            <MetricItem label="可执行批次" value={data.metrics.executable} />
            <MetricItem
              label="执行失败·仍冻结"
              value={data.metrics.failedFrozen}
            />
            <MetricItem label="已完成" value={data.metrics.completed} />
          </MetricStrip>

          <Card size="sm">
            <CardContent className="grid gap-3 pt-4 sm:grid-cols-2 lg:grid-cols-5">
              <Fact
                label="一期同步水位"
                value={data.statusSummary.phase1WatermarkLabel}
                mono
              />
              <Fact
                label="冻结状态"
                value={
                  data.statusSummary.freezeActive ? (
                    <Badge variant="warning">冻结中</Badge>
                  ) : (
                    <Badge variant="outline">未冻结</Badge>
                  )
                }
              />
              <Fact
                label="已迁移/总客户"
                value={`${data.statusSummary.migratedCustomers} / ${data.statusSummary.totalCustomers}`}
              />
              <Fact
                label="已迁移/总销售单"
                value={`${data.statusSummary.migratedOrders} / ${data.statusSummary.totalOrders}`}
              />
              <Fact
                label="T 状态"
                value={
                  data.statusSummary.tStatus === "ENABLED"
                    ? `已登记 ${formatTime(data.statusSummary.tEnabledAt)}`
                    : "尚未登记"
                }
              />
            </CardContent>
          </Card>

          <ListToolbar
            filters={
              <div className="flex flex-wrap items-end gap-2">
                <div className="space-y-1">
                  <Label className="text-xs">来源商城</Label>
                  <Select
                    value={urlState.mall ?? MALL.id}
                    onValueChange={(v) => {
                      if (v == null) return
                      patchUrl({ mall: v, page: 1 })
                    }}
                  >
                    <SelectTrigger className="h-8 w-[12rem]" size="sm">
                      <SelectValue />
                    </SelectTrigger>
                    <SelectContent>
                      <SelectItem value={MALL.id}>{MALL.name}</SelectItem>
                    </SelectContent>
                  </Select>
                </div>
                <div className="space-y-1">
                  <Label className="text-xs">客户</Label>
                  <Input
                    className="h-8 w-[10rem]"
                    placeholder="客户名/编号"
                    defaultValue={urlState.customer ?? ""}
                    key={urlState.customer ?? "cust"}
                    onBlur={(e) =>
                      patchUrl({
                        customer: e.target.value.trim() || undefined,
                        page: 1,
                      })
                    }
                    onKeyDown={(e) => {
                      if (e.key === "Enter") {
                        patchUrl({
                          customer:
                            (e.target as HTMLInputElement).value.trim() ||
                            undefined,
                          page: 1,
                        })
                      }
                    }}
                  />
                </div>
                <div className="space-y-1">
                  <Label className="text-xs">批次状态</Label>
                  <Select
                    value={urlState.status ?? "open"}
                    onValueChange={(v) => {
                      if (v == null) return
                      patchUrl({
                        status: v as BatchStatus | "open",
                        page: 1,
                      })
                    }}
                  >
                    <SelectTrigger className="h-8 w-[11rem]" size="sm">
                      <SelectValue />
                    </SelectTrigger>
                    <SelectContent>
                      <SelectItem value="open">未完成与失败</SelectItem>
                      {(
                        Object.keys(BATCH_STATUS_LABEL) as BatchStatus[]
                      ).map((s) => (
                        <SelectItem key={s} value={s}>
                          {BATCH_STATUS_LABEL[s]}
                        </SelectItem>
                      ))}
                    </SelectContent>
                  </Select>
                </div>
                <div className="space-y-1">
                  <Label className="text-xs">确认状态</Label>
                  <Select
                    value={urlState.confirmation ?? "all"}
                    onValueChange={(v) => {
                      if (v == null) return
                      patchUrl({
                        confirmation:
                          v === "all"
                            ? undefined
                            : (v as ConfirmationFilter),
                        page: 1,
                      })
                    }}
                  >
                    <SelectTrigger className="h-8 w-[10rem]" size="sm">
                      <SelectValue placeholder="全部" />
                    </SelectTrigger>
                    <SelectContent>
                      <SelectItem value="all">全部确认</SelectItem>
                      <SelectItem value="pending_sales">待销售</SelectItem>
                      <SelectItem value="pending_finance">待财务</SelectItem>
                      <SelectItem value="pending_baseline">待基线</SelectItem>
                      <SelectItem value="invalidated">确认失效</SelectItem>
                    </SelectContent>
                  </Select>
                </div>
                <div className="space-y-1">
                  <Label className="text-xs">阻塞类型</Label>
                  <Select
                    value={urlState.blocker ?? "all"}
                    onValueChange={(v) => {
                      if (v == null) return
                      patchUrl({
                        blocker:
                          v === "all" ? undefined : (v as BlockerCode),
                        page: 1,
                      })
                    }}
                  >
                    <SelectTrigger className="h-8 w-[10rem]" size="sm">
                      <SelectValue placeholder="全部" />
                    </SelectTrigger>
                    <SelectContent>
                      <SelectItem value="all">全部阻塞</SelectItem>
                      {(Object.keys(BLOCKER_CODE_LABEL) as BlockerCode[]).map(
                        (code) => (
                          <SelectItem key={code} value={code}>
                            {BLOCKER_CODE_LABEL[code]}
                          </SelectItem>
                        )
                      )}
                    </SelectContent>
                  </Select>
                </div>
                <div className="space-y-1">
                  <Label className="text-xs">搜索</Label>
                  <form
                    className="flex gap-1"
                    onSubmit={(e) => {
                      e.preventDefault()
                      patchUrl({ q: qDraft || undefined, page: 1 })
                    }}
                  >
                    <Input
                      className="h-8 w-[12rem]"
                      value={qDraft}
                      onChange={(e) => setQDraft(e.target.value)}
                      placeholder="批次号 / 客户"
                    />
                    <Button type="submit" size="sm" variant="secondary">
                      搜索
                    </Button>
                  </form>
                </div>
                {hasFilters ? (
                  <Button
                    type="button"
                    size="sm"
                    variant="ghost"
                    onClick={() =>
                      replaceUrl({
                        page: 1,
                        panel: "overview",
                        role: urlState.role,
                        mall: urlState.mall,
                      })
                    }
                  >
                    清除筛选
                  </Button>
                ) : null}
              </div>
            }
          />

          <Alert>
            <ShieldAlertIcon />
            <AlertTitle>范围与职责</AlertTitle>
            <AlertDescription>
              仅已生效及之后、未作废的正式存量卡券销售单可迁移；商城草稿不入批次与统计。每批仅一位客户。销售 / 财务 / 基线确认职责分离，管理员不可代签。成功仅改主责标记。
              {data.financeFieldsMasked ? (
                <span className="mt-1 block font-medium">
                  当前角色票款字段已掩码，仅见确认状态。
                </span>
              ) : null}
            </AlertDescription>
          </Alert>

          {listQuery.isError ? (
            <BusinessEmptyState
              kind="no-data"
              title="批次列表加载失败"
              description="请重试。冻结 Banner 仍以独立查询为准。"
              action={
                <Button type="button" onClick={() => void listQuery.refetch()}>
                  重试
                </Button>
              }
            />
          ) : data.totalCount === 0 && !hasFilters ? (
            <BusinessEmptyState
              kind="no-data"
              title="尚未建立迁移批次"
              description="创建批次须明确唯一客户，并完成正式范围预检。商城草稿不会进入批次。"
            />
          ) : data.totalCount === 0 && hasFilters ? (
            <BusinessEmptyState
              kind="filter"
              title="当前筛选无结果"
              description="没有批次符合商城 / 客户 / 状态 / 确认 / 阻塞条件。"
              action={
                <Button
                  type="button"
                  variant="secondary"
                  onClick={() =>
                    replaceUrl({
                      page: 1,
                      panel: "overview",
                      role: urlState.role,
                      mall: urlState.mall,
                    })
                  }
                >
                  清除筛选
                </Button>
              }
            />
          ) : (
            <BusinessTableFrame
              title="迁移批次"
              description={`${data.mallName} · 共 ${data.totalCount} 批 · 单客户原子批次`}
              table={
                <DataTable
                  data={[...data.rows]}
                  columns={columns}
                  getRowId={(row) => row.batchId}
                  rowCount={data.totalCount}
                  pagination={pagination}
                  onPaginationChange={(next) => {
                    setPagination(next)
                    patchUrl({ page: next.pageIndex + 1 })
                  }}
                  layout="flush"
                  density="compact"
                  loading={listQuery.isPending}
                />
              }
            />
          )}

          {isMobile ? (
            <Alert variant="info">
              <AlertTitle>移动端只读</AlertTitle>
              <AlertDescription>
                手机视口不提供创建批次、确认基线、执行迁移或登记 T 等高风险动作。
              </AlertDescription>
            </Alert>
          ) : null}
        </>
      ) : null}
    </div>
  )
}

function BatchWizardView({
  batchId,
  urlState,
  role,
  patchUrl,
  replaceUrl,
}: {
  batchId: string
  urlState: OwnershipMigrationUrlState
  role: ViewerRoleDemo
  patchUrl: (patch: Partial<OwnershipMigrationUrlState>) => void
  replaceUrl: (next: OwnershipMigrationUrlState) => void
}) {
  const isMobile = useIsMobile()
  const detailQuery = useOwnershipMigrationBatchQuery(batchId, role)
  const formalMutation = useMigrationFormalMutation()
  const freezeQuery = useMaintenanceFreezeQuery()
  const [lastResult, setLastResult] = React.useState<MigrationFormalResult | null>(
    null
  )

  const batch = detailQuery.data
  const stage = urlState.stage ?? batch?.stage ?? "SCOPE"

  const runFormal = async (
    action: Parameters<typeof formalMutation.mutateAsync>[0]["action"],
    extra?: Partial<Parameters<typeof formalMutation.mutateAsync>[0]>
  ) => {
    const result = await formalMutation.mutateAsync({
      batchId,
      action,
      expectedObjectVersion: batch?.objectVersion,
      expectedScopeHash: batch?.scopeHash,
      requestId: newRequestId(action.toLowerCase()),
      role,
      ...extra,
    })
    setLastResult(result)
    return result
  }

  if (detailQuery.isPending) {
    return (
      <div className="mx-auto flex w-full max-w-shell flex-col gap-4 p-4 md:p-5">
        <div className="h-10 w-48 animate-pulse rounded-lg bg-muted" />
        <div className="h-24 animate-pulse rounded-2xl bg-muted" />
        <div className="h-40 animate-pulse rounded-2xl bg-muted" />
      </div>
    )
  }

  if (detailQuery.isError || !batch) {
    return (
      <div className="mx-auto flex w-full max-w-shell flex-col gap-4 p-4 md:p-5">
        <BusinessEmptyState
          kind="no-data"
          title="批次不存在或无权查看"
          description="请返回总览或检查客户数据范围。"
          action={
            <Button
              type="button"
              variant="secondary"
              onClick={() =>
                replaceUrl({
                  ...urlState,
                  panel: "overview",
                  batchId: undefined,
                  stage: undefined,
                })
              }
            >
              返回总览
            </Button>
          }
        />
      </div>
    )
  }

  const stageStates = buildStageStates(stage)
  const hideHighRisk = isMobile
  const can = (action: string) => batch.allowedActions.includes(action)
  const blockersFor = (action: string) =>
    batch.actionBlockers.filter((b) => b.action === action || b.action === "*")

  return (
    <div className="mx-auto flex w-full max-w-shell flex-col gap-4 p-4 md:p-5">
      {(batch.freeze.active || freezeQuery.data?.active) && (
        <MaintenanceBanner
          tone="warning"
          icon={ShieldAlertIcon}
          title={`维护冻结 · ${batch.identity.sourceMallName}`}
          description={
            <div className="space-y-1 text-sm">
              <p>{batch.freeze.scopeLabel}</p>
              {batch.freeze.startedAt ? (
                <p>开始于 {formatTime(batch.freeze.startedAt)}</p>
              ) : null}
              <p className="font-medium">不可忽略 · 无本地绕过入口</p>
            </div>
          }
        />
      )}

      <div className="flex flex-wrap items-center gap-2">
        <Button
          type="button"
          variant="ghost"
          size="sm"
          onClick={() =>
            replaceUrl({
              ...urlState,
              panel: "overview",
              batchId: undefined,
              stage: undefined,
            })
          }
        >
          <ArrowLeftIcon className="size-4" />
          返回总览
        </Button>
        <RoleDemoBar
          role={role}
          onChange={(r) =>
            patchUrl({ role: r === "SYSTEM_ADMIN" ? undefined : r })
          }
        />
      </div>

      <DocumentHeader
        title="主责迁移批次"
        documentNumber={batch.identity.batchNo}
        primaryStatus={{
          label: BATCH_STATUS_LABEL[batch.status],
          tone: BATCH_STATUS_TONE[batch.status],
        }}
        version={batch.objectVersion}
        statuses={[
          {
            id: "customer",
            label: "客户",
            status: {
              label: batch.identity.customerName,
              tone: "neutral",
            },
          },
          {
            id: "mall",
            label: "来源商城",
            status: {
              label: batch.identity.sourceMallName,
              tone: "info",
            },
          },
          {
            id: "freeze",
            label: "冻结",
            status: {
              label: batch.freeze.active ? "冻结中" : "未冻结",
              tone: batch.freeze.active ? "warning" : "neutral",
            },
          },
        ]}
        secondaryActions={
          !hideHighRisk ? (
            <>
              <Button
                type="button"
                size="sm"
                variant="outline"
                disabled={formalMutation.isPending}
                onClick={() => void runFormal("RECHECK_SCOPE")}
              >
                重新预检范围
              </Button>
              <Button
                type="button"
                size="sm"
                variant="outline"
                disabled={formalMutation.isPending}
                onClick={() => void runFormal("DEMO_INVALIDATE_SCOPE")}
              >
                演示：scopeHash 变化使确认失效
              </Button>
            </>
          ) : null
        }
      />

      <Card size="sm">
        <CardContent className="grid gap-3 pt-4 sm:grid-cols-2 lg:grid-cols-5">
          <Fact label="scopeHash" value={batch.scopeHash} mono />
          <Fact label="销售版本摘要" value={batch.salesVersionSummary} />
          <Fact
            label="票款摘要"
            value={
              batch.financeSummaryMasked ? (
                <span className="text-muted-foreground">
                  {batch.financeSummary}
                </span>
              ) : (
                batch.financeSummary
              )
            }
          />
          <Fact label="卡实例/余额基线" value={batch.cardBaselineSummary} />
          <Fact
            label="最后水位"
            value={batch.lastSyncWatermark ?? "—"}
            mono
          />
        </CardContent>
      </Card>

      <Alert variant="info">
        <AlertTitle>成功语义（固定）</AlertTitle>
        <AlertDescription>{batch.successSemanticsNote}</AlertDescription>
      </Alert>

      <ImportStageIndicator
        stages={stageStates}
        aria-label="主责迁移阶段"
      />

      <Tabs
        value={stage}
        onValueChange={(v) => {
          if (v == null) return
          patchUrl({ stage: v as MigrationWizardStage })
        }}
      >
        <TabsList className="flex h-auto flex-wrap">
          {WIZARD_ORDER.map((s) => (
            <TabsTrigger key={s} value={s}>
              {WIZARD_STAGE_LABEL[s]}
            </TabsTrigger>
          ))}
        </TabsList>
      </Tabs>

      {lastResult ? (
        <FormalActionResult
          status={
            lastResult.status === "COMMITTED"
              ? "succeeded"
              : lastResult.status === "RUNNING"
                ? "processing"
                : lastResult.status === "RESULT_UNKNOWN"
                  ? "unknown"
                  : "blocked"
          }
          title={lastResult.message}
          description={lastResult.nextAction}
          reference={lastResult.operationId}
        />
      ) : null}

      {batch.formalResult ? (
        <FormalActionResult
          status={
            batch.formalResult.status === "COMMITTED"
              ? "succeeded"
              : batch.formalResult.status === "RUNNING"
                ? "processing"
                : batch.formalResult.status === "RESULT_UNKNOWN"
                  ? "unknown"
                  : "rejected"
          }
          title={batch.formalResult.title}
          description={batch.formalResult.description}
          reference={batch.formalResult.operationId}
        />
      ) : null}

      {stage === "SCOPE" ? <ScopeSection batch={batch} /> : null}
      {stage === "CONFIRMATIONS" ? (
        <ConfirmationsSection
          batch={batch}
          role={role}
          hideHighRisk={hideHighRisk}
          can={can}
          blockersFor={blockersFor}
          pending={formalMutation.isPending}
          onConfirm={(action) => void runFormal(action)}
        />
      ) : null}
      {stage === "FREEZE_SYNC" ? (
        <FreezeSyncSection
          batch={batch}
          hideHighRisk={hideHighRisk}
          can={can}
          pending={formalMutation.isPending}
          onStartFreeze={() => void runFormal("START_FREEZE")}
          onFinalSync={() => void runFormal("RUN_FINAL_SYNC")}
        />
      ) : null}
      {stage === "BASELINE" ? (
        <BaselineSection
          batch={batch}
          role={role}
          hideHighRisk={hideHighRisk}
          can={can}
          blockersFor={blockersFor}
          pending={formalMutation.isPending}
          onConfirm={() => void runFormal("CONFIRM_BASELINE")}
          onFinalSync={() => void runFormal("RUN_FINAL_SYNC")}
        />
      ) : null}
      {stage === "EXECUTION" ? (
        <ExecutionSection
          batch={batch}
          hideHighRisk={hideHighRisk}
          can={can}
          blockersFor={blockersFor}
          pending={formalMutation.isPending}
          onExecute={() => void runFormal("EXECUTE_BATCH")}
          onResume={() => void runFormal("RESUME_BATCH")}
        />
      ) : null}
      {stage === "COMPLETE" ? <CompleteSection batch={batch} /> : null}

      <ItemsTable batch={batch} />
    </div>
  )
}

function ScopeSection({ batch }: { batch: OwnershipMigrationBatchView }) {
  return (
    <div className="grid gap-4 lg:grid-cols-2">
      <Card size="sm">
        <CardHeader className="border-b">
          <CardTitle>迁移范围</CardTitle>
          <CardDescription>
            仅已生效及之后、未作废的正式存量卡券销售单；一批仅一位客户。
          </CardDescription>
        </CardHeader>
        <CardContent className="space-y-3 pt-4">
          <MetricStrip columns={4} aria-label="范围计数">
            <MetricItem label="清单项" value={batch.counts.total} />
            <MetricItem label="可迁移" value={batch.counts.eligible} />
            <MetricItem label="阻塞" value={batch.counts.blocked} />
            <MetricItem
              label="已迁移"
              value={
                batch.status === "COMPLETED" ? batch.counts.migrated : 0
              }
            />
          </MetricStrip>
          <p className="text-xs text-muted-foreground">
            「已迁移」仅在全批原子提交后计数；检查通过 ≠ 已提交。
          </p>
        </CardContent>
      </Card>
      <Card size="sm">
        <CardHeader className="border-b">
          <CardTitle>排除说明（不入统计）</CardTitle>
          <CardDescription>
            商城草稿与已作废单不迁移、不补建、不进入正式统计。
          </CardDescription>
        </CardHeader>
        <CardContent className="space-y-2 pt-4">
          {batch.exclusions.length === 0 ? (
            <p className="text-sm text-muted-foreground">本批无排除项说明。</p>
          ) : (
            batch.exclusions.map((ex) => (
              <div
                key={ex.kind}
                className="flex items-start gap-2 rounded-lg border p-2 text-sm"
              >
                <Badge variant="secondary">
                  {ex.kind === "MALL_DRAFT" ? "商城草稿" : "已作废"} · {ex.count}
                </Badge>
                <span className="text-muted-foreground">{ex.reason}</span>
              </div>
            ))
          )}
          {batch.checks.map((c) => (
            <div key={c.code} className="flex flex-wrap items-center gap-2 text-sm">
              <Badge
                variant={
                  c.status === "PASSED"
                    ? "secondary"
                    : c.status === "BLOCKED"
                      ? "destructive"
                      : "outline"
                }
              >
                {c.status}
              </Badge>
              <span className="font-mono text-xs">{c.code}</span>
              <span className="text-muted-foreground">{c.summary}</span>
              {c.destinationWorkspaceId ? (
                <Button
                  variant="link"
                  size="sm"
                  className="h-auto p-0"
                  render={<Link href="/governance/mall-sync" />}
                >
                  前往 {c.destinationWorkspaceId}
                  <ExternalLinkIcon className="size-3.5" />
                </Button>
              ) : null}
            </div>
          ))}
        </CardContent>
      </Card>
    </div>
  )
}

function ConfirmationsSection({
  batch,
  role,
  hideHighRisk,
  can,
  blockersFor,
  pending,
  onConfirm,
}: {
  batch: OwnershipMigrationBatchView
  role: ViewerRoleDemo
  hideHighRisk: boolean
  can: (a: string) => boolean
  blockersFor: (a: string) => { code: string; message: string }[]
  pending: boolean
  onConfirm: (action: "CONFIRM_SALES" | "CONFIRM_FINANCE") => void
}) {
  return (
    <div className="space-y-3">
      <Alert>
        <BanIcon />
        <AlertTitle>三类确认独立 · 管理员不可代签</AlertTitle>
        <AlertDescription>
          销售与财务通过 W02 完整任务信封提交；当前演示角色为{" "}
          <strong>{ROLE_LABEL[role]}</strong>
          。scopeHash / 分面变化会使旧确认失效并保留审计。
        </AlertDescription>
      </Alert>
      <div className="grid gap-4 lg:grid-cols-3">
        <ConfirmationCard
          title="销售清单确认"
          facet="sales"
          summary={batch.confirmations.sales}
          scopeHash={batch.scopeHash}
          objectSummary={batch.salesVersionSummary}
          canAct={!hideHighRisk && can("CONFIRM_SALES")}
          blockers={blockersFor("CONFIRM_SALES")}
          pending={pending}
          onConfirm={() => onConfirm("CONFIRM_SALES")}
        />
        <ConfirmationCard
          title="财务清单确认"
          facet="finance"
          summary={batch.confirmations.finance}
          scopeHash={batch.scopeHash}
          objectSummary={
            batch.financeSummaryMasked
              ? "（金额已掩码）"
              : batch.financeSummary
          }
          canAct={!hideHighRisk && can("CONFIRM_FINANCE")}
          blockers={blockersFor("CONFIRM_FINANCE")}
          pending={pending}
          onConfirm={() => onConfirm("CONFIRM_FINANCE")}
        />
        <ConfirmationCard
          title="最终权威基线"
          facet="baseline"
          summary={batch.confirmations.baseline}
          scopeHash={batch.scopeHash}
          objectSummary={`水位 ${batch.lastSyncWatermark ?? "—"} · 须在冻结+同步+核对后确认`}
          canAct={false}
          blockers={[
            {
              code: "USE_BASELINE_STAGE",
              message: "请在「最终基线」阶段由上线负责人确认。",
            },
            ...blockersFor("CONFIRM_BASELINE"),
          ]}
          pending={false}
          onConfirm={() => {}}
        />
      </div>
    </div>
  )
}

function ConfirmationCard({
  title,
  facet,
  summary,
  scopeHash,
  objectSummary,
  canAct,
  blockers,
  pending,
  onConfirm,
}: {
  title: string
  facet: string
  summary: ConfirmationSummary
  scopeHash: string
  objectSummary: string
  canAct: boolean
  blockers: { code: string; message: string }[]
  pending: boolean
  onConfirm: () => void
}) {
  return (
    <Card size="sm" data-facet={facet}>
      <CardHeader className="border-b">
        <div className="flex flex-wrap items-center justify-between gap-2">
          <CardTitle className="text-base">{title}</CardTitle>
          <ConfirmationBadge summary={summary} />
        </div>
        <CardDescription>独立确认卡 · 不可与其它分面合并代签</CardDescription>
      </CardHeader>
      <CardContent className="space-y-3 pt-4 text-sm">
        <Fact label="当前 scopeHash" value={scopeHash} mono />
        <Fact label="对象摘要" value={objectSummary} />
        {summary.state === "VALID" ? (
          <>
            <Fact label="确认人" value={summary.confirmedBy ?? "—"} />
            <Fact label="确认时间" value={formatTime(summary.confirmedAt)} />
            <Fact label="subjectHash" value={summary.subjectHash ?? "—"} mono />
          </>
        ) : null}
        {summary.state === "INVALIDATED" ? (
          <Alert variant="destructive">
            <TriangleAlertIcon />
            <AlertTitle>范围已变化，需重新确认</AlertTitle>
            <AlertDescription className="space-y-1">
              <p>{summary.invalidatedReason}</p>
              {summary.priorAudit ? (
                <p className="text-xs">
                  保留审计：{summary.priorAudit.confirmedBy} ·{" "}
                  {formatTime(summary.priorAudit.confirmedAt)} ·{" "}
                  <span className="font-mono">
                    {summary.priorAudit.subjectHash}
                  </span>
                </p>
              ) : null}
            </AlertDescription>
          </Alert>
        ) : null}
        {blockers.length > 0 ? (
          <ul className="list-disc space-y-1 pl-4 text-xs text-muted-foreground">
            {blockers.map((b) => (
              <li key={b.code}>
                <span className="font-mono">{b.code}</span>：{b.message}
              </li>
            ))}
          </ul>
        ) : null}
        <Button
          type="button"
          size="sm"
          disabled={!canAct || pending || summary.state === "VALID"}
          onClick={onConfirm}
        >
          {summary.state === "VALID" ? "已确认" : "提交确认"}
        </Button>
      </CardContent>
    </Card>
  )
}

function FreezeSyncSection({
  batch,
  hideHighRisk,
  can,
  pending,
  onStartFreeze,
  onFinalSync,
}: {
  batch: OwnershipMigrationBatchView
  hideHighRisk: boolean
  can: (a: string) => boolean
  pending: boolean
  onStartFreeze: () => void
  onFinalSync: () => void
}) {
  return (
    <Card size="sm">
      <CardHeader className="border-b">
        <CardTitle>冻结与最后同步</CardTitle>
        <CardDescription>
          冻结写入服务端事实后全局 Banner 生效；失败不得显示已冻结。
        </CardDescription>
      </CardHeader>
      <CardContent className="space-y-3 pt-4">
        <Fact
          label="冻结"
          value={
            batch.freeze.active
              ? `生效中 · ${formatTime(batch.freeze.startedAt)}`
              : "未生效"
          }
        />
        <Fact
          label="最后水位"
          value={batch.lastSyncWatermark ?? "尚未执行最后同步"}
          mono
        />
        <Fact
          label="全量核对"
          value={batch.fullReconcileDone ? "已完成" : "未完成"}
        />
        {!hideHighRisk ? (
          <div className="flex flex-wrap gap-2">
            <Button
              type="button"
              size="sm"
              disabled={pending || batch.freeze.active}
              onClick={onStartFreeze}
            >
              启动维护冻结
            </Button>
            <Button
              type="button"
              size="sm"
              variant="secondary"
              disabled={pending || !batch.freeze.active}
              onClick={onFinalSync}
            >
              执行最后一期同步
            </Button>
          </div>
        ) : (
          <p className="text-sm text-muted-foreground">
            移动端隐藏高风险冻结/同步动作。
          </p>
        )}
        {!can("START_FREEZE") && !batch.freeze.active ? (
          <p className="text-xs text-muted-foreground">
            启动冻结由上线窗口与服务端权限控制（演示可在有权角色下执行）。
          </p>
        ) : null}
      </CardContent>
    </Card>
  )
}

function BaselineSection({
  batch,
  role,
  hideHighRisk,
  can,
  blockersFor,
  pending,
  onConfirm,
  onFinalSync,
}: {
  batch: OwnershipMigrationBatchView
  role: ViewerRoleDemo
  hideHighRisk: boolean
  can: (a: string) => boolean
  blockersFor: (a: string) => { code: string; message: string }[]
  pending: boolean
  onConfirm: () => void
  onFinalSync: () => void
}) {
  const gatesOk =
    batch.freeze.active &&
    Boolean(batch.lastSyncWatermark) &&
    batch.fullReconcileDone

  return (
    <div className="grid gap-4 lg:grid-cols-2">
      <Card size="sm">
        <CardHeader className="border-b">
          <CardTitle>最终权威基线</CardTitle>
          <CardDescription>
            基线登记不生成新销售版本；迁移执行基线为第一份投影修订。
          </CardDescription>
        </CardHeader>
        <CardContent className="space-y-3 pt-4">
          <GateRow ok={batch.freeze.active} label="维护冻结已生效" />
          <GateRow
            ok={Boolean(batch.lastSyncWatermark)}
            label="最后一期同步水位已记录"
          />
          <GateRow ok={batch.fullReconcileDone} label="全量核对已完成" />
          <GateRow
            ok={batch.confirmations.sales.state === "VALID"}
            label="销售确认有效"
          />
          <GateRow
            ok={batch.confirmations.finance.state === "VALID"}
            label="财务确认有效"
          />
          <Fact
            label="基线确认状态"
            value={
              <ConfirmationBadge summary={batch.confirmations.baseline} />
            }
          />
          {batch.confirmations.baseline.state === "INVALIDATED" ? (
            <Alert variant="destructive">
              <AlertTitle>基线确认已失效</AlertTitle>
              <AlertDescription>
                {batch.confirmations.baseline.invalidatedReason}
              </AlertDescription>
            </Alert>
          ) : null}
          <p className="text-xs text-muted-foreground">
            当前角色：{ROLE_LABEL[role]} · 仅上线负责人可确认基线
          </p>
          {!hideHighRisk ? (
            <div className="flex flex-wrap gap-2">
              <Button
                type="button"
                size="sm"
                variant="secondary"
                disabled={pending || !batch.freeze.active}
                onClick={onFinalSync}
              >
                重新执行最后同步
              </Button>
              <Button
                type="button"
                size="sm"
                disabled={
                  pending ||
                  !can("CONFIRM_BASELINE") ||
                  !gatesOk ||
                  batch.confirmations.baseline.state === "VALID"
                }
                onClick={onConfirm}
              >
                确认最终权威基线
              </Button>
            </div>
          ) : (
            <p className="text-sm text-muted-foreground">
              移动端不提供基线确认。
            </p>
          )}
          {blockersFor("CONFIRM_BASELINE").map((b) => (
            <p key={b.code} className="text-xs text-muted-foreground">
              <span className="font-mono">{b.code}</span>：{b.message}
            </p>
          ))}
        </CardContent>
      </Card>
      <Card size="sm">
        <CardHeader className="border-b">
          <CardTitle>执行基线说明</CardTitle>
        </CardHeader>
        <CardContent className="space-y-2 pt-4 text-sm">
          <p>
            每项迁移关联的{" "}
            <code className="text-xs">baselineProjectionRevisionId</code>{" "}
            是第一份执行投影修订，不是新销售版本。
          </p>
          <p className="text-muted-foreground">
            示例：{batch.items.find((i) => i.baselineProjectionRevisionId)
              ?.baselineProjectionRevisionId ?? "—"}
          </p>
          <p className="text-muted-foreground">
            销售版本 ID 保持不变：{" "}
            {batch.items.find((i) => i.baselineSalesOrderRevisionId)
              ?.baselineSalesOrderRevisionId ?? "—"}
          </p>
        </CardContent>
      </Card>
    </div>
  )
}

function GateRow({ ok, label }: { ok: boolean; label: string }) {
  return (
    <div className="flex items-start gap-2 text-sm">
      <Badge variant={ok ? "secondary" : "destructive"}>
        {ok ? "已满足" : "未满足"}
      </Badge>
      <span className={ok ? "text-foreground" : "text-muted-foreground"}>
        {label}
      </span>
    </div>
  )
}

function ExecutionSection({
  batch,
  hideHighRisk,
  can,
  blockersFor,
  pending,
  onExecute,
  onResume,
}: {
  batch: OwnershipMigrationBatchView
  hideHighRisk: boolean
  can: (a: string) => boolean
  blockersFor: (a: string) => { code: string; message: string }[]
  pending: boolean
  onExecute: () => void
  onResume: () => void
}) {
  const job = batch.backgroundOperation
  return (
    <div className="space-y-4">
      <Alert variant="warning">
        <TriangleAlertIcon />
        <AlertTitle>原子提交 · 无部分成功</AlertTitle>
        <AlertDescription>
          任一项失败则全批未提交。进度百分比仅表示后台操作进度，不表示项目已正式迁移。
          失败保持冻结并使用原批次续跑。
        </AlertDescription>
      </Alert>

      <BackgroundJobProgress
        mode="all-or-nothing"
        status={
          job?.status === "succeeded"
            ? "succeeded"
            : job?.status === "failed"
              ? "failed"
              : job?.status === "running"
                ? "running"
                : job?.status === "frozen"
                  ? "frozen"
                  : batch.status === "FAILED"
                    ? "failed"
                    : batch.status === "COMPLETED"
                      ? "succeeded"
                      : "queued"
        }
        total={batch.counts.eligible}
        completed={
          batch.status === "COMPLETED"
            ? batch.counts.eligible
            : job
              ? Math.round(
                  (job.progressPercent / 100) * batch.counts.eligible
                )
              : 0
        }
        label="迁移后台进度（非正式成功数）"
        description={
          job?.progressLabel ??
          "整批原子执行；未提交前所有项均为未迁移。"
        }
      />

      {batch.status === "FAILED" ? (
        <FormalActionResult
          status="rejected"
          title="本批未提交，维护冻结仍有效"
          description="界面不得展示「已迁移 N 项」的部分成功语义。其它已完成客户批次不回退。"
          reference={job?.operationId}
          actions={
            !hideHighRisk && can("RESUME_BATCH") ? (
              <Button
                type="button"
                size="sm"
                disabled={pending}
                onClick={onResume}
              >
                原批次续跑
              </Button>
            ) : null
          }
        />
      ) : null}

      {!hideHighRisk ? (
        <div className="flex flex-wrap gap-2">
          <Button
            type="button"
            size="sm"
            disabled={pending || !can("EXECUTE_BATCH")}
            onClick={onExecute}
          >
            执行迁移批次（全批提交或全批不提交）
          </Button>
          {can("RESUME_BATCH") ? (
            <Button
              type="button"
              size="sm"
              variant="secondary"
              disabled={pending}
              onClick={onResume}
            >
              原批次续跑
            </Button>
          ) : null}
        </div>
      ) : (
        <p className="text-sm text-muted-foreground">
          移动端隐藏执行与续跑等高风险动作。
        </p>
      )}

      {blockersFor("EXECUTE_BATCH").map((b) => (
        <p key={b.code} className="text-xs text-muted-foreground">
          <span className="font-mono">{b.code}</span>：{b.message}
        </p>
      ))}
    </div>
  )
}

function CompleteSection({ batch }: { batch: OwnershipMigrationBatchView }) {
  return (
    <div className="space-y-3">
      <FormalActionResult
        status="succeeded"
        title="批次完成"
        description={batch.successSemanticsNote}
        facts={[
          {
            label: "已迁移销售单",
            value: String(batch.counts.migrated),
          },
          {
            label: "主责变化",
            value: "福利商城 → ERP（仅标记）",
          },
          {
            label: "单号 / 应收 / 回款 / 发票",
            value: "均未变更",
          },
        ]}
      />
      <Alert>
        <AlertTitle>不可回退</AlertTitle>
        <AlertDescription>
          页面不提供恢复商城主责、重开 B2B 建单或恢复一期轮询的动作。
        </AlertDescription>
      </Alert>
    </div>
  )
}

function ItemsTable({ batch }: { batch: OwnershipMigrationBatchView }) {
  const columns = React.useMemo<ColumnDef<MigrationItem>[]>(
    () => [
      {
        id: "order",
        header: "销售单",
        cell: ({ row }) => (
          <div className="space-y-0.5">
            <div className="num font-mono text-sm">
              {row.original.salesOrderNo}
            </div>
            <div className="text-xs text-muted-foreground">
              来源 {row.original.sourceOrderNo}
            </div>
          </div>
        ),
      },
      {
        id: "status",
        header: "当前状态",
        cell: ({ row }) => (
          <span className="text-sm">{row.original.salesOrderStatus}</span>
        ),
      },
      {
        id: "owner",
        header: "主责变化",
        cell: () => (
          <span className="text-sm">福利商城 → ERP</span>
        ),
      },
      {
        id: "checks",
        header: "检查",
        cell: ({ row }) => (
          <span className="text-xs text-muted-foreground">
            明细 {row.original.checkResults.singleLine ?? "—"} · 映射{" "}
            {row.original.checkResults.mapping ?? "—"}
          </span>
        ),
      },
      {
        id: "baseline",
        header: "基线",
        cell: ({ row }) => (
          <div className="space-y-0.5 text-xs">
            <div className="font-mono">
              销售版本 {row.original.baselineSalesOrderRevisionId ?? "—"}
            </div>
            <div className="font-mono text-muted-foreground">
              投影修订 {row.original.baselineProjectionRevisionId ?? "—"}
            </div>
          </div>
        ),
      },
      {
        id: "result",
        header: "项目结果",
        cell: ({ row }) => (
          <div className="space-y-0.5">
            <Badge
              variant={
                row.original.itemStatus === "MIGRATED"
                  ? "secondary"
                  : row.original.itemStatus === "BLOCKED" ||
                      row.original.itemStatus === "NOT_MIGRATED"
                    ? "destructive"
                    : row.original.itemStatus.startsWith("EXCLUDED")
                      ? "outline"
                      : "info"
              }
            >
              {ITEM_STATUS_LABEL[row.original.itemStatus]}
            </Badge>
            {row.original.exclusionReason ? (
              <div className="text-xs text-muted-foreground">
                {row.original.exclusionReason}
              </div>
            ) : null}
            {row.original.errorSummary ? (
              <div className="text-xs text-muted-foreground">
                {row.original.errorSummary}
              </div>
            ) : null}
          </div>
        ),
      },
    ],
    []
  )

  return (
    <BusinessTableFrame
      title="迁移项"
      description="检查通过不等于已提交；仅全批事务提交后显示「已迁移」。不含商城草稿统计。"
      table={
        <DataTable
          data={[...batch.items]}
          columns={columns}
          getRowId={(row) => row.itemId}
          rowCount={batch.items.length}
          layout="flush"
          density="compact"
        />
      }
    />
  )
}

function CutoverView({
  urlState,
  role,
  patchUrl,
  replaceUrl,
}: {
  urlState: OwnershipMigrationUrlState
  role: ViewerRoleDemo
  patchUrl: (patch: Partial<OwnershipMigrationUrlState>) => void
  replaceUrl: (next: OwnershipMigrationUrlState) => void
}) {
  const isMobile = useIsMobile()
  const mallId = urlState.mall ?? MALL.id
  const cutoverQuery = useConsumptionCutoverQuery(mallId, role)
  const formalMutation = useMigrationFormalMutation()
  const readyMutation = useCutoverDemoReadyMutation()
  const [lastResult, setLastResult] = React.useState<MigrationFormalResult | null>(
    null
  )

  const cutover = cutoverQuery.data

  const registerT = async () => {
    const result = await formalMutation.mutateAsync({
      cutoverId: cutover?.cutoverId,
      action: "ENABLE_CUTOVER",
      expectedObjectVersion: cutover?.objectVersion,
      requestId: newRequestId("enable_cutover"),
      role,
    })
    setLastResult(result)
  }

  const queryResult = async () => {
    const result = await formalMutation.mutateAsync({
      cutoverId: cutover?.cutoverId,
      action: "QUERY_FORMAL_RESULT",
      requestId: newRequestId("query_t"),
      role,
    })
    setLastResult(result)
  }

  if (cutoverQuery.isPending) {
    return (
      <div className="mx-auto flex w-full max-w-shell flex-col gap-4 p-4 md:p-5">
        <div className="h-10 w-56 animate-pulse rounded-lg bg-muted" />
        <div className="h-40 animate-pulse rounded-2xl bg-muted" />
      </div>
    )
  }

  if (!cutover) {
    return (
      <div className="mx-auto flex w-full max-w-shell flex-col gap-4 p-4 md:p-5">
        <BusinessEmptyState
          kind="no-data"
          title="切换记录不可用"
          action={
            <Button
              type="button"
              onClick={() =>
                replaceUrl({
                  ...urlState,
                  panel: "overview",
                })
              }
            >
              返回总览
            </Button>
          }
        />
      </div>
    )
  }

  const canRegister =
    !isMobile &&
    cutover.allowedActions.includes("ENABLE_CUTOVER") &&
    cutover.status !== "ENABLED"

  return (
    <div className="mx-auto flex w-full max-w-shell flex-col gap-4 p-4 md:p-5">
      <div className="flex flex-wrap items-center gap-2">
        <Button
          type="button"
          variant="ghost"
          size="sm"
          onClick={() =>
            replaceUrl({
              ...urlState,
              panel: "overview",
            })
          }
        >
          <ArrowLeftIcon className="size-4" />
          返回总览
        </Button>
        <RoleDemoBar
          role={role}
          onChange={(r) =>
            patchUrl({ role: r === "SYSTEM_ADMIN" ? undefined : r })
          }
        />
      </div>

      <PageHeader
        title={`消费回流启用 · ${cutover.mallName}`}
        description="商城级唯一 T：前提检查链尾全部通过后由上线负责人原子登记，不可改删。"
        breadcrumbs={[
          {
            id: "gov",
            label: "治理",
            href: "/governance/ownership-migrations",
            current: false,
          },
          {
            id: "om",
            label: "主责迁移",
            href: "/governance/ownership-migrations",
            current: false,
          },
          { id: "cut", label: "切换登记", current: true },
        ]}
        metadata={
          <Badge
            variant={cutover.status === "ENABLED" ? "secondary" : "warning"}
          >
            T：
            {cutover.status === "ENABLED"
              ? formatTime(cutover.enabledAt)
              : "尚未登记"}
          </Badge>
        }
        actions={
          <div className="flex flex-wrap gap-2">
            <Button
              type="button"
              size="sm"
              variant="outline"
              disabled={readyMutation.isPending || cutover.status === "ENABLED"}
              onClick={() => void readyMutation.mutateAsync()}
            >
              演示：前提全过
            </Button>
            <Button
              type="button"
              size="sm"
              variant="ghost"
              onClick={() => void cutoverQuery.refetch()}
            >
              <RefreshCwIcon className="size-4" />
              刷新
            </Button>
          </div>
        }
      />

      {lastResult ? (
        <FormalActionResult
          status={
            lastResult.status === "COMMITTED"
              ? "succeeded"
              : lastResult.status === "RESULT_UNKNOWN"
                ? "unknown"
                : "blocked"
          }
          title={lastResult.message}
          description={lastResult.nextAction}
          reference={lastResult.operationId}
          facts={
            lastResult.enabledAt
              ? [{ label: "T", value: formatTime(lastResult.enabledAt) }]
              : undefined
          }
          actions={
            lastResult.status === "RESULT_UNKNOWN" ? (
              <Button type="button" size="sm" onClick={() => void queryResult()}>
                查询切换记录
              </Button>
            ) : null
          }
        />
      ) : null}

      {cutover.status === "ENABLED" ? (
        <FormalActionResult
          status="succeeded"
          title="唯一 T 已登记"
          description="一经登记不可修改或删除。结果未知时须先查询，不创建第二个切换。"
          facts={[
            { label: "enabledAt", value: formatTime(cutover.enabledAt) },
            { label: "enabledBy", value: cutover.enabledBy ?? "—" },
            {
              label: "migrationScopeDigest",
              value: cutover.migrationScopeDigest,
            },
            {
              label: "confirmationDigest",
              value: cutover.confirmationDigest ?? "—",
            },
          ]}
        />
      ) : null}

      <Card size="sm">
        <CardHeader className="border-b">
          <CardTitle>必要前提</CardTitle>
          <CardDescription>
            链尾全部通过前无法登记 T；旧/失败/过期证据不能当作当前通过。
          </CardDescription>
        </CardHeader>
        <CardContent className="space-y-2 pt-4">
          {cutover.prerequisites.map((p) => (
            <GateRow key={p.key} ok={p.passed} label={`${p.label} — ${p.detail}`} />
          ))}
          <Separator className="my-2" />
          <Fact
            label="范围摘要"
            value={`批次 ${cutover.coveredBatchCount}/${cutover.totalTargetBatchCount} · 销售单 ${cutover.coveredSalesOrderCount} · digest ${cutover.migrationScopeDigest}`}
          />
        </CardContent>
      </Card>

      <Card size="sm">
        <CardHeader className="border-b">
          <CardTitle>固定检查链（当前链尾）</CardTitle>
          <CardDescription>
            仅 isCurrentTail=true 的记录代表当前证据。
          </CardDescription>
        </CardHeader>
        <CardContent className="space-y-2 pt-4">
          <div className="grid gap-2">
            {cutover.checks.map((c) => (
              <CheckRow key={`${c.checkCode}-${c.checkNo}`} check={c} />
            ))}
          </div>
        </CardContent>
      </Card>

      {cutover.supersededChecks.length > 0 ? (
        <Card size="sm">
          <CardHeader className="border-b">
            <CardTitle>历史 / 被替代证据（不可当当前通过）</CardTitle>
          </CardHeader>
          <CardContent className="space-y-2 pt-4">
            {cutover.supersededChecks.map((c) => (
              <CheckRow
                key={`hist-${c.checkCode}-${c.checkNo}`}
                check={c}
                muted
              />
            ))}
          </CardContent>
        </Card>
      ) : null}

      <div className="flex flex-wrap gap-2">
        {canRegister ? (
          <Button
            type="button"
            disabled={formalMutation.isPending}
            onClick={() => void registerT()}
          >
            登记唯一启用时间 T
          </Button>
        ) : cutover.status === "ENABLED" ? (
          <Badge variant="secondary">T 已登记 · 不可改删</Badge>
        ) : (
          <FormalActionResult
            status="blocked"
            title="无法登记 T"
            description={
              cutover.actionBlockers
                .filter((b) => b.action === "ENABLE_CUTOVER")
                .map((b) => b.message)
                .join(" ") || "前提未满足或角色无权"
            }
          />
        )}
        <Button
          type="button"
          variant="outline"
          size="sm"
          disabled={formalMutation.isPending}
          onClick={() => void queryResult()}
        >
          查询切换记录（结果未知时）
        </Button>
      </div>

      {isMobile ? (
        <Alert variant="info">
          <AlertTitle>移动端</AlertTitle>
          <AlertDescription>不提供登记 T 等高风险动作。</AlertDescription>
        </Alert>
      ) : null}
    </div>
  )
}

function CheckRow({
  check,
  muted,
}: {
  check: CutoverCheck
  muted?: boolean
}) {
  return (
    <div
      className={
        muted
          ? "flex flex-wrap items-start gap-2 rounded-lg border border-dashed p-2 text-sm opacity-70"
          : "flex flex-wrap items-start gap-2 rounded-lg border p-2 text-sm"
      }
    >
      <Badge
        variant={
          check.isCurrentTail
            ? check.checkStatus === "PASSED"
              ? "secondary"
              : "destructive"
            : "outline"
        }
      >
        {check.isCurrentTail ? "链尾" : "非当前"} · {check.checkStatus}
      </Badge>
      <div className="min-w-0 flex-1 space-y-0.5">
        <div className="font-medium">
          {check.label}{" "}
          <span className="font-mono text-xs text-muted-foreground">
            {check.checkCode}
          </span>
        </div>
        <div className="text-xs text-muted-foreground">
          #{check.checkNo} · {check.checkedBy} · {formatTime(check.checkedAt)}
        </div>
        <div className="font-mono text-xs">
          subject {check.subjectHash} · {check.evidenceReference}
        </div>
      </div>
    </div>
  )
}
