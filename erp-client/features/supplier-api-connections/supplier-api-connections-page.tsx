"use client"

import * as React from "react"
import Link from "next/link"
import { usePathname, useRouter, useSearchParams } from "next/navigation"
import type { ColumnDef, PaginationState } from "@tanstack/react-table"
import {
  ArrowLeftIcon,
  ExternalLinkIcon,
  KeyRoundIcon,
  PlusIcon,
  RefreshCwIcon,
  SearchIcon,
  ShieldAlertIcon,
  TriangleAlertIcon,
} from "lucide-react"
import { z } from "zod"

import {
  BackgroundJobProgress,
  BatchImpactPreview,
  BusinessEmptyState,
  BusinessFailureState,
  BusinessStatusBadge,
  BusinessTableFrame,
  DataFreshness,
  DataTable,
  DocumentHeader,
  FormalActionResult,
  GuardedBusinessAction,
  ListToolbar,
  MetricFilterItem,
  MetricStrip,
  OptionCombobox,
  SupplierCombobox,
  PageHeader,
  PageScaffold,
  surfaceInsetClassName,
  surfacePanelClassName,
} from "@/components/business"
import { toFieldErrors, useAppForm } from "@/components/form"
import { Field, FieldError, FieldLabel } from "@/components/ui/field"
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
import { Tabs, TabsList, TabsTrigger } from "@/components/ui/tabs"
import {
  useBindCredentialMutation,
  useBindEndpointMutation,
  useConnectionCenterQuery,
  useConnectionListQuery,
  useCreateConnectionMutation,
  useDisableConnectionMutation,
  useEnableConnectionMutation,
  useRunHealthCheckMutation,
  useStartCatalogSyncMutation,
  useUpdateCapabilitiesMutation,
} from "@/features/supplier-api-connections/queries"
import { cn } from "@/lib/utils"
import type {
  CapabilityCode,
  CapabilityView,
  ConnectionCenterView,
  ConnectionListItem,
  ConnectionSection,
  FormalOutcome,
  HealthRecordView,
} from "@/features/supplier-api-connections/types"
import {
  AUDIT_ACTION_LABEL,
  CAPABILITY_LABEL,
  REFERENCE_STATE_LABEL,
  SECTION_LABEL,
  SECTIONS,
} from "@/features/supplier-api-connections/types"
import {
  buildConnectionsSearchParams,
  parseConnectionsSearchParams,
  type ConnectionsUrlState,
} from "@/features/supplier-api-connections/url-state"
import { useSupplierOptionsQuery } from "@/hooks/use-options"
import { freshnessText } from "@/lib/ui-text"
import { formatDateTime } from "@/lib/datetime"
import { type ResultState } from "@/components/business/feedback"

function outcomeToResult(outcome: FormalOutcome): ResultState {
  if (outcome.status === "succeeded") {
    return {
      status: "succeeded",
      title: outcome.title,
      description: outcome.message,
      reference: outcome.reference ?? outcome.auditEventId,
      facts: outcome.facts,
    }
  }
  if (outcome.status === "processing") {
    return {
      status: "processing",
      title: outcome.title,
      description: outcome.message,
      reference: outcome.jobNo,
      jobId: outcome.jobId,
      jobNo: outcome.jobNo,
    }
  }
  if (outcome.status === "unknown") {
    return {
      status: "unknown",
      title: outcome.title,
      description: outcome.message,
    }
  }
  return {
    status: outcome.status,
    title: outcome.title,
    description: outcome.message,
    reference: outcome.reference,
  }
}

function newIdempotencyKey(prefix: string) {
  return `${prefix}_${Date.now()}_${Math.random().toString(36).slice(2, 8)}`
}

export function SupplierApiConnectionsPage() {
  const router = useRouter()
  const pathname = usePathname()
  const searchParams = useSearchParams()

  // Support path-based /connections/:id
  const pathMatch = pathname.match(/\/supplier-api\/connections\/([^/]+)$/)
  const pathConnectionId = pathMatch?.[1]

  const urlState = React.useMemo(() => {
    const parsed = parseConnectionsSearchParams(searchParams)
    if (pathConnectionId && !parsed.connectionId) {
      return { ...parsed, connectionId: pathConnectionId }
    }
    return parsed
  }, [searchParams, pathConnectionId])

  const replaceUrl = React.useCallback(
    (next: ConnectionsUrlState) => {
      // Prefer query-param center on list route for SPA tab identity;
      // path route keeps path when already on [connectionId].
      if (pathConnectionId && next.connectionId === pathConnectionId) {
        const base = `/supplier-api/connections/${pathConnectionId}`
        const params = new URLSearchParams()
        if (next.section !== "overview") params.set("section", next.section)
        const qs = params.toString()
        router.replace(qs ? `${base}?${qs}` : base, { scroll: false })
        return
      }
      const listPath = "/supplier-api/connections"
      const qs = buildConnectionsSearchParams(next)
      router.replace(`${listPath}${qs}`, { scroll: false })
    },
    [pathConnectionId, router]
  )

  const patchUrl = React.useCallback(
    (patch: Partial<ConnectionsUrlState>) => {
      replaceUrl({ ...urlState, ...patch })
    },
    [replaceUrl, urlState]
  )

  if (urlState.connectionId) {
    return (
      <ConnectionCenter
        connectionId={urlState.connectionId}
        urlState={urlState}
        patchUrl={patchUrl}
        onBack={() =>
          patchUrl({ connectionId: undefined, section: "overview" })
        }
      />
    )
  }

  return (
    <ConnectionList
      urlState={urlState}
      patchUrl={patchUrl}
      onOpen={(id) => patchUrl({ connectionId: id, section: "overview" })}
    />
  )
}

