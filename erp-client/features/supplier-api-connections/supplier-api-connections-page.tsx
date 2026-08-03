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
  ListToolbar,
  MetricFilterItem,
  MetricStrip,
  OptionCombobox,
  SupplierCombobox,
  PageActions,
  PageHeader,
} from "@/components/business"
import { toFieldErrors, useAppForm } from "@/components/form"
import { PROCUREMENT_SUPPLIER_OPTIONS } from "@/lib/business-options"
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
  useConfirmCapabilityMutation,
  useConnectionCenterQuery,
  useConnectionListQuery,
  useCreateConnectionMutation,
  useDisableConnectionMutation,
  useEnableConnectionMutation,
  useQueryFormalIdempotencyMutation,
  useRunHealthCheckMutation,
  useStartCatalogSyncMutation,
  useUpdateCapabilitiesMutation,
} from "@/features/supplier-api-connections/queries"
import type {
  CapabilityCode,
  CapabilityView,
  ConnectionCenterView,
  ConnectionListItem,
  ConnectionSection,
  DemoRole,
  FormalOutcome,
  HealthRecordView,
} from "@/features/supplier-api-connections/types"
import {
  CAPABILITY_LABEL,
  DEMO_ROLE_LABEL,
  ENVIRONMENT_LABEL,
  REFERENCE_STATE_LABEL,
  SECTION_LABEL,
  SECTIONS,
} from "@/features/supplier-api-connections/types"
import {
  buildConnectionsSearchParams,
  parseConnectionsSearchParams,
  type ConnectionsUrlState,
} from "@/features/supplier-api-connections/url-state"
import { freshnessText } from "@/lib/ui-text"

type ResultState = {
  status: "succeeded" | "failed" | "blocked" | "rejected" | "unknown" | "processing"
  title: string
  description: string
  reference?: string
  facts?: Array<{ label: string; value: React.ReactNode }>
  pendingIdempotencyKey?: string
  jobId?: string
  jobNo?: string
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
      reference: outcome.operationId,
      pendingIdempotencyKey: outcome.idempotencyKey,
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
        if (next.role !== "admin") params.set("role", next.role)
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

function RoleDemoBar({
  role,
  demoFlag,
  onRole,
  onFlag,
}: {
  role: DemoRole
  demoFlag?: "no-permission" | "no-scope"
  onRole: (r: DemoRole) => void
  onFlag: (f?: "no-permission" | "no-scope") => void
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
          { value: "procurement", label: "采购" },
          { value: "ops", label: "研发运维" },
          { value: "admin", label: "系统管理员" },
        ]}
        className="w-[9rem]"
        size="sm"
        allowClear={false}
      />
      <OptionCombobox
        value={demoFlag ?? "normal"}
        onValueChange={(v) => {
          if (v == null || v === "normal") onFlag(undefined)
          else onFlag(v as "no-permission" | "no-scope")
        }}
        options={[
          { value: "normal", label: "正常权限" },
          { value: "no-permission", label: "无模块权限" },
          { value: "no-scope", label: "无数据范围" },
        ]}
        className="w-[11rem]"
        size="sm"
        allowClear={false}
      />
      <span className="text-xs text-muted-foreground">
        当前：{DEMO_ROLE_LABEL[role]}
        {role === "procurement"
          ? " · 业务确认，不可写能力状态/密钥"
          : role === "ops"
            ? " · 技术引用与健康检查"
            : " · 启停与能力治理；不可代采购确认"}
      </span>
    </div>
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
  const [result, setResult] = React.useState<ResultState>(null)
  const createMutation = useCreateConnectionMutation()

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
    pageSize: 20,
    role: urlState.role,
    demoFlag: urlState.demoFlag,
  })

  const data = listQuery.data
  const [pagination, setPagination] = React.useState<PaginationState>({
    pageIndex: Math.max(0, urlState.page - 1),
    pageSize: 20,
  })

  React.useEffect(() => {
    setPagination((p) => ({
      ...p,
      pageIndex: Math.max(0, urlState.page - 1),
    }))
  }, [urlState.page])

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
              <div className="font-mono text-sm font-medium">
                {r.connectionCode}
              </div>
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
            <div className="text-[11px] text-muted-foreground">
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
            <div className="text-[11px] text-muted-foreground">
              {formatTime(row.original.lastHealthAt)}
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
        role: urlState.role,
        idempotencyKey: newIdempotencyKey("create"),
      })
      const mapped = outcomeToResult(outcome)
      setResult(mapped)
      if (outcome.status === "succeeded" && outcome.reference) {
        setCreateOpen(false)
        form.reset()
        onOpen(outcome.reference)
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
        <PageHeader title="API 供应商连接" description="加载失败" />
        <BusinessFailureState
          kind="system"
          title="连接列表加载失败"
          description="请重试。已有数据时保留旧连接。"
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
    <div className="mx-auto flex w-full max-w-shell flex-col gap-3 p-3 md:p-4">
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
            updatedAt="刚刚"
            dateTime={data?.projectedAt}
            state="fresh"
            label="连接列表"
          />
        }
        actions={
          <PageActions
            actions={[
              {
                actionKey: "create",
                label: "新建连接",
                icon: PlusIcon,
                mobileVisibility: "hide",
                disabled: urlState.role !== "admin" || !data?.hasModulePermission,
                onClick: () => setCreateOpen(true),
              },
            ]}
          />
        }
      />

      <RoleDemoBar
        role={urlState.role}
        demoFlag={urlState.demoFlag}
        onRole={(r) => patchUrl({ role: r, page: 1 })}
        onFlag={(f) => patchUrl({ demoFlag: f, page: 1 })}
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
        />
      ) : null}

      {empty === "NO_PERMISSION" ? (
        <BusinessEmptyState
          kind="no-scope"
          title="无模块权限"
          description="当前角色无权访问 API 供应商连接。不展示导航内快捷创建。"
        />
      ) : empty === "NO_SCOPE" ? (
        <BusinessEmptyState
          kind="no-scope"
          title="当前角色无连接数据范围"
          description="你可进入此页面，但授权供应商/环境范围内没有可查看连接。不显示 0 连接。"
        />
      ) : (
        <>
          <MetricStrip columns={5} aria-label="连接指标筛选">
            <MetricFilterItem
              label="已启用"
              value={data?.metrics.enabled ?? 0}
              active={urlState.status === "ENABLED"}
              onClick={() =>
                patchUrl({
                  status:
                    urlState.status === "ENABLED" ? undefined : "ENABLED",
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
                  status:
                    urlState.status === "FAULTED" ? undefined : "FAULTED",
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
              <div className="flex flex-wrap items-center gap-2">
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
                />
                <Button
                  type="button"
                  size="sm"
                  variant="ghost"
                  onClick={() => {
                    setSearchDraft("")
                    patchUrl({
                      q: undefined,
                      status: undefined,
                      health: undefined,
                      catalogFreshness: undefined,
                      capability: undefined,
                      page: 1,
                    })
                  }}
                >
                  清除筛选
                </Button>
              </div>
            }
          />

          <BusinessTableFrame
            title="连接列表"
            description="一行展示代码、供应商、环境、状态、能力、健康与下一步；身份与操作列固定"
            table={
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
                defaultColumnPinning={{
                  left: ["identity"],
                  right: ["actions"],
                }}
                pagination={pagination}
                onPaginationChange={(next) => {
                  setPagination(next)
                  patchUrl({ page: next.pageIndex + 1 })
                }}
                onRowOpen={(row) => onOpen(row.connectionId)}
                emptyState={
                  empty === "FILTER_NO_RESULT" ? (
                    <BusinessEmptyState
                      kind="filter"
                      title="当前筛选无结果"
                      description="没有连接符合当前环境/状态/健康条件。可清除筛选。"
                      action={
                        <Button
                          type="button"
                          variant="outline"
                          onClick={() =>
                            patchUrl({
                              status: undefined,
                              health: undefined,
                              catalogFreshness: undefined,
                              q: undefined,
                              page: 1,
                            })
                          }
                        >
                          清除筛选
                        </Button>
                      }
                    />
                  ) : empty === "NO_CONNECTIONS" ? (
                    <BusinessEmptyState
                      kind="no-data"
                      title="尚未接入供应商连接"
                      description="当前环境还没有连接身份。有权限时可新建连接。"
                      action={
                        urlState.role === "admin" ? (
                          <Button
                            type="button"
                            onClick={() => setCreateOpen(true)}
                          >
                            新建连接
                          </Button>
                        ) : null
                      }
                    />
                  ) : undefined
                }
              />
            }
          />
        </>
      )}

      <Dialog open={createOpen} onOpenChange={setCreateOpen}>
        <DialogContent>
          <DialogHeader>
            <DialogTitle>新建连接身份</DialogTitle>
            <DialogDescription>
              创建全局唯一连接代码（环境不是唯一键组成部分）。成功后打开连接详情完成配置。
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
                        const supplier = PROCUREMENT_SUPPLIER_OPTIONS.find(
                          (s) => s.supplierId === next
                        )
                        form.setFieldValue(
                          "supplierName",
                          supplier?.supplierName ?? ""
                        )
                      }}
                      suppliers={PROCUREMENT_SUPPLIER_OPTIONS}
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
                    <p className="text-xs text-destructive" role="status">
                      正在创建生产环境连接身份
                    </p>
                  ) : null}
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
                <form.SubmitButton label="创建" />
              </form.AppForm>
            </DialogFooter>
          </form>
        </DialogContent>
      </Dialog>
    </div>
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
  const centerQuery = useConnectionCenterQuery(connectionId, urlState.role)
  const [result, setResult] = React.useState<ResultState>(null)
  const [disableOpen, setDisableOpen] = React.useState(false)
  const [credOpen, setCredOpen] = React.useState(false)
  const [selectedRef, setSelectedRef] = React.useState<string>("")
  const [capConfigOpen, setCapConfigOpen] = React.useState(false)

  const bindCred = useBindCredentialMutation()
  const confirmCap = useConfirmCapabilityMutation()
  const updateCaps = useUpdateCapabilitiesMutation()
  const runHealth = useRunHealthCheckMutation()
  const startCatalog = useStartCatalogSyncMutation()
  const disableMut = useDisableConnectionMutation()
  const enableMut = useEnableConnectionMutation()
  const queryIdem = useQueryFormalIdempotencyMutation()
  const listQuery = useConnectionListQuery({
    environment: "ALL",
    page: 1,
    role: urlState.role,
  })

  const conn = centerQuery.data
  const section = urlState.section

  const can = (action: string) =>
    Boolean(conn?.allowedActions.includes(action))
  const blockerMsg = (action: string) =>
    conn?.actionBlockers.find((b) => b.action === action)?.message

  const applyOutcome = (outcome: FormalOutcome) => {
    setResult(outcomeToResult(outcome))
  }

  if (centerQuery.isPending) {
    return (
      <div className="mx-auto flex w-full max-w-shell flex-col gap-4 p-4 md:p-5">
        <div className="h-10 w-40 animate-pulse rounded-lg bg-muted" />
        <div className="h-24 animate-pulse rounded-xl bg-muted" />
        <div className="h-64 animate-pulse rounded-2xl bg-muted" />
      </div>
    )
  }

  if (centerQuery.isError) {
    return (
      <div className="mx-auto flex w-full max-w-shell p-5">
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
      </div>
    )
  }

  if (!conn) {
    return (
      <div className="mx-auto flex w-full max-w-shell flex-col gap-4 p-4 md:p-5">
        <Button type="button" variant="ghost" size="sm" onClick={onBack}>
          <ArrowLeftIcon className="size-4" aria-hidden="true" />
          返回列表
        </Button>
        <BusinessEmptyState
          kind="no-data"
          title="未找到连接"
          description={`连接 ${connectionId} 不存在或无权查看。`}
        />
      </div>
    )
  }

  const isProd = conn.environment === "PRODUCTION"
  const authFailed = conn.lastHealth?.result === "AUTH_FAILED"
  const resultUnknown = conn.lastHealth?.result === "UNKNOWN"

  return (
    <div className="mx-auto flex w-full max-w-shell flex-col gap-3 p-4 md:p-5">
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

      <RoleDemoBar
        role={urlState.role}
        demoFlag={urlState.demoFlag}
        onRole={(r) => patchUrl({ role: r })}
        onFlag={(f) => patchUrl({ demoFlag: f })}
      />

      <DocumentHeader
        density="compact"
        title={`${conn.connectionCode} · ${conn.supplier.name}`}
        documentNumber={conn.connectionId}
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
              配置 {formatTime(conn.updatedAt)}
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
            {can("RUN_HEALTH_CHECK") ? (
              <Button
                type="button"
                size="sm"
                variant="outline"
                disabled={runHealth.isPending}
                title={blockerMsg("RUN_HEALTH_CHECK")}
                onClick={async () => {
                  const outcome = await runHealth.mutateAsync({
                    connectionId: conn.connectionId,
                    expectedVersion: conn.version,
                    role: urlState.role,
                    idempotencyKey: newIdempotencyKey("health"),
                  })
                  applyOutcome(outcome)
                }}
              >
                <RefreshCwIcon className="size-4" aria-hidden="true" />
                健康检查
              </Button>
            ) : null}
            {can("ENABLE") && conn.status !== "ENABLED" ? (
              <Button
                type="button"
                size="sm"
                disabled={enableMut.isPending}
                title={blockerMsg("ENABLE")}
                onClick={async () => {
                  const outcome = await enableMut.mutateAsync({
                    connectionId: conn.connectionId,
                    expectedVersion: conn.version,
                    role: urlState.role,
                    idempotencyKey: newIdempotencyKey("enable"),
                  })
                  applyOutcome(outcome)
                }}
              >
                启用连接
              </Button>
            ) : null}
            {can("DISABLE") && conn.status === "ENABLED" ? (
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
            actions={
              result.pendingIdempotencyKey ? (
                <Button
                  type="button"
                  size="sm"
                  variant="outline"
                  disabled={queryIdem.isPending}
                  onClick={async () => {
                    const r = await queryIdem.mutateAsync(
                      result.pendingIdempotencyKey!
                    )
                    if (r) applyOutcome(r)
                  }}
                >
                  按原任务号查询最终结果
                </Button>
              ) : undefined
            }
          />
          {result.jobNo ? (
            <BackgroundJobProgress
              mode="partialAllowed"
              status={
                result.status === "processing"
                  ? "running"
                  : result.status === "succeeded"
                    ? "succeeded"
                    : "failed"
              }
              total={conn.capabilities.filter((c) => c.status === "ENABLED")
                .length || 4}
              completed={
                result.status === "processing"
                  ? 1
                  : conn.capabilities.filter((c) => c.status === "ENABLED")
                      .length || 4
              }
              succeeded={result.status === "succeeded" ? 4 : 0}
              failed={result.status === "failed" ? 1 : 0}
              label={`后台任务 ${result.jobNo}`}
              description="请求成功返回不代表业务处理完成，请以任务号查询最终结果。"
            />
          ) : null}
        </div>
      ) : null}

      <Tabs
        value={section}
        onValueChange={(v) => {
          if (v) patchUrl({ section: v as ConnectionSection })
        }}
      >
        <TabsList className="flex h-auto flex-wrap justify-start gap-1">
          {SECTIONS.map((s) => (
            <TabsTrigger key={s} value={s} className="text-xs sm:text-sm">
              {SECTION_LABEL[s]}
            </TabsTrigger>
          ))}
        </TabsList>
      </Tabs>

      {section === "overview" ? (
        <OverviewSection conn={conn} role={urlState.role} />
      ) : null}
      {section === "capabilities" ? (
        <CapabilitiesSection
          conn={conn}
          role={urlState.role}
          can={can}
          blockerMsg={blockerMsg}
          onConfirm={async (code, requirement) => {
            const cap = conn.capabilities.find((c) => c.capabilityCode === code)
            if (!cap) return
            const outcome = await confirmCap.mutateAsync({
              connectionId: conn.connectionId,
              capabilityCode: code,
              requirement,
              reasonCode: "BUSINESS_NEED",
              expectedConnectionVersion: conn.version,
              expectedCapabilityVersion: cap.version,
              role: urlState.role,
              operationId: newIdempotencyKey("op_ccr"),
              idempotencyKey: newIdempotencyKey("ccr"),
            })
            applyOutcome(outcome)
          }}
          onOpenConfig={() => setCapConfigOpen(true)}
          confirming={confirmCap.isPending}
        />
      ) : null}
      {section === "security" ? (
        <SecuritySection
          conn={conn}
          role={urlState.role}
          canBind={can("BIND_CREDENTIAL_REFERENCE")}
          onBind={() => {
            setSelectedRef(
              listQuery.data?.credentialOpaqueOptions[0]?.referenceId ?? ""
            )
            setCredOpen(true)
          }}
        />
      ) : null}
      {section === "health" ? (
        <HealthSection records={conn.healthRecords} last={conn.lastHealth} />
      ) : null}
      {section === "catalog" ? (
        <CatalogSection
          conn={conn}
          canSync={can("START_CATALOG_SYNC")}
          blocker={blockerMsg("START_CATALOG_SYNC")}
          syncing={startCatalog.isPending}
          onSync={async () => {
            const outcome = await startCatalog.mutateAsync({
              connectionId: conn.connectionId,
              role: urlState.role,
              idempotencyKey: newIdempotencyKey("catalog"),
            })
            applyOutcome(outcome)
          }}
        />
      ) : null}
      {section === "related" ? <RelatedSection conn={conn} /> : null}
      {section === "audit" ? <AuditSection conn={conn} /> : null}

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
            processable={1}
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
          <p className="text-xs text-muted-foreground">
            历史版本与业务记录保留；不暗示删除。替代方案可链到供应商商品库 / 供应商订单 / 接口错误中心。
          </p>
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
                  role: urlState.role,
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
                  role: urlState.role,
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
            role: urlState.role,
            operationId: newIdempotencyKey("op_cap"),
            idempotencyKey: newIdempotencyKey("cap"),
          })
          applyOutcome(outcome)
          if (outcome.status === "succeeded") setCapConfigOpen(false)
        }}
      />
    </div>
  )
}