function ConnectionList({
  urlState,
  patchUrl,
  onOpen,
}: {
  urlState: ConnectionsUrlState
  patchUrl: (patch: Partial<ConnectionsUrlState>) => void
  onOpen: (connectionId: string) => void
}) {
  const [searchDraft, setSearchDraft] = React.useState(urlState.q ?? "")
  const [createOpen, setCreateOpen] = React.useState(false)
  const [result, setResult] = React.useState<
    (ResultState & { actions?: React.ReactNode }) | null
  >(null)
  const createMutation = useCreateConnectionMutation()
  const { data: supplierOptions } = useSupplierOptionsQuery()

  React.useEffect(() => {
    setSearchDraft(urlState.q ?? "")
  }, [urlState.q])

  const listQuery = useConnectionListQuery({
    environment: urlState.environment,
    status: urlState.status,
    health: urlState.health,
    capability: urlState.capability,
    catalogFreshness: urlState.catalogFreshness,
    supplierId: urlState.supplierId,
    q: urlState.q,
    page: urlState.page,
    pageSize: urlState.pageSize,
  })

  const data = listQuery.data

  // D7：常驻/空态清除 = 清全部筛选参数并回第 1 页；environment 属视图类参数按 P4 保留，
  // 语义通过按钮 title/aria 说明。status/health/catalogFreshness 为逗号分隔多值串
  // （codec array 语义自洽），保持不变。
  const clearFilters = React.useCallback(() => {
    setSearchDraft("")
    patchUrl({
      q: undefined,
      status: undefined,
      health: undefined,
      catalogFreshness: undefined,
      capability: undefined,
      supplierId: undefined,
      page: 1,
    })
  }, [patchUrl])

  const [pagination, setPagination] = React.useState<PaginationState>({
    pageIndex: Math.max(0, urlState.page - 1),
    pageSize: urlState.pageSize,
  })

  React.useEffect(() => {
    setPagination((p) => ({
      ...p,
      pageIndex: Math.max(0, urlState.page - 1),
      pageSize: urlState.pageSize,
    }))
  }, [urlState.page, urlState.pageSize])

  const columns = React.useMemo<ColumnDef<ConnectionListItem>[]>(
    () => [
      {
        id: "identity",
        accessorFn: (row) => row.connectionCode,
        header: "连接身份",
        meta: { label: "连接身份", width: "reference" },
        cell: ({ row }) => {
          const r = row.original
          return (
            <div className="min-w-0 py-0.5">
              <Button
                type="button"
                variant="link"
                size="xs"
                className="num h-auto justify-start px-0 font-medium"
                aria-label={`打开连接 ${r.connectionCode}`}
                onClick={() => onOpen(r.connectionId)}
              >
                {r.connectionCode}
              </Button>
              <div className="truncate text-xs text-muted-foreground">
                {r.supplier.name}
              </div>
            </div>
          )
        },
      },
      {
        id: "environment",
        accessorFn: (row) => row.environmentLabel,
        header: "环境",
        meta: { label: "环境", width: "status" },
        cell: ({ row }) => {
          const env = row.original.environment
          const isProd = env === "PRODUCTION"
          return (
            <span
              className={
                isProd
                  ? "text-sm font-medium text-destructive"
                  : "text-sm text-muted-foreground"
              }
              aria-label={`环境：${row.original.environmentLabel}${
                isProd ? "（生产环境）" : ""
              }`}
            >
              {row.original.environmentLabel}
              {isProd ? (
                <span className="sr-only">生产环境</span>
              ) : null}
            </span>
          )
        },
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
        id: "capabilities",
        accessorFn: (row) => row.capabilitySummary,
        header: "能力摘要",
        meta: { label: "能力摘要" },
        cell: ({ row }) => (
          <div className="max-w-[14rem]">
            <div className="truncate text-sm">
              {row.original.capabilitySummary}
            </div>
            <div className="text-tiny text-muted-foreground">
              连接级 · 非商品级
            </div>
          </div>
        ),
      },
      {
        id: "health",
        accessorFn: (row) => row.healthLabel,
        header: "健康",
        meta: { label: "健康", width: "status" },
        cell: ({ row }) => (
          <div className="space-y-0.5">
            <BusinessStatusBadge
              context="list"
              label={row.original.healthLabel}
              tone={row.original.healthTone}
            />
            <div className="text-tiny text-muted-foreground">
              {formatDateTime(row.original.lastHealthAt, "default")}
            </div>
          </div>
        ),
      },
      {
        id: "catalog",
        accessorFn: (row) => row.catalogLabel,
        header: freshnessText.catalogSyncAt,
        meta: { label: freshnessText.catalogSyncAt },
        cell: ({ row }) => (
          <span className="text-sm">{row.original.catalogLabel}</span>
        ),
      },
      {
        id: "nextStep",
        accessorFn: (row) => row.nextStep,
        header: "下一步",
        meta: { label: "下一步" },
        cell: ({ row }) => (
          <span className="line-clamp-2 text-sm text-muted-foreground">
            {row.original.nextStep}
          </span>
        ),
      },
      {
        id: "owners",
        accessorFn: (row) =>
          `${row.businessOwner ?? "—"} / ${row.technicalOwner ?? "—"}`,
        header: "业务/技术",
        meta: { label: "业务/技术负责人" },
        cell: ({ row }) => (
          <span className="text-xs text-muted-foreground">
            {row.original.businessOwner ?? "—"} /{" "}
            {row.original.technicalOwner ?? "—"}
          </span>
        ),
      },
      {
        id: "actions",
        header: "操作",
        meta: { label: "操作", width: "status" },
        enableSorting: false,
        cell: ({ row }) => (
          <Button
            type="button"
            size="sm"
            variant="outline"
            onClick={() => onOpen(row.original.connectionId)}
          >
            打开
          </Button>
        ),
      },
    ],
    [onOpen]
  )

  const createSchema = z.object({
    connectionCode: z.string().trim().min(3, "请填写连接代码"),
    supplierId: z.string().trim().min(1, "请选择供应商"),
    supplierName: z.string().trim().min(2, "请选择供应商"),
    environment: z.enum(["DEVELOPMENT", "STAGING", "PRODUCTION"]),
  })

  const form = useAppForm({
    defaultValues: {
      connectionCode: "",
      supplierId: "",
      supplierName: "",
      environment: "PRODUCTION" as "DEVELOPMENT" | "STAGING" | "PRODUCTION",
    },
    validators: { onChange: createSchema },
    onSubmit: async ({ value }) => {
      const outcome = await createMutation.mutateAsync({
        connectionCode: value.connectionCode,
        supplierId: value.supplierId,
        supplierName: value.supplierName,
        environment: value.environment,
        idempotencyKey: newIdempotencyKey("create"),
      })
      const mapped = outcomeToResult(outcome)
      if (outcome.status === "succeeded" && outcome.connectionId) {
        setCreateOpen(false)
        form.reset()
        setResult(
          mapped
            ? {
                ...mapped,
                actions: (
                  <Button
                    type="button"
                    size="sm"
                    onClick={() => onOpen(outcome.connectionId!)}
                  >
                    打开连接详情
                  </Button>
                ),
              }
            : mapped
        )
      } else {
        setResult(mapped)
      }
    },
  })

  if (listQuery.isPending) {
    return (
      <PageScaffold density="compact">
        <div className="h-10 w-56 animate-pulse rounded-lg bg-muted" />
        <div className="h-16 animate-pulse rounded-lg bg-muted" />
        <div className="h-72 animate-pulse rounded-lg bg-muted" />
      </PageScaffold>
    )
  }

  if (listQuery.isError) {
    return (
      <PageScaffold density="compact">
        <PageHeader title="API 供应商连接" description="加载失败" />
        <BusinessFailureState
          kind="system"
          title="连接列表加载失败"
          description="请重试。"
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
    <PageScaffold density="compact">
      <PageHeader
        title="API 供应商连接"
        breadcrumbs={[
          {
            id: "api",
            label: "供应商 API",
            href: "/supplier-api/connections",
          },
          { id: "conn", label: "API 连接", current: true },
        ]}
        metadata={
          <DataFreshness
            updatedAt={
              data?.projectedAt
                ? formatDateTime(data.projectedAt, "default")
                : "—"
            }
            dateTime={data?.projectedAt}
            state={listQuery.isFetching ? "syncing" : "fresh"}
            label="连接列表"
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
                disabled={!data?.hasModulePermission}
                reason={
                  data?.hasModulePermission
                    ? undefined
                    : "当前账号无模块权限"
                }
                onClick={() => setCreateOpen(true)}
              >
                <PlusIcon className="size-3.5" aria-hidden="true" />
                新建连接
              </GuardedBusinessAction>
            </div>
          </div>
        }
      />

      {result ? (
        <FormalActionResult
          status={
            result.status === "failed"
              ? "rejected"
              : result.status === "processing"
                ? "processing"
                : result.status
          }
          title={result.title}
          description={result.description}
          reference={result.reference}
          facts={result.facts}
          actions={result.actions}
        />
      ) : null}

      {/* D7：空态不再隐藏筛选区——MetricStrip 与 ListToolbar 常驻，仅表格区切换空态 */}
      <MetricStrip columns={5} aria-label="连接指标筛选">
        <MetricFilterItem
          label="已启用"
          value={data?.metrics.enabled ?? 0}
          active={urlState.status === "ENABLED"}
          onClick={() =>
            patchUrl({
              status: urlState.status === "ENABLED" ? undefined : "ENABLED",
              page: 1,
            })
          }
        />
        <MetricFilterItem
          label="故障"
          value={data?.metrics.faulted ?? 0}
          active={urlState.status === "FAULTED"}
          onClick={() =>
            patchUrl({
              status: urlState.status === "FAULTED" ? undefined : "FAULTED",
              page: 1,
            })
          }
        />
        <MetricFilterItem
          label="待配置"
          value={data?.metrics.pendingConfig ?? 0}
          active={urlState.status === "PENDING_CONFIG"}
          onClick={() =>
            patchUrl({
              status:
                urlState.status === "PENDING_CONFIG"
                  ? undefined
                  : "PENDING_CONFIG",
              page: 1,
            })
          }
        />
        <MetricFilterItem
          label="健康异常"
          value={data?.metrics.healthAbnormal ?? 0}
          active={Boolean(urlState.health)}
          onClick={() =>
            patchUrl({
              health: urlState.health
                ? undefined
                : "FAILED,AUTH_FAILED,PARTIAL,UNKNOWN",
              page: 1,
            })
          }
        />
        <MetricFilterItem
          label="目录陈旧"
          value={data?.metrics.catalogStale ?? 0}
          active={Boolean(urlState.catalogFreshness)}
          onClick={() =>
            patchUrl({
              catalogFreshness: urlState.catalogFreshness
                ? undefined
                : "STALE,FAILED",
              page: 1,
            })
          }
        />
      </MetricStrip>

      <BusinessTableFrame
        title="连接列表"
        description="一行展示代码、供应商、环境、状态、能力、健康与下一步；身份与操作列固定；默认仅展示生产环境连接，可在工具栏切换。"
        toolbar={
          <ListToolbar
            search={
              <InputGroup className="max-w-md">
                <InputGroupAddon>
                  <SearchIcon className="size-4" aria-hidden="true" />
                </InputGroupAddon>
                <InputGroupInput
                  placeholder="连接代码、供应商名称"
                  value={searchDraft}
                  onChange={(e) => setSearchDraft(e.target.value)}
                  onKeyDown={(e) => {
                    if (e.key === "Enter") {
                      patchUrl({ q: searchDraft.trim() || undefined, page: 1 })
                    }
                  }}
                  aria-label="搜索连接"
                />
              </InputGroup>
            }
            filters={
              <>
                <OptionCombobox
                  value={urlState.environment}
                  onValueChange={(v) => {
                    if (v == null) return
                    patchUrl({
                      environment: v as ConnectionsUrlState["environment"],
                      page: 1,
                    })
                  }}
                  options={[
                    { value: "ALL", label: "全部环境" },
                    { value: "PRODUCTION", label: "生产" },
                    { value: "STAGING", label: "测试" },
                    { value: "DEVELOPMENT", label: "开发" },
                  ]}
                  className="w-[7.5rem]"
                  size="sm"
                  placeholder="环境"
                  allowClear={false}
                  aria-label="环境"
                />
                <OptionCombobox
                  value={urlState.status ?? "default"}
                  onValueChange={(v) => {
                    if (v == null || v === "default") {
                      patchUrl({ status: undefined, page: 1 })
                    } else if (v === "all") {
                      patchUrl({
                        status: "ENABLED,DISABLED,FAULTED,PENDING_CONFIG",
                        page: 1,
                      })
                    } else {
                      patchUrl({ status: v, page: 1 })
                    }
                  }}
                  options={[
                    { value: "default", label: "启用+故障+待配置" },
                    { value: "all", label: "全部状态" },
                    { value: "ENABLED", label: "启用" },
                    { value: "FAULTED", label: "故障" },
                    { value: "DISABLED", label: "停用" },
                    { value: "PENDING_CONFIG", label: "待配置" },
                  ]}
                  className="w-[8rem]"
                  size="sm"
                  placeholder="状态"
                  allowClear={false}
                  aria-label="连接状态"
                />
                <SupplierCombobox
                  value={urlState.supplierId || undefined}
                  onValueChange={(id) =>
                    patchUrl({
                      supplierId: id || undefined,
                      page: 1,
                    })
                  }
                  suppliers={supplierOptions ?? []}
                  className="w-[12rem]"
                  placeholder="全部供应商"
                  aria-label="供应商"
                />
              </>
            }
            secondary={
              <OptionCombobox
                value={urlState.capability ?? ""}
                onValueChange={(v) =>
                  patchUrl({
                    capability: v || undefined,
                    page: 1,
                  })
                }
                options={[
                  { value: "", label: "全部能力" },
                  ...(
                    Object.keys(CAPABILITY_LABEL) as Array<
                      keyof typeof CAPABILITY_LABEL
                    >
                  ).map((k) => ({
                    value: k,
                    label: CAPABILITY_LABEL[k],
                  })),
                ]}
                className="w-[8rem]"
                size="sm"
                placeholder="能力"
                allowClear={false}
                aria-label="能力"
              />
            }
            actions={
              <Button
                type="button"
                size="sm"
                variant="ghost"
                onClick={clearFilters}
                title="清除筛选，保留当前环境"
                aria-label="清除筛选（保留当前环境）"
              >
                清除筛选
              </Button>
            }
          />
        }
        table={
          empty === "FILTER_NO_RESULT" ? (
            <BusinessEmptyState
              kind="filter"
              className="rounded-lg border-0 bg-transparent p-6 shadow-none ring-0"
              title="当前筛选无结果"
              description="没有连接符合当前环境/状态/能力/健康条件，可清除筛选。"
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
          ) : empty === "NO_CONNECTIONS" ? (
            <BusinessEmptyState
              kind="no-data"
              className="rounded-lg border-0 bg-transparent p-6 shadow-none ring-0"
              title="尚未接入供应商连接"
              description="当前环境还没有连接身份。有权限时可新建连接。"
              action={
                data?.hasModulePermission ? (
                  <Button
                    type="button"
                    onClick={() => setCreateOpen(true)}
                  >
                    新建连接
                  </Button>
                ) : null
              }
            />
          ) : (
            <DataTable
              data={data?.items ?? []}
              columns={columns}
              getRowId={(row) => row.connectionId}
              rowCount={data?.total ?? 0}
              rowLabel={(row) => row.connectionCode}
              caption="API 供应商连接列表"
              density="compact"
              layout="flush"
              enableColumnPinning
              defaultColumnVisibility={{ owners: false }}
              defaultColumnPinning={{
                left: ["identity"],
                right: ["actions"],
              }}
              pagination={pagination}
              onPaginationChange={(next) => {
                setPagination(next)
                patchUrl({
                  page: next.pageIndex + 1,
                  pageSize: next.pageSize,
                })
              }}
              onRowOpen={(row) => onOpen(row.connectionId)}
            />
          )
        }
      />

      <Dialog open={createOpen} onOpenChange={setCreateOpen}>
        <DialogContent>
          <DialogHeader>
            <DialogTitle>新建连接身份</DialogTitle>
            <DialogDescription>
              连接代码全局唯一，不可与环境组合复用。创建成功后可在结果中打开连接详情完成配置。
            </DialogDescription>
          </DialogHeader>
          <form
            className="flex flex-col gap-3"
            onSubmit={(e) => {
              e.preventDefault()
              void form.handleSubmit()
            }}
          >
            <form.AppField
              name="connectionCode"
              children={(field) => (
                <field.TextField label="连接代码" placeholder="CONN-XXX-PROD" />
              )}
            />
            <form.AppField
              name="supplierId"
              children={(field) => {
                const isInvalid =
                  field.state.meta.isTouched && !field.state.meta.isValid
                const errors = toFieldErrors(field.state.meta.errors)
                return (
                  <Field data-invalid={isInvalid || undefined}>
                    <FieldLabel htmlFor="create-supplierId">供应商</FieldLabel>
                    <SupplierCombobox
                      value={field.state.value || undefined}
                      onValueChange={(id) => {
                        const next = id ?? ""
                        field.handleChange(next)
                        const supplier = supplierOptions?.find(
                          (s) => s.supplierId === next
                        )
                        form.setFieldValue(
                          "supplierName",
                          supplier?.supplierName ?? ""
                        )
                      }}
                      suppliers={supplierOptions ?? []}
                      placeholder="搜索供应商名称或编码"
                    />
                    {isInvalid ? <FieldError errors={errors} /> : null}
                  </Field>
                )
              }}
            />
            <form.AppField
              name="environment"
              children={(field) => (
                <div className="space-y-1.5">
                  <Label>环境</Label>
                  <OptionCombobox
                    value={field.state.value}
                    onValueChange={(v) => {
                      if (v) field.handleChange(v as typeof field.state.value)
                    }}
                    options={[
                      { value: "PRODUCTION", label: "生产" },
                      { value: "STAGING", label: "测试" },
                      { value: "DEVELOPMENT", label: "开发" },
                    ]}
                    allowClear={false}
                  />
                  {field.state.value === "PRODUCTION" ? (
                    <p className="text-xs text-muted-foreground" role="status">
                      正在创建生产环境连接身份
                    </p>
                  ) : null}
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
                <form.SubmitButton label="创建" />
              </form.AppForm>
            </DialogFooter>
          </form>
        </DialogContent>
      </Dialog>
    </PageScaffold>
  )
}

function ConnectionCenter({
  connectionId,
  urlState,
  patchUrl,
  onBack,
}: {
  connectionId: string
  urlState: ConnectionsUrlState
  patchUrl: (patch: Partial<ConnectionsUrlState>) => void
  onBack: () => void
}) {
  const centerQuery = useConnectionCenterQuery(connectionId)
  const [result, setResult] = React.useState<ResultState>(null)
  const [disableOpen, setDisableOpen] = React.useState(false)
  const [credOpen, setCredOpen] = React.useState(false)
  const [endpointOpen, setEndpointOpen] = React.useState(false)
  const [selectedRef, setSelectedRef] = React.useState<string>("")
  const [selectedEndpointRef, setSelectedEndpointRef] =
    React.useState<string>("")
  const [confirmHealthOpen, setConfirmHealthOpen] = React.useState(false)
  const [confirmEnableOpen, setConfirmEnableOpen] = React.useState(false)
  const [capConfigOpen, setCapConfigOpen] = React.useState(false)

  const bindCred = useBindCredentialMutation()
  const bindEndpoint = useBindEndpointMutation()
  const updateCaps = useUpdateCapabilitiesMutation()
  const runHealth = useRunHealthCheckMutation()
  const startCatalog = useStartCatalogSyncMutation()
  const disableMut = useDisableConnectionMutation()
  const enableMut = useEnableConnectionMutation()
  const listQuery = useConnectionListQuery({
    environment: "ALL",
    page: 1,
  })

  const conn = centerQuery.data
  const section = urlState.section

  const applyOutcome = (outcome: FormalOutcome) => {
    setResult(outcomeToResult(outcome))
  }

  if (centerQuery.isPending) {
    return (
      <PageScaffold>
        <div className="h-10 w-40 animate-pulse rounded-lg bg-muted" />
        <div className="h-24 animate-pulse rounded-lg bg-muted" />
        <div className="h-64 animate-pulse rounded-lg bg-muted" />
      </PageScaffold>
    )
  }

  if (centerQuery.isError) {
    return (
      <PageScaffold>
        <BusinessFailureState
          kind="system"
          title="连接详情加载失败"
          description="请重试"
          action={
            <Button type="button" onClick={() => void centerQuery.refetch()}>
              重试
            </Button>
          }
        />
      </PageScaffold>
    )
  }

  if (!conn) {
    return (
      <PageScaffold>
        <Button type="button" variant="ghost" size="sm" onClick={onBack}>
          <ArrowLeftIcon className="size-4" aria-hidden="true" />
          返回列表
        </Button>
        <BusinessEmptyState
          kind="no-data"
          className="rounded-lg border-0 bg-transparent p-6 shadow-none ring-0"
          title="未找到连接"
          description="该连接不存在或当前角色无权查看。可返回列表重新选择。"
        />
      </PageScaffold>
    )
  }

  const isProd = conn.environment === "PRODUCTION"
  const authFailed = conn.lastHealth?.result === "AUTH_FAILED"
  const resultUnknown = conn.lastHealth?.result === "UNKNOWN"

  return (
    <PageScaffold>
      <PageHeader
        variant="object-chrome"
        breadcrumbs={[
          {
            id: "api",
            label: "供应商 API",
            href: "/supplier-api/connections",
          },
          { id: "conn", label: "API 连接", href: "/supplier-api/connections" },
          {
            id: "detail",
            label: conn.connectionCode,
            current: true,
          },
        ]}
        actions={
          <Button type="button" variant="outline" size="sm" onClick={onBack}>
            <ArrowLeftIcon className="size-4" aria-hidden="true" />
            返回列表
          </Button>
        }
      />

      <DocumentHeader
        density="compact"
        title={`${conn.connectionCode} · ${conn.supplier.name}`}
        documentNumber={conn.connectionCode}
        primaryStatus={{ label: conn.statusLabel, tone: conn.statusTone }}
        version={conn.version}
        meta={
          <span className="inline-flex flex-wrap items-center gap-x-1.5 gap-y-0.5">
            <span>
              业务{" "}
              <span className="font-medium text-foreground">
                {conn.businessOwner?.label ?? "—"}
              </span>
            </span>
            <span className="text-border" aria-hidden="true">
              ·
            </span>
            <span>
              技术{" "}
              <span className="font-medium text-foreground">
                {conn.technicalOwner?.label ?? "—"}
              </span>
            </span>
            <span className="text-border" aria-hidden="true">
              ·
            </span>
            <span className="text-muted-foreground">
              配置 {formatDateTime(conn.updatedAt, "default")}
            </span>
          </span>
        }
        statuses={[
          {
            id: "env",
            label: "环境",
            status: {
              label: conn.environmentLabel,
              tone: isProd ? "destructive" : "neutral",
            },
          },
          {
            id: "health",
            label: "最近健康",
            status: {
              label: conn.lastHealth?.resultLabel ?? "未检查",
              tone:
                conn.lastHealth?.result === "SUCCESS"
                  ? "success"
                  : conn.lastHealth?.result === "AUTH_FAILED" ||
                      conn.lastHealth?.result === "FAILED"
                    ? "destructive"
                    : "warning",
            },
          },
        ]}
        primaryAction={
          <div className="flex flex-wrap gap-2">
            <Button
              type="button"
              size="sm"
              variant="outline"
              disabled={runHealth.isPending}
              onClick={() => setConfirmHealthOpen(true)}
            >
              <RefreshCwIcon className="size-4" aria-hidden="true" />
              健康检查
            </Button>
            {conn.status !== "ENABLED" ? (
              <Button
                type="button"
                size="sm"
                disabled={enableMut.isPending}
                onClick={() => setConfirmEnableOpen(true)}
              >
                启用连接
              </Button>
            ) : null}
            {conn.status === "ENABLED" ? (
              <Button
                type="button"
                size="sm"
                variant="destructive"
                onClick={() => setDisableOpen(true)}
              >
                停用连接
              </Button>
            ) : null}
          </div>
        }
      />

      {isProd ? (
        <Alert variant="warning" role="status">
          <TriangleAlertIcon aria-hidden="true" />
          <AlertTitle>生产环境</AlertTitle>
          <AlertDescription>
            当前连接运行在生产环境。启停、密钥轮换与全能力检查均需二次确认；检查不会创建真实业务订单。
          </AlertDescription>
        </Alert>
      ) : null}

      {conn.alerts.map((al) => (
        <Alert
          key={al.id}
          variant={
            al.severity === "destructive"
              ? "destructive"
              : al.severity === "warning"
                ? "warning"
                : "default"
          }
          role="alert"
        >
          <ShieldAlertIcon aria-hidden="true" />
          <AlertTitle>{al.title}</AlertTitle>
          <AlertDescription>{al.description}</AlertDescription>
        </Alert>
      ))}

      {authFailed && !conn.alerts.some((a) => a.title.includes("鉴权")) ? (
        <Alert variant="destructive" role="alert">
          <ShieldAlertIcon aria-hidden="true" />
          <AlertTitle>鉴权/签名失败 · 自动重试已停止</AlertTitle>
          <AlertDescription>
            {conn.lastHealth?.errorSummary ??
              "高风险故障。请运维检查密钥引用与适配器；本页不展示密钥正文。"}
          </AlertDescription>
        </Alert>
      ) : null}

      {resultUnknown ? (
        <Alert variant="warning" role="status" aria-live="polite">
          <TriangleAlertIcon aria-hidden="true" />
          <AlertTitle>处理结果待确认</AlertTitle>
          <AlertDescription>
            不得按成功或失败处理，不乐观改变启停或引用状态。请按原任务号查询最终结论。
          </AlertDescription>
        </Alert>
      ) : null}

      {result ? (
        <div className="space-y-2">
          <FormalActionResult
            status={
              result.status === "failed"
                ? "rejected"
                : result.status === "processing"
                  ? "processing"
                  : result.status
            }
            title={result.title}
            description={result.description}
            reference={result.reference}
            facts={result.facts}
          />
        </div>
      ) : null}

      <div className={cn(surfacePanelClassName, "min-w-0 overflow-hidden")}>
      <Tabs
        value={section}
        onValueChange={(v) => {
          if (v) patchUrl({ section: v as ConnectionSection })
        }}
      >
        <TabsList
          variant="line"
          className="sticky top-0 z-10 h-auto w-full flex-wrap justify-start gap-1 overflow-x-auto rounded-none border-b border-border/30 bg-card/95 px-3 py-1.5 backdrop-blur supports-backdrop-filter:bg-card/80"
        >
          {SECTIONS.map((s) => (
            <TabsTrigger key={s} value={s} className="text-xs sm:text-sm">
              {SECTION_LABEL[s]}
            </TabsTrigger>
          ))}
        </TabsList>
      </Tabs>

      <div className="space-y-4 p-3 md:p-4">
      {section === "overview" ? (
        <OverviewSection conn={conn} />
      ) : null}
      {section === "capabilities" ? (
        <CapabilitiesSection
          conn={conn}
          onOpenConfig={() => setCapConfigOpen(true)}
        />
      ) : null}
      {section === "security" ? (
        <SecuritySection
          conn={conn}
          onBind={() => {
            setSelectedRef("")
            setCredOpen(true)
          }}
          onBindEndpoint={() => {
            setSelectedEndpointRef("")
            setEndpointOpen(true)
          }}
        />
      ) : null}
      {section === "health" ? (
        <HealthSection records={conn.healthRecords} last={conn.lastHealth} />
      ) : null}
      {section === "catalog" ? (
        <CatalogSection
          conn={conn}
          syncing={startCatalog.isPending}
          onSync={async () => {
            const outcome = await startCatalog.mutateAsync({
              connectionId: conn.connectionId,
              idempotencyKey: newIdempotencyKey("catalog"),
            })
            applyOutcome(outcome)
          }}
        />
      ) : null}
      {section === "related" ? <RelatedSection conn={conn} /> : null}
      {section === "audit" ? <AuditSection conn={conn} /> : null}
      </div>
      </div>

      {/* 停用影响预览 */}
      <Dialog open={disableOpen} onOpenChange={setDisableOpen}>
        <DialogContent className="sm:max-w-lg">
          <DialogHeader>
            <DialogTitle>
              {isProd ? "停用生产环境连接" : "停用连接"}
            </DialogTitle>
            <DialogDescription>
              停用改变治理状态，不删除连接、版本和历史业务记录。
            </DialogDescription>
          </DialogHeader>
          <BatchImpactPreview
            title="停用影响预览"
            description="请核对发布、待处理订单与同步任务影响。"
            filterSummary={`${conn.connectionCode} · ${conn.environmentLabel}`}
            selectionScope={`${conn.supplier.name} · 单一连接`}
            estimated={
              conn.relatedImpact.activePublications +
              conn.relatedImpact.openSupplierOrders +
              conn.relatedImpact.activeSyncJobs
            }
            estimatedLabel="受影响发布/订单/任务"
            processable={1}
            processableLabel="连接"
            skipped={0}
            background={false}
            sensitiveFields={["密钥配置", "签名材料"]}
            skippedReason={undefined}
          />
          <dl className="grid gap-2 text-sm sm:grid-cols-3">
            <div className="rounded-lg border p-3">
              <dt className="text-xs text-muted-foreground">生效发布</dt>
              <dd className="num font-medium">
                {conn.relatedImpact.activePublications}
              </dd>
            </div>
            <div className="rounded-lg border p-3">
              <dt className="text-xs text-muted-foreground">待处理订单</dt>
              <dd className="num font-medium">
                {conn.relatedImpact.openSupplierOrders}
              </dd>
            </div>
            <div className="rounded-lg border p-3">
              <dt className="text-xs text-muted-foreground">同步任务</dt>
              <dd className="num font-medium">
                {conn.relatedImpact.activeSyncJobs}
              </dd>
            </div>
          </dl>
          <div className="space-y-1 text-xs text-muted-foreground">
            <p>历史版本与业务记录保留，不会删除任何数据。</p>
            <p className="flex flex-wrap items-center gap-x-3">
              替代方案：
              <Link
                href="/procurement/supplier-catalog"
                className="text-primary underline-offset-2 hover:underline"
              >
                供应商商品库
              </Link>
              <Link
                href="/supplier-api/orders"
                className="text-primary underline-offset-2 hover:underline"
              >
                供应商订单
              </Link>
              <Link
                href="/governance/integration-errors"
                className="text-primary underline-offset-2 hover:underline"
              >
                接口错误中心
              </Link>
            </p>
          </div>
          <DialogFooter>
            <Button
              type="button"
              variant="outline"
              onClick={() => setDisableOpen(false)}
            >
              取消
            </Button>
            <Button
              type="button"
              variant="destructive"
              disabled={disableMut.isPending}
              onClick={async () => {
                const outcome = await disableMut.mutateAsync({
                  connectionId: conn.connectionId,
                  expectedVersion: conn.version,
                  reasonCode: "ADMIN_DISABLE",
                  idempotencyKey: newIdempotencyKey("disable"),
                })
                applyOutcome(outcome)
                setDisableOpen(false)
              }}
            >
              确认停用
            </Button>
          </DialogFooter>
        </DialogContent>
      </Dialog>

      {/* 密钥引用选择器 — 仅不透明引用 */}
      <Dialog open={credOpen} onOpenChange={setCredOpen}>
        <DialogContent>
          <DialogHeader>
            <DialogTitle>
              {isProd ? "轮换生产环境密钥引用" : "绑定/轮换密钥引用"}
            </DialogTitle>
            <DialogDescription>
              只能从密钥管理系统选择不透明引用。无明文密钥输入框；页面、URL
              与结果均不返回正文。
            </DialogDescription>
          </DialogHeader>
          <div className="space-y-3">
            {listQuery.isError ? (
              <Alert variant="destructive" role="alert">
                <AlertTitle>引用选项加载失败</AlertTitle>
                <AlertDescription>
                  无法取得密钥管理引用列表，请重试后再选择。
                </AlertDescription>
              </Alert>
            ) : null}
            <Label htmlFor="opaque-ref">密钥管理引用</Label>
            <OptionCombobox
              id="opaque-ref"
              value={selectedRef || null}
              onValueChange={(v) => {
                if (v) setSelectedRef(v)
              }}
              options={(listQuery.data?.credentialOpaqueOptions ?? []).map(
                (o) => ({
                  value: o.referenceId,
                  label: `${o.alias} · ${o.version}`,
                })
              )}
              placeholder="选择不透明引用"
              allowClear={false}
            />
            <p className="text-xs text-muted-foreground">
              当前状态：
              {REFERENCE_STATE_LABEL[conn.safeReferences.credential.state]}
              {conn.safeReferences.credential.alias
                ? ` · ${conn.safeReferences.credential.alias}`
                : ""}
            </p>
          </div>
          <DialogFooter>
            <Button
              type="button"
              variant="outline"
              onClick={() => setCredOpen(false)}
            >
              取消
            </Button>
            <Button
              type="button"
              disabled={!selectedRef || bindCred.isPending}
              onClick={async () => {
                const outcome = await bindCred.mutateAsync({
                  connectionId: conn.connectionId,
                  opaqueReferenceId: selectedRef,
                  expectedVersion: conn.version,
                  idempotencyKey: newIdempotencyKey("cred"),
                })
                applyOutcome(outcome)
                if (outcome.status === "succeeded") setCredOpen(false)
              }}
            >
              <KeyRoundIcon className="size-4" aria-hidden="true" />
              确认绑定引用
            </Button>
          </DialogFooter>
        </DialogContent>
      </Dialog>

      {/* 地址配置引用选择器 — 仅不透明引用 */}
      <Dialog open={endpointOpen} onOpenChange={setEndpointOpen}>
        <DialogContent>
          <DialogHeader>
            <DialogTitle>
              {isProd ? "轮换生产环境地址引用" : "绑定/轮换地址引用"}
            </DialogTitle>
            <DialogDescription>
              只能从系统提供的地址配置引用中选择，不能自由输入地址。
            </DialogDescription>
          </DialogHeader>
          <div className="space-y-3">
            {listQuery.isError ? (
              <Alert variant="destructive" role="alert">
                <AlertTitle>引用选项加载失败</AlertTitle>
                <AlertDescription>
                  无法取得地址配置引用列表，请重试后再选择。
                </AlertDescription>
              </Alert>
            ) : null}
            <Label htmlFor="endpoint-ref">地址配置引用</Label>
            <OptionCombobox
              id="endpoint-ref"
              value={selectedEndpointRef || null}
              onValueChange={(v) => {
                if (v) setSelectedEndpointRef(v)
              }}
              options={(listQuery.data?.endpointOpaqueOptions ?? []).map(
                (o) => ({
                  value: o.referenceId,
                  label: `${o.alias} · ${o.version}`,
                })
              )}
              placeholder="选择地址配置引用"
              allowClear={false}
            />
            <p className="text-xs text-muted-foreground">
              当前状态：
              {REFERENCE_STATE_LABEL[conn.safeReferences.endpoint.state]}
              {conn.safeReferences.endpoint.alias
                ? ` · ${conn.safeReferences.endpoint.alias}`
                : ""}
            </p>
          </div>
          <DialogFooter>
            <Button
              type="button"
              variant="outline"
              onClick={() => setEndpointOpen(false)}
            >
              取消
            </Button>
            <Button
              type="button"
              disabled={!selectedEndpointRef || bindEndpoint.isPending}
              onClick={async () => {
                const outcome = await bindEndpoint.mutateAsync({
                  connectionId: conn.connectionId,
                  opaqueReferenceId: selectedEndpointRef,
                  expectedVersion: conn.version,
                  idempotencyKey: newIdempotencyKey("endpoint"),
                })
                applyOutcome(outcome)
                if (outcome.status === "succeeded") setEndpointOpen(false)
              }}
            >
              <KeyRoundIcon className="size-4" aria-hidden="true" />
              确认绑定地址
            </Button>
          </DialogFooter>
        </DialogContent>
      </Dialog>

      {/* 健康检查确认（生产环境二次确认） */}
      <Dialog open={confirmHealthOpen} onOpenChange={setConfirmHealthOpen}>
        <DialogContent className="sm:max-w-md">
          <DialogHeader>
            <DialogTitle>执行健康检查</DialogTitle>
            <DialogDescription>
              将对全能力执行健康检查并记录结果。
              {isProd
                ? "生产环境检查不会创建真实业务订单。"
                : "结果可随时在本页健康记录中查看。"}
            </DialogDescription>
          </DialogHeader>
          <DialogFooter>
            <Button
              type="button"
              variant="outline"
              onClick={() => setConfirmHealthOpen(false)}
            >
              取消
            </Button>
            <Button
              type="button"
              disabled={runHealth.isPending}
              onClick={async () => {
                const outcome = await runHealth.mutateAsync({
                  connectionId: conn.connectionId,
                  expectedVersion: conn.version,
                  idempotencyKey: newIdempotencyKey("health"),
                })
                applyOutcome(outcome)
                setConfirmHealthOpen(false)
              }}
            >
              <RefreshCwIcon className="size-4" aria-hidden="true" />
              确认执行
            </Button>
          </DialogFooter>
        </DialogContent>
      </Dialog>

      {/* 启用连接确认（生产环境二次确认） */}
      <Dialog open={confirmEnableOpen} onOpenChange={setConfirmEnableOpen}>
        <DialogContent className="sm:max-w-md">
          <DialogHeader>
            <DialogTitle>
              {isProd ? "启用生产环境连接" : "启用连接"}
            </DialogTitle>
            <DialogDescription>
              启用后连接将恢复对外接口可用，后续下单、查询等业务请求将按能力声明放行。
              {isProd ? " 生产环境操作需谨慎核对。" : ""}
            </DialogDescription>
          </DialogHeader>
          <DialogFooter>
            <Button
              type="button"
              variant="outline"
              onClick={() => setConfirmEnableOpen(false)}
            >
              取消
            </Button>
            <Button
              type="button"
              disabled={enableMut.isPending}
              onClick={async () => {
                const outcome = await enableMut.mutateAsync({
                  connectionId: conn.connectionId,
                  expectedVersion: conn.version,
                  idempotencyKey: newIdempotencyKey("enable"),
                })
                applyOutcome(outcome)
                setConfirmEnableOpen(false)
              }}
            >
              确认启用
            </Button>
          </DialogFooter>
        </DialogContent>
      </Dialog>

      {/* 管理员能力配置 */}
      <CapConfigDialog
        open={capConfigOpen}
        onOpenChange={setCapConfigOpen}
        conn={conn}
        pending={updateCaps.isPending}
        onSubmit={async (changes) => {
          const expectedCapabilityVersions: Record<string, string> = {}
          for (const c of conn.capabilities) {
            expectedCapabilityVersions[c.capabilityCode] = c.version
          }
          const outcome = await updateCaps.mutateAsync({
            connectionId: conn.connectionId,
            changes,
            expectedConnectionVersion: conn.version,
            expectedCapabilityVersions,
            reasonCode: "ADMIN_CONFIG",
            operationId: newIdempotencyKey("op_cap"),
            idempotencyKey: newIdempotencyKey("cap"),
          })
          applyOutcome(outcome)
          if (outcome.status === "succeeded") setCapConfigOpen(false)
        }}
      />
    </PageScaffold>
  )
}

function OverviewSection({ conn }: { conn: ConnectionCenterView }) {
  return (
    <div className="grid gap-3 lg:grid-cols-2">
      <Card size="sm" className={cn(surfaceInsetClassName, "shadow-none ring-0")}>
        <CardHeader className="rounded-t-lg border-b border-border/30 pb-2">
          <CardTitle className="text-base">业务身份</CardTitle>
          <CardDescription>采购主责供应商与业务影响</CardDescription>
        </CardHeader>
        <CardContent className="grid gap-2 text-sm">
          <Row label="连接代码" value={conn.connectionCode} mono />
          <Row label="供应商" value={conn.supplier.name} />
          <Row
            label="环境"
            value={
              <span
                className={
                  conn.environment === "PRODUCTION"
                    ? "font-medium text-destructive"
                    : undefined
                }
              >
                {conn.environmentLabel}
                {conn.environment === "PRODUCTION" ? "（生产）" : ""}
              </span>
            }
          />
          <Row label="业务负责人" value={conn.businessOwner?.label ?? "—"} />
          <Row label="下一步" value={conn.nextStep} />
        </CardContent>
      </Card>
      <Card size="sm" className={cn(surfaceInsetClassName, "shadow-none ring-0")}>
        <CardHeader className="rounded-t-lg border-b border-border/30 pb-2">
          <CardTitle className="text-base">技术就绪</CardTitle>
          <CardDescription>地址/密钥引用与适配器</CardDescription>
        </CardHeader>
        <CardContent className="grid gap-2 text-sm">
          <Row
            label="地址配置"
            value={
              <RefLabel
                state={conn.safeReferences.endpoint.state}
                alias={conn.safeReferences.endpoint.alias}
                version={conn.safeReferences.endpoint.version}
                visible={conn.safeReferences.endpoint.visible}
              />
            }
          />
          <Row
            label="密钥配置"
            value={
              <RefLabel
                state={conn.safeReferences.credential.state}
                alias={conn.safeReferences.credential.alias}
                version={conn.safeReferences.credential.version}
                visible={conn.safeReferences.credential.visible}
              />
            }
          />
          {conn.adapter?.visible ? (
            <Row
              label="适配器"
              value={`${conn.adapter.code} @ ${conn.adapter.version}`}
              mono
            />
          ) : (
            <Row label="适配器" value="—" />
          )}
          <Row label="技术负责人" value={conn.technicalOwner?.label ?? "—"} />
          <Row
            label={freshnessText.catalogSyncAt}
            value={`${conn.catalog.stateLabel}${
              conn.catalog.lastSuccessfulAt
                ? ` · ${formatDateTime(conn.catalog.lastSuccessfulAt, "default")}`
                : ""
            }`}
          />
        </CardContent>
      </Card>
      <Card
        size="sm"
        className={cn(surfaceInsetClassName, "shadow-none ring-0 lg:col-span-2")}
      >
        <CardHeader className="rounded-t-lg border-b border-border/30 pb-2">
          <CardTitle className="text-base">能力与健康摘要</CardTitle>
          <CardDescription>
            连接级能力声明不等于每个商品可用 ·{" "}
            <Link
              href="/procurement/supplier-catalog"
              className="text-primary underline-offset-2 hover:underline"
            >
              供应商商品库
            </Link>
            {" · "}
            <Link
              href="/commerce/publications"
              className="text-primary underline-offset-2 hover:underline"
            >
              商品发布
            </Link>
          </CardDescription>
        </CardHeader>
        <CardContent className="flex flex-wrap gap-2">
          {conn.capabilities.map((c) => (
            <Badge
              key={c.capabilityCode}
              variant={c.status === "ENABLED" ? "default" : "secondary"}
            >
              {c.capabilityLabel}
              {c.status === "ENABLED" ? "" : "·停"}
              {c.verification === "SUCCESS"
                ? " ✓"
                : c.verification === "FAILED"
                  ? " !"
                  : ""}
            </Badge>
          ))}
          {conn.capabilities.length === 0 ? (
            <span className="text-sm text-muted-foreground">尚未配置能力</span>
          ) : null}
          <p className="w-full text-tiny text-muted-foreground">
            图例：✓ 验证成功 · ! 验证失败 · 停 能力停用
          </p>
        </CardContent>
      </Card>
    </div>
  )
}

function CapabilitiesSection({
  conn,
  onOpenConfig,
}: {
  conn: ConnectionCenterView
  onOpenConfig: () => void
}) {
  const columns = React.useMemo<ColumnDef<CapabilityView>[]>(
    () => [
      {
        id: "code",
        accessorFn: (r) => r.capabilityLabel,
        header: "能力",
        meta: { label: "能力", width: "reference" },
        cell: ({ row }) => (
          <div className="text-sm font-medium">
            {row.original.capabilityLabel}
          </div>
        ),
      },
      {
        id: "status",
        header: "能力状态",
        meta: { label: "能力状态", width: "status" },
        cell: ({ row }) => (
          <BusinessStatusBadge
            context="list"
            label={row.original.statusLabel}
            tone={row.original.status === "ENABLED" ? "success" : "neutral"}
          />
        ),
      },
      {
        id: "req",
        header: "业务需求确认",
        meta: { label: "业务需求" },
        cell: ({ row }) => (
          <span className="text-sm">
            {row.original.businessRequirementLabel}
          </span>
        ),
      },
      {
        id: "verify",
        header: "验证",
        meta: { label: "验证" },
        cell: ({ row }) => (
          <span className="text-sm">{row.original.verificationLabel}</span>
        ),
      },
      {
        id: "note",
        header: "边界说明",
        meta: { label: "边界" },
        cell: () => (
          <span className="text-xs text-muted-foreground">
            连接级 ≠ 商品级 · 见供应商商品库/商品发布
          </span>
        ),
      },
      {
        id: "actions",
        header: "动作",
        meta: { label: "动作" },
        cell: () => <span className="text-xs text-muted-foreground">—</span>,
      },
    ],
    []
  )

  return (
    <div className="space-y-3">
      <Alert>
        <AlertTitle>能力边界</AlertTitle>
        <AlertDescription>
          下表为<strong>连接级</strong>
          统一能力声明，不表示每个供应商商品都可用。商品/供给/发布级能力由供应商商品库 / 商品发布返回。能力启停由系统管理员配置。
        </AlertDescription>
      </Alert>
      <div className="flex justify-end">
        <Button type="button" size="sm" onClick={onOpenConfig}>
          配置能力
        </Button>
      </div>
      <BusinessTableFrame
        title="能力矩阵"
        description="连接级能力 × 状态 × 业务需求 × 验证；不等于商品级可用"
        table={
          <DataTable
            data={conn.capabilities}
            columns={columns}
            getRowId={(r) => r.capabilityCode}
            rowCount={conn.capabilities.length}
            caption="连接能力矩阵"
            density="compact"
            layout="flush"
            showPagination={false}
            defaultColumnPinning={{ left: ["code"] }}
            emptyState={
              <BusinessEmptyState
                kind="no-data"
                className="rounded-lg border-0 bg-transparent p-6 shadow-none ring-0"
                title="尚未配置能力"
                description="可配置能力启停；业务需求与验证状态随后端数据返回。"
              />
            }
          />
        }
      />
    </div>
  )
}

function SecuritySection({
  conn,
  onBind,
  onBindEndpoint,
}: {
  conn: ConnectionCenterView
  onBind: () => void
  onBindEndpoint: () => void
}) {
  return (
    <div className="space-y-3">
      <Alert>
        <KeyRoundIcon aria-hidden="true" />
        <AlertTitle>安全配置引用</AlertTitle>
        <AlertDescription>
          仅显示绑定状态、安全别名与版本。永不展示、复制或导出密钥正文。轮换只能选择密钥管理系统不透明引用。
        </AlertDescription>
      </Alert>
      <div className="grid gap-3 sm:grid-cols-2">
        <Card size="sm" className={cn(surfaceInsetClassName, "shadow-none ring-0")}>
          <CardHeader className="rounded-t-lg border-b border-border/30 pb-2">
            <CardTitle className="text-base">地址配置引用</CardTitle>
          </CardHeader>
          <CardContent className="space-y-2 text-sm">
            <RefLabel
              state={conn.safeReferences.endpoint.state}
              alias={conn.safeReferences.endpoint.alias}
              version={conn.safeReferences.endpoint.version}
              visible={conn.safeReferences.endpoint.visible}
            />
            <Button type="button" size="sm" onClick={onBindEndpoint}>
              绑定/轮换地址
            </Button>
          </CardContent>
        </Card>
        <Card size="sm" className={cn(surfaceInsetClassName, "shadow-none ring-0")}>
          <CardHeader className="rounded-t-lg border-b border-border/30 pb-2">
            <CardTitle className="text-base">密钥配置引用</CardTitle>
          </CardHeader>
          <CardContent className="space-y-2 text-sm">
            <RefLabel
              state={conn.safeReferences.credential.state}
              alias={conn.safeReferences.credential.alias}
              version={conn.safeReferences.credential.version}
              visible={conn.safeReferences.credential.visible}
            />
            <Button type="button" size="sm" onClick={onBind}>
              绑定/轮换引用
            </Button>
          </CardContent>
        </Card>
      </div>
    </div>
  )
}

function HealthSection({
  records,
  last,
}: {
  records: HealthRecordView[]
  last?: ConnectionCenterView["lastHealth"]
}) {
  const columns = React.useMemo<ColumnDef<HealthRecordView>[]>(
    () => [
      {
        id: "at",
        accessorFn: (r) => r.at,
        header: "时间",
        meta: { label: "时间" },
        cell: ({ row }) => (
          <span className="text-sm">{formatDateTime(row.original.at, "default")}</span>
        ),
      },
      {
        id: "type",
        accessorFn: (r) => r.checkType,
        header: "检查类型",
        meta: { label: "检查类型" },
      },
      {
        id: "result",
        header: "结果",
        meta: { label: "结果", width: "status" },
        cell: ({ row }) => (
          <div className="space-y-0.5">
            <BusinessStatusBadge
              context="list"
              label={row.original.resultLabel}
              tone={row.original.resultTone}
            />
            {row.original.autoRetryStopped ? (
              <div className="text-tiny text-destructive" role="status">
                自动重试已停止
              </div>
            ) : null}
            {row.original.result === "UNKNOWN" ? (
              <div className="text-tiny text-warning-soft-foreground" role="status">
                结果未知 · 不按失败播报
              </div>
            ) : null}
          </div>
        ),
      },
      {
        id: "latency",
        header: "耗时",
        meta: { label: "耗时", numeric: true },
        cell: ({ row }) => (
          <span className="num text-sm">
            {row.original.latencyMs != null
              ? `${row.original.latencyMs} ms`
              : "—"}
          </span>
        ),
      },
      {
        id: "job",
        header: "任务号",
        meta: { label: "任务号" },
        cell: ({ row }) => (
          <span className="font-mono text-xs">
            {row.original.jobNo ?? "—"}
          </span>
        ),
      },
      {
        id: "trace",
        header: "追踪号",
        meta: { label: "追踪号" },
        cell: ({ row }) => (
          <span className="font-mono text-xs">
            {row.original.traceId ?? "—"}
          </span>
        ),
      },
      {
        id: "summary",
        header: "摘要",
        meta: { label: "摘要" },
        cell: ({ row }) => (
          <span className="text-xs text-muted-foreground">
            {row.original.errorSummary ?? "—"}
          </span>
        ),
      },
    ],
    []
  )

  return (
    <div className="space-y-3">
      {last ? (
        <p className="text-sm text-muted-foreground">
          最近：{formatDateTime(last.at, "default")} · {last.resultLabel}
          {last.autoRetryStopped ? " · 自动重试已停止" : ""}
        </p>
      ) : null}
      <BusinessTableFrame
        title="健康检查记录"
        description="不展示原始密钥与敏感消息内容；结果未知单独文字说明"
        table={
          <DataTable
            data={records}
            columns={columns}
            getRowId={(r) => r.recordId}
            rowCount={records.length}
            caption="健康检查记录"
            density="compact"
            layout="flush"
            manualPagination={false}
            emptyState={
              <BusinessEmptyState
                kind="no-data"
                className="rounded-lg border-0 bg-transparent p-6 shadow-none ring-0"
                title="暂无健康记录"
                description="技术角色可在页头执行健康检查，结果会记录在本页。"
              />
            }
          />
        }
      />
    </div>
  )
}

function CatalogSection({
  conn,
  syncing,
  onSync,
}: {
  conn: ConnectionCenterView
  syncing: boolean
  onSync: () => Promise<void>
}) {
  const progress = conn.catalog.progress
  return (
    <div className="space-y-3">
      <Card size="sm" className={cn(surfaceInsetClassName, "shadow-none ring-0")}>
        <CardHeader className="rounded-t-lg border-b border-border/30 pb-2">
          <CardTitle className="text-base">目录同步进度</CardTitle>
          <CardDescription>
            与连接状态分开展示 ·{" "}
            <Link
              href={`/procurement/supplier-catalog?connectionId=${conn.connectionId}`}
              className="inline-flex items-center gap-1 text-primary underline-offset-2 hover:underline"
            >
              打开供应商商品库
              <ExternalLinkIcon className="size-3" aria-hidden="true" />
            </Link>
          </CardDescription>
        </CardHeader>
        <CardContent className="space-y-3 text-sm">
          <Row label="同步状态" value={conn.catalog.stateLabel} />
          <Row
            label="最近成功"
            value={formatDateTime(conn.catalog.lastSuccessfulAt, "default")}
          />
          <Row
            label="当前任务"
            value={conn.catalog.activeJobNo ?? "—"}
            mono
          />
          {progress ? (
            <BackgroundJobProgress
              mode="partialAllowed"
              status={progress.status}
              total={progress.total}
              completed={progress.completed}
              succeeded={progress.succeeded}
              failed={progress.failed}
              label={`目录同步 ${conn.catalog.activeJobNo ?? ""}`}
              description="目录同步在后台执行；同来源批次不会重复处理。"
            />
          ) : null}
          <div className="flex flex-wrap gap-2">
            <Button
              type="button"
              size="sm"
              disabled={syncing}
              onClick={() => void onSync()}
            >
              触发目录同步
            </Button>
          </div>
        </CardContent>
      </Card>
    </div>
  )
}

function RelatedSection({ conn }: { conn: ConnectionCenterView }) {
  return (
    <div className="grid gap-3 sm:grid-cols-2 lg:grid-cols-4">
      {[
        {
          label: "活跃供给",
          value: conn.relatedImpact.activeOfferings,
          href: "/procurement/supplier-catalog",
        },
        {
          label: "生效发布",
          value: conn.relatedImpact.activePublications,
          href: "/commerce/publications",
        },
        {
          label: "待处理订单",
          value: conn.relatedImpact.openSupplierOrders,
          href: "/supplier-api/orders",
        },
        {
          label: "同步任务",
          value: conn.relatedImpact.activeSyncJobs,
          href: "/procurement/supplier-catalog",
        },
      ].map((item) => (
        <Card
          key={item.label}
          size="sm"
          className={cn(surfaceInsetClassName, "shadow-none ring-0")}
        >
          <CardHeader className="pb-1">
            <CardDescription>{item.label}</CardDescription>
            <CardTitle className="num text-2xl">{item.value}</CardTitle>
          </CardHeader>
          <CardContent>
            <Link
              href={item.href}
              className="text-xs text-primary underline-offset-2 hover:underline"
            >
              打开关联页面
            </Link>
          </CardContent>
        </Card>
      ))}
      <p className="text-xs text-muted-foreground sm:col-span-2 lg:col-span-4">
        进入相关页面时将重新获取最新状态。
      </p>
    </div>
  )
}

function AuditSection({ conn }: { conn: ConnectionCenterView }) {
  const [expanded, setExpanded] = React.useState(false)
  const events = expanded ? conn.auditEvents : conn.auditEvents.slice(0, 10)
  return (
    <div className="space-y-3">
      <p className="text-sm text-muted-foreground">
        配置变更与业务确认均保留审计记录 ·{" "}
        <Link
          href={`/system/access-audit?objectId=${conn.connectionId}`}
          className="text-primary underline-offset-2 hover:underline"
        >
          打开权限与审计
        </Link>
      </p>
      <ul className="space-y-2">
        {events.map((e) => (
          <li
            key={e.eventId}
            className={cn(surfaceInsetClassName, "px-3 py-2 text-sm")}
          >
            <div className="flex flex-wrap items-center justify-between gap-2">
              <span className="font-medium">
                {AUDIT_ACTION_LABEL[e.action] ?? e.summary.split("·")[0]}
              </span>
              <span className="text-xs text-muted-foreground">
                {formatDateTime(e.at, "default")}
              </span>
            </div>
            <p className="text-muted-foreground">{e.summary}</p>
            <p className="text-xs text-muted-foreground">
              {e.actor}
              {e.auditNo ? ` · 审计号 ${e.auditNo}` : ""}
            </p>
          </li>
        ))}
        {conn.auditEvents.length === 0 ? (
          <BusinessEmptyState
            kind="no-data"
            className="rounded-lg border-0 bg-transparent p-6 shadow-none ring-0"
            title="暂无审计事件"
            description="配置与确认动作会追加审计记录。"
          />
        ) : null}
      </ul>
      {conn.auditEvents.length > 10 ? (
        <Button
          type="button"
          size="sm"
          variant="ghost"
          className="text-muted-foreground hover:text-foreground"
          onClick={() => setExpanded((v) => !v)}
        >
          {expanded ? "收起" : `查看更多（共 ${conn.auditEvents.length} 条）`}
        </Button>
      ) : null}
    </div>
  )
}

function CapConfigDialog({
  open,
  onOpenChange,
  conn,
  pending,
  onSubmit,
}: {
  open: boolean
  onOpenChange: (o: boolean) => void
  conn: ConnectionCenterView
  pending: boolean
  onSubmit: (
    changes: Array<{ code: CapabilityCode; enabled: boolean }>
  ) => Promise<void>
}) {
  const [draft, setDraft] = React.useState<Record<string, boolean>>({})

  React.useEffect(() => {
    if (open) {
      const next: Record<string, boolean> = {}
      for (const c of conn.capabilities) {
        next[c.capabilityCode] = c.status === "ENABLED"
      }
      setDraft(next)
    }
  }, [open, conn.capabilities])

  return (
    <Dialog open={open} onOpenChange={onOpenChange}>
      <DialogContent>
        <DialogHeader>
          <DialogTitle>配置连接能力</DialogTitle>
          <DialogDescription>
            由系统管理员统一配置，配置后能力需重新验证；不复用采购确认写入口。
          </DialogDescription>
        </DialogHeader>
        <div className="max-h-72 space-y-2 overflow-y-auto">
          {conn.capabilities.map((c) => (
            <label
              key={c.capabilityCode}
              className="flex items-center justify-between gap-2 rounded-lg border px-3 py-2 text-sm"
            >
              <span>{c.capabilityLabel}</span>
              <input
                type="checkbox"
                checked={draft[c.capabilityCode] ?? false}
                onChange={(e) =>
                  setDraft((d) => ({
                    ...d,
                    [c.capabilityCode]: e.target.checked,
                  }))
                }
                aria-label={`${
                  draft[c.capabilityCode] ?? false ? "停用" : "启用"
                } ${c.capabilityLabel}`}
              />
            </label>
          ))}
        </div>
        <DialogFooter>
          <Button
            type="button"
            variant="outline"
            onClick={() => onOpenChange(false)}
          >
            取消
          </Button>
          <Button
            type="button"
            disabled={pending}
            onClick={() => {
              const changes = conn.capabilities
                .filter(
                  (c) =>
                    (draft[c.capabilityCode] ?? false) !==
                    (c.status === "ENABLED")
                )
                .map((c) => ({
                  code: c.capabilityCode,
                  enabled: draft[c.capabilityCode] ?? false,
                }))
              if (changes.length === 0) {
                onOpenChange(false)
                return
              }
              void onSubmit(changes)
            }}
          >
            提交能力配置
          </Button>
        </DialogFooter>
      </DialogContent>
    </Dialog>
  )
}

function Row({
  label,
  value,
  mono,
}: {
  label: string
  value: React.ReactNode
  mono?: boolean
}) {
  return (
    <div className="flex items-start justify-between gap-3">
      <dt className="shrink-0 text-muted-foreground">{label}</dt>
      <dd className={mono ? "font-mono text-right" : "text-right"}>{value}</dd>
    </div>
  )
}

function RefLabel({
  state,
  alias,
  version,
  visible,
}: {
  state: "MISSING" | "BOUND" | "ROTATION_DUE"
  alias?: string
  version?: string
  visible: boolean
}) {
  const label = REFERENCE_STATE_LABEL[state]
  return (
    <div
      className="space-y-0.5"
      aria-label={`引用状态 ${label}${
        visible && alias ? ` 别名 ${alias} 版本 ${version}` : ""
      }`}
    >
      <BusinessStatusBadge
        context="list"
        label={label}
        tone={
          state === "BOUND"
            ? "success"
            : state === "ROTATION_DUE"
              ? "warning"
              : "neutral"
        }
      />
      {visible && alias ? (
        <div className="font-mono text-xs text-muted-foreground">
          {alias}
          {version ? ` · ${version}` : ""}
        </div>
      ) : (
        <div className="text-xs text-muted-foreground">
          {state === "BOUND"
            ? "配置已绑定"
            : state === "ROTATION_DUE"
              ? "需轮换"
              : "待绑定"}
        </div>
      )}
    </div>
  )
}