function OverviewSection({
  conn,
  role,
}: {
  conn: ConnectionCenterView
  role: DemoRole
}) {
  return (
    <div className="grid gap-3 lg:grid-cols-2">
      <Card>
        <CardHeader className="pb-2">
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
      <Card>
        <CardHeader className="pb-2">
          <CardTitle className="text-base">技术就绪</CardTitle>
          <CardDescription>
            {role === "procurement"
              ? "采购角色仅查看就绪状态"
              : "地址/密钥引用与适配器"}
          </CardDescription>
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
                ? ` · ${formatTime(conn.catalog.lastSuccessfulAt)}`
                : ""
            }`}
          />
        </CardContent>
      </Card>
      <Card className="lg:col-span-2">
        <CardHeader className="pb-2">
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
        </CardContent>
      </Card>
    </div>
  )
}

function CapabilitiesSection({
  conn,
  role,
  can,
  blockerMsg,
  onConfirm,
  onOpenConfig,
  confirming,
}: {
  conn: ConnectionCenterView
  role: DemoRole
  can: (a: string) => boolean
  blockerMsg: (a: string) => string | undefined
  onConfirm: (
    code: CapabilityCode,
    requirement: "REQUIRED" | "NOT_REQUIRED"
  ) => Promise<void>
  onOpenConfig: () => void
  confirming: boolean
}) {
  const columns = React.useMemo<ColumnDef<CapabilityView>[]>(
    () => [
      {
        id: "code",
        accessorFn: (r) => r.capabilityLabel,
        header: "能力",
        meta: { label: "能力", width: "reference" },
        cell: ({ row }) => (
          <div>
            <div className="text-sm font-medium">
              {row.original.capabilityLabel}
            </div>
            <div className="font-mono text-[11px] text-muted-foreground">
              {row.original.capabilityCode}
            </div>
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
        cell: ({ row }) => (
          <div className="flex flex-wrap gap-1">
            {role === "procurement" &&
            can("CONFIRM_CAPABILITY_REQUIREMENT") ? (
              <>
                <Button
                  type="button"
                  size="sm"
                  variant="outline"
                  disabled={confirming}
                  onClick={() =>
                    void onConfirm(row.original.capabilityCode, "REQUIRED")
                  }
                >
                  确认需要
                </Button>
                <Button
                  type="button"
                  size="sm"
                  variant="ghost"
                  disabled={confirming}
                  onClick={() =>
                    void onConfirm(row.original.capabilityCode, "NOT_REQUIRED")
                  }
                >
                  不需要
                </Button>
              </>
            ) : null}
          </div>
        ),
      },
    ],
    [can, confirming, onConfirm, role]
  )

  return (
    <div className="space-y-3">
      <Alert>
        <AlertTitle>能力边界</AlertTitle>
        <AlertDescription>
          下表为<strong>连接级</strong>
          统一能力声明，不表示每个供应商商品都可用。商品/供给/发布级能力由供应商商品库 / 商品发布返回。采购确认只追加业务需求与审计，不写能力启停；系统管理员使用独立配置命令。
        </AlertDescription>
      </Alert>
      {can("UPDATE_CAPABILITIES") ? (
        <div className="flex justify-end">
          <Button type="button" size="sm" onClick={onOpenConfig}>
            配置能力（管理员）
          </Button>
        </div>
      ) : (
        <p className="text-xs text-muted-foreground">
          {blockerMsg("UPDATE_CAPABILITIES") ??
            (role === "procurement"
              ? "你可确认业务需求；能力启停由管理员配置。"
              : "当前角色不可配置能力启停。")}
        </p>
      )}
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
            defaultColumnPinning={{ left: ["code"], right: ["actions"] }}
            emptyState={
              <BusinessEmptyState
                kind="no-data"
                title="尚未配置能力"
                description="管理员可配置能力；采购可在能力出现后确认业务需求。"
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
  role,
  canBind,
  onBind,
}: {
  conn: ConnectionCenterView
  role: DemoRole
  canBind: boolean
  onBind: () => void
}) {
  return (
    <div className="space-y-3">
      <Alert>
        <KeyRoundIcon aria-hidden="true" />
        <AlertTitle>安全配置引用</AlertTitle>
        <AlertDescription>
          仅显示绑定状态
          {role === "procurement"
            ? "（采购不显示别名/版本）"
            : "、安全别名与版本"}
          。永不展示、复制或导出密钥正文。轮换只能选择密钥管理系统不透明引用。
        </AlertDescription>
      </Alert>
      <div className="grid gap-3 sm:grid-cols-2">
        <Card>
          <CardHeader className="pb-2">
            <CardTitle className="text-base">地址配置引用</CardTitle>
          </CardHeader>
          <CardContent className="space-y-2 text-sm">
            <RefLabel
              state={conn.safeReferences.endpoint.state}
              alias={conn.safeReferences.endpoint.alias}
              version={conn.safeReferences.endpoint.version}
              visible={conn.safeReferences.endpoint.visible}
            />
          </CardContent>
        </Card>
        <Card>
          <CardHeader className="pb-2">
            <CardTitle className="text-base">密钥配置引用</CardTitle>
          </CardHeader>
          <CardContent className="space-y-2 text-sm">
            <RefLabel
              state={conn.safeReferences.credential.state}
              alias={conn.safeReferences.credential.alias}
              version={conn.safeReferences.credential.version}
              visible={conn.safeReferences.credential.visible}
            />
            {canBind ? (
              <Button type="button" size="sm" onClick={onBind}>
                绑定/轮换引用
              </Button>
            ) : (
              <p className="text-xs text-muted-foreground">
                当前角色不可轮换密钥引用
              </p>
            )}
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
          <span className="text-sm">{formatTime(row.original.at)}</span>
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
              <div className="text-[11px] text-destructive" role="status">
                自动重试已停止
              </div>
            ) : null}
            {row.original.result === "UNKNOWN" ? (
              <div className="text-[11px] text-amber-700" role="status">
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
          最近：{formatTime(last.at)} · {last.resultLabel}
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
            showPagination={false}
            emptyState={
              <BusinessEmptyState
                kind="no-data"
                title="暂无健康记录"
                description="技术角色可在页头执行健康检查；结果以任务号固定。"
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
  canSync,
  blocker,
  syncing,
  onSync,
}: {
  conn: ConnectionCenterView
  canSync: boolean
  blocker?: string
  syncing: boolean
  onSync: () => Promise<void>
}) {
  const progress = conn.catalog.progress
  return (
    <div className="space-y-3">
      <Card>
        <CardHeader className="pb-2">
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
            value={formatTime(conn.catalog.lastSuccessfulAt)}
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
              disabled={!canSync || syncing}
              title={blocker}
              onClick={() => void onSync()}
            >
              触发目录同步
            </Button>
            {!canSync && blocker ? (
              <span className="text-xs text-muted-foreground">{blocker}</span>
            ) : null}
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
        <Card key={item.label}>
          <CardHeader className="pb-1">
            <CardDescription>{item.label}</CardDescription>
            <CardTitle className="num text-2xl">{item.value}</CardTitle>
          </CardHeader>
          <CardContent>
            <Link
              href={item.href}
              className="text-xs text-primary underline-offset-2 hover:underline"
            >
              打开关联工作面
            </Link>
          </CardContent>
        </Card>
      ))}
      <p className="text-xs text-muted-foreground sm:col-span-2 lg:col-span-4">
        进入相关工作面时将重新获取最新状态。
      </p>
    </div>
  )
}

function AuditSection({ conn }: { conn: ConnectionCenterView }) {
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
        {conn.auditEvents.map((e) => (
          <li
            key={e.eventId}
            className="rounded-xl border bg-card px-3 py-2 text-sm"
          >
            <div className="flex flex-wrap items-center justify-between gap-2">
              <span className="font-medium">{e.action}</span>
              <span className="text-xs text-muted-foreground">
                {formatTime(e.at)}
              </span>
            </div>
            <p className="text-muted-foreground">{e.summary}</p>
            <p className="text-xs text-muted-foreground">
              {e.actor}
              {e.auditNo ? ` · ${e.auditNo}` : ""}
            </p>
          </li>
        ))}
        {conn.auditEvents.length === 0 ? (
          <BusinessEmptyState
            kind="no-data"
            title="暂无审计事件"
            description="配置与确认动作会追加审计号。"
          />
        ) : null}
      </ul>
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
            系统管理员独立命令；校验连接与能力版本。变更后能力标记为未验证，不复用采购确认写入口。
          </DialogDescription>
        </DialogHeader>
        <div className="max-h-72 space-y-2 overflow-y-auto">
          {conn.capabilities.map((c) => (
            <label
              key={c.capabilityCode}
              className="flex items-center justify-between gap-2 rounded-lg border px-3 py-2 text-sm"
            >
              <span>
                {c.capabilityLabel}
                <span className="ml-2 font-mono text-xs text-muted-foreground">
                  {c.capabilityCode}
                </span>
              </span>
              <input
                type="checkbox"
                checked={draft[c.capabilityCode] ?? false}
                onChange={(e) =>
                  setDraft((d) => ({
                    ...d,
                    [c.capabilityCode]: e.target.checked,
                  }))
                }
                aria-label={`启用 ${c.capabilityLabel}`}
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
