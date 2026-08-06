"use client"

import * as React from "react"
import { usePathname, useRouter, useSearchParams } from "next/navigation"
import {
  ChevronDownIcon,
  DownloadIcon,
  EyeIcon,
  FilterIcon,
  LockIcon,
  MoreHorizontalIcon,
  PlusIcon,
  SearchIcon,
  ShieldAlertIcon,
  ShieldOffIcon,
  Trash2Icon,
  TriangleAlertIcon,
  XIcon,
} from "lucide-react"
import type { ColumnDef, PaginationState } from "@tanstack/react-table"
import { z } from "zod"

import {
  BatchImpactPreview,
  BusinessDiffPanel,
  BusinessEmptyState,
  BusinessFailureState,
  BusinessStatusBadge,
  BusinessTableFrame,
  DataFreshness,
  DataTable,
  FormalActionResult,
  ListToolbar,
  OptionCombobox,
  PageHeader,
  QuickPreviewSheet,
} from "@/components/business"
import { formatDateTime } from "@/lib/datetime"
import { patchUrl as patchSearchParams } from "@/lib/patch-search-params"
import { type ResultState } from "@/components/business/feedback"
import { useAppForm } from "@/components/form"
import {
  Alert,
  AlertDescription,
  AlertTitle,
} from "@/components/ui/alert"
import { Badge } from "@/components/ui/badge"
import { Button } from "@/components/ui/button"
import {
  Collapsible,
  CollapsibleContent,
  CollapsibleTrigger,
} from "@/components/ui/collapsible"
import {
  Dialog,
  DialogContent,
  DialogDescription,
  DialogFooter,
  DialogHeader,
  DialogTitle,
} from "@/components/ui/dialog"
import {
  DropdownMenu,
  DropdownMenuContent,
  DropdownMenuItem,
  DropdownMenuSeparator,
  DropdownMenuTrigger,
} from "@/components/ui/dropdown-menu"
import {
  InputGroup,
  InputGroupAddon,
  InputGroupInput,
} from "@/components/ui/input-group"
import { Input } from "@/components/ui/input"
import { Label } from "@/components/ui/label"
import {
  Popover,
  PopoverContent,
  PopoverTrigger,
} from "@/components/ui/popover"
import { Separator } from "@/components/ui/separator"
import { Tabs, TabsList, TabsTrigger } from "@/components/ui/tabs"
import { Textarea } from "@/components/ui/textarea"
import {
  useAccessListQuery,
  useAuditEventQuery,
  useEffectiveAccessQuery,
  usePreviewAccessChangeMutation,
  useSubmitAccessChangeMutation,
} from "@/features/access-audit/queries"
import {
  AccountFormDialog,
  type AccountDraft,
} from "@/features/admin/account-form-dialog"
import { DeleteAdminDialog } from "@/features/admin/delete-admin-dialog"
import { DeleteRoleDialog } from "@/features/admin/delete-role-dialog"
import { useAssignableRolesQuery } from "@/features/admin/queries"
import type {
  AccessChangeCommand,
  AccessChangeOutcome,
  AccessEmptyReason,
  AccessGovernancePolicyView,
  AccessImpactPreview,
  AccessListQuery,
  AccessView,
  AuditEventRow,
  FieldPolicyRow,
  RoleRow,
  ScopeRow,
  UserRow,
} from "@/features/access-audit/types"
import {
  ACCESS_LAYER_HELP,
  ACCESS_VIEW_LABEL,
} from "@/features/access-audit/types"
import { resultText } from "@/lib/ui-text"

function parseView(raw: string | null): AccessView {
  // 字段策略无后端资源（backend_gap），入口隐藏；旧 URL 回退到 roles
  if (
    raw === "roles" ||
    raw === "users" ||
    raw === "scopes" ||
    raw === "audit"
  ) {
    return raw
  }
  return "roles"
}

function riskLabel(flag: string) {
  const map: Record<string, string> = {
    HIGH_PRIVILEGE: "高权限",
    EMPTY_SCOPE: "空数据范围",
    EXPIRING_SOON: "即将过期",
    ACCESS_ADMIN: "权限管理",
    PENDING_DISABLE: "待停用",
    REVOKED: "已撤权",
  }
  return map[flag] ?? flag
}

const changeReasonSchema = z.object({
  reasonCode: z.string().min(1, "请选择变更原因"),
  comment: z.string().trim().max(200),
})

function policyStatusLabel(state: "MISSING" | "CONFIGURED") {
  return state === "MISSING" ? "未配置" : "已配置"
}

function PolicyBanner({
  policies,
  view,
}: {
  policies: AccessGovernancePolicyView
  view: AccessView
}) {
  const time = policies.userRoleTimePolicy
  const field = policies.fieldPolicyGranularity
  const audit = policies.auditAccessPolicy
  const hasMissing =
    time.state === "MISSING" ||
    field.state === "MISSING" ||
    audit.state === "MISSING"

  const summaryItems: { key: string; label: string; missing: boolean }[] = []
  if (view === "users" || view === "roles") {
    summaryItems.push({
      key: "time",
      label: `角色时间 · ${policyStatusLabel(time.state)}`,
      missing: time.state === "MISSING",
    })
  }
  if (view === "fields" || view === "roles") {
    summaryItems.push({
      key: "field",
      label: `字段粒度 · ${policyStatusLabel(field.state)}`,
      missing: field.state === "MISSING",
    })
  }
  if (view === "audit" || view === "roles" || view === "users") {
    summaryItems.push({
      key: "audit",
      label: `审计导出 · ${policyStatusLabel(audit.state)}`,
      missing: audit.state === "MISSING",
    })
  }

  return (
    <Collapsible
      data-slot="policy-banner"
      className="rounded-xl border border-border bg-card"
    >
      <CollapsibleTrigger className="group flex w-full items-center gap-2 px-3 py-2 text-left text-sm hover:bg-muted/40">
        <ShieldAlertIcon
          className={
            hasMissing
              ? "size-4 shrink-0 text-warning"
              : "size-4 shrink-0 text-muted-foreground"
          }
          aria-hidden="true"
        />
        <span className="min-w-0 flex-1">
          <span className="font-medium text-foreground">治理策略</span>
          <span className="ml-2 inline-flex flex-wrap items-center gap-1.5 align-middle">
            {summaryItems.map((item) => (
              <Badge
                key={item.key}
                variant={item.missing ? "warning" : "outline"}
              >
                {item.label}
              </Badge>
            ))}
            <Badge variant="outline">本期无任务流</Badge>
          </span>
        </span>
        <span className="shrink-0 text-xs text-muted-foreground group-aria-expanded:hidden">
          详情
        </span>
        <ChevronDownIcon
          aria-hidden="true"
          className="size-4 shrink-0 text-muted-foreground transition-transform group-aria-expanded:rotate-180"
        />
      </CollapsibleTrigger>
      <CollapsibleContent className="border-t border-border px-3 py-2 text-xs text-muted-foreground">
        <div className="grid gap-x-4 gap-y-1.5 sm:grid-cols-2">
          {(view === "users" || view === "roles") && (
            <p>
              <strong className="text-foreground">用户角色时间：</strong>
              {time.state === "MISSING" ? (
                <>未配置 · 仅允许立即紧急撤权</>
              ) : (
                <>
                  预约 {time.schedulingAllowed ? "允许" : "禁用"} · 到期
                  {time.expirationAllowed ? "允许" : "禁用"}
                </>
              )}
            </p>
          )}
          {(view === "fields" || view === "roles") && (
            <p>
              <strong className="text-foreground">字段粒度：</strong>
              {field.state === "MISSING" ? (
                <>未配置 · 只读，不可自由输入字段名</>
              ) : (
                <>{field.editableTargets.map((t) => t.label).join("、")}</>
              )}
            </p>
          )}
          {(view === "audit" || view === "roles" || view === "users") && (
            <p>
              <strong className="text-foreground">审计 / 导出：</strong>
              {audit.state === "MISSING" ? (
                <>
                  未配置 · 保守窗口{" "}
                  {formatDateTime(audit.fallbackFrom, "full")} ~{" "}
                  {formatDateTime(audit.fallbackTo, "full")}，导出禁用
                </>
              ) : (
                <>
                  最长可查{" "}
                  {Math.round(audit.maxOnlineWindowSeconds / 3600)} 小时
                </>
              )}
            </p>
          )}
          <p className="sm:col-span-2">
            {ACCESS_LAYER_HELP.map((item) => item.title).join(" · ")}
            。命中复核要求的动作，在复核策略确定前将被阻断。
          </p>
        </div>
      </CollapsibleContent>
    </Collapsible>
  )
}

function EmptyByReason({
  reason,
  onClearFilters,
}: {
  reason: AccessEmptyReason
  onClearFilters?: () => void
}) {
  switch (reason) {
    case "NO_MODULE_PERMISSION":
      return (
        <BusinessEmptyState
          kind="no-scope"
          title="无模块权限"
          description="当前账号不能进入「权限与审计」。正常情况下导航入口应隐藏；这与「无数据范围」或「范围内无记录」不同。"
        />
      )
    case "NO_DATA_SCOPE":
      return (
        <BusinessEmptyState
          kind="no-scope"
          title="无数据范围"
          description="你可以进入本页，但当前管理范围内没有任何可配置主体。请查看管理范围或申请授权——不是筛选过严。"
        />
      )
    case "NO_RECORDS_IN_SCOPE":
      return (
        <BusinessEmptyState
          kind="no-data"
          title="范围内无记录"
          description="管理范围有效，但当前视图下没有可展示的记录。可调整时间范围、清除筛选，或（有权时）创建配置。"
        />
      )
    case "FIELD_MASKED":
      return (
        <BusinessEmptyState
          kind="no-data"
          title="字段级打码（非空列表）"
          description="列表与标签保留，敏感值按字段策略打码显示。权限管理员不会因为能配置权限而自动看到业务敏感正文。"
        />
      )
    case "FILTER_NO_RESULT":
    default:
      return (
        <BusinessEmptyState
          kind="filter"
          title="当前筛选无结果"
          description="没有记录符合当前条件。可清除筛选后重试。"
          action={
            onClearFilters ? (
              <Button type="button" size="sm" onClick={onClearFilters}>
                清除筛选
              </Button>
            ) : null
          }
        />
      )
  }
}

export function AccessAuditPage() {
  const router = useRouter()
  const pathname = usePathname()
  const searchParams = useSearchParams()

  const view = parseView(searchParams.get("view"))
  const qParam = searchParams.get("q") ?? ""
  const status = searchParams.get("status") ?? undefined
  const org = searchParams.get("org") ?? undefined
  const risk = searchParams.get("risk") ?? undefined
  const subjectTypeParam = searchParams.get("subjectType") ?? undefined
  const subjectIdParam = searchParams.get("subjectId") ?? undefined
  const eventIdParam = searchParams.get("eventId") ?? undefined
  const fromParam = searchParams.get("from") ?? undefined
  const toParam = searchParams.get("to") ?? undefined
  const actorId = searchParams.get("actorId") ?? undefined
  const action = searchParams.get("action") ?? undefined
  const objectType = searchParams.get("objectType") ?? undefined
  const objectId = searchParams.get("objectId") ?? undefined
  const resultFilter = searchParams.get("result") ?? undefined
  const traceId = searchParams.get("traceId") ?? undefined

  const [searchInput, setSearchInput] = React.useState(qParam)
  const searchInputRef = React.useRef<HTMLInputElement | null>(null)
  const [pagination, setPagination] = React.useState<PaginationState>({
    pageIndex: 0,
    pageSize: 20,
  })

  const [explainSubject, setExplainSubject] = React.useState<{
    type: "ROLE" | "USER"
    id: string
  } | null>(
    subjectIdParam &&
      (view === "roles" || view === "users" || view === "scopes")
      ? {
          type:
            subjectTypeParam === "USER" || view === "users"
              ? "USER"
              : "ROLE",
          id: subjectIdParam,
        }
      : null
  )
  const [eventOpenId, setEventOpenId] = React.useState<string | null>(
    eventIdParam ?? null
  )
  const [changeOpen, setChangeOpen] = React.useState(false)
  const [pendingCommand, setPendingCommand] =
    React.useState<AccessChangeCommand | null>(null)
  const [impact, setImpact] = React.useState<AccessImpactPreview | null>(null)
  const [lastResult, setLastResult] = React.useState<ResultState>(null)
  const [actionError, setActionError] = React.useState<string | null>(null)
  // 账号新建 / 编辑弹窗（账号字段少，弹窗足够；角色编辑走整页表单）
  const [accountForm, setAccountForm] = React.useState<{
    mode: "create" | "edit"
    account: AccountDraft | null
  } | null>(null)
  const [deletingAccount, setDeletingAccount] = React.useState<{
    id: string
    account: string
  } | null>(null)
  const [deletingRole, setDeletingRole] = React.useState<{
    id: string
    name: string
  } | null>(null)
  const idempotencyRef = React.useRef<string | null>(null)
  const rowFocusRef = React.useRef<Map<string, HTMLButtonElement | null>>(
    new Map()
  )
  const restoreFocusIdRef = React.useRef<string | null>(null)

  const restoreRowFocus = React.useCallback(() => {
    const id = restoreFocusIdRef.current
    if (!id) return
    window.requestAnimationFrame(() => {
      const element = rowFocusRef.current.get(id)
      if (!element) return
      element.focus()
      restoreFocusIdRef.current = null
    })
  }, [])

  React.useEffect(() => {
    setSearchInput(qParam)
  }, [qParam])

  React.useEffect(() => {
    setEventOpenId(eventIdParam ?? null)
  }, [eventIdParam])

  React.useEffect(() => {
    if (subjectIdParam) {
      setExplainSubject({
        type:
          subjectTypeParam === "USER" || view === "users" ? "USER" : "ROLE",
        id: subjectIdParam,
      })
    }
  }, [subjectIdParam, subjectTypeParam, view])

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
      searchInputRef.current?.focus()
    }
    window.addEventListener("keydown", onKey)
    return () => window.removeEventListener("keydown", onKey)
  }, [])

  function patchUrl(
    patch: Record<string, string | null | undefined>,
    options?: { replace?: boolean }
  ) {
    patchSearchParams({ router, pathname, searchParams, view }, patch, options)
  }

  /** 筛选变更统一回到第一页：避免第二页起筛选变窄后「共 N 条」与空态并存。 */
  const patchFilterUrl = (
    patch: Record<string, string | null | undefined>
  ) => {
    patchUrl(patch)
    setPagination((p) => (p.pageIndex === 0 ? p : { ...p, pageIndex: 0 }))
  }

  React.useEffect(() => {
    const handle = globalThis.setTimeout(() => {
      if (searchInput === qParam) return
      patchUrl({ q: searchInput.trim() || null }, { replace: true })
      setPagination((p) => (p.pageIndex === 0 ? p : { ...p, pageIndex: 0 }))
    }, 300)
    return () => globalThis.clearTimeout(handle)
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [searchInput])

  // 高级筛选输入防抖：不逐键发请求
  const [debouncedFilters, setDebouncedFilters] = React.useState<{
    actorId?: string
    traceId?: string
    objectType?: string
    objectId?: string
  }>({})
  const lastPatchedFilters = React.useRef("")
  React.useEffect(() => {
    const handle = globalThis.setTimeout(() => {
      const next = debouncedFilters
      const key = JSON.stringify(next)
      if (key === lastPatchedFilters.current) return
      lastPatchedFilters.current = key
      patchFilterUrl({
        actorId: next.actorId?.trim() || null,
        traceId: next.traceId?.trim() || null,
        objectType: next.objectType?.trim() || null,
        objectId: next.objectId?.trim() || null,
      })
    }, 300)
    return () => globalThis.clearTimeout(handle)
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [debouncedFilters])

  const listQuery: AccessListQuery = React.useMemo(
    () => ({
      view,
      q: qParam || undefined,
      status,
      org,
      risk,
      // 详情参数只驱动详情请求：subjectId/subjectType 仅数据范围视图参与列表筛选，
      // eventId 不进列表查询（避免打开详情背后列表闪变）。
      subjectType: view === "scopes" ? subjectTypeParam : undefined,
      subjectId: view === "scopes" ? subjectIdParam : undefined,
      from: fromParam,
      to: toParam,
      actorId,
      action,
      objectType,
      objectId,
      result: resultFilter,
      traceId,
      eventId: undefined,
    }),
    [
      view,
      qParam,
      status,
      org,
      risk,
      subjectTypeParam,
      subjectIdParam,
      fromParam,
      toParam,
      actorId,
      action,
      objectType,
      objectId,
      resultFilter,
      traceId,
    ]
  )

  const pageQuery = useAccessListQuery(listQuery)
  const effectiveQuery = useEffectiveAccessQuery(
    explainSubject?.type ?? null,
    explainSubject?.id ?? null
  )
  const eventQuery = useAuditEventQuery(eventOpenId)
  const previewMutation = usePreviewAccessChangeMutation()
  const submitMutation = useSubmitAccessChangeMutation()
  // 账号表单角色选项：仅当前操作者可分配的角色（API 层失败时回落全部角色）
  const assignableRolesQuery = useAssignableRolesQuery()

  const data = pageQuery.data
  const policies = data?.governancePolicies

  const form = useAppForm({
    defaultValues: {
      reasonCode: "SECURITY_OPS",
      comment: "",
    },
    validators: {
      onChange: changeReasonSchema,
    },
    onSubmit: async () => {
      // 确认在影响预览 Dialog 内提交
    },
  })

  const openExplain = React.useCallback(
    (type: "ROLE" | "USER", id: string) => {
      restoreFocusIdRef.current = id
      setExplainSubject({ type, id })
      patchUrl(
        {
          subjectType: type,
          subjectId: id,
          eventId: null,
        },
        { replace: true }
      )
    },
    // eslint-disable-next-line react-hooks/exhaustive-deps
    [searchParams, pathname, view]
  )

  const closeExplain = React.useCallback(() => {
    setExplainSubject(null)
    patchUrl({ subjectId: null, subjectType: null }, { replace: true })
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [searchParams, pathname, view])

  const openEvent = React.useCallback(
    (eventId: string) => {
      restoreFocusIdRef.current = eventId
      setEventOpenId(eventId)
      patchUrl({ eventId }, { replace: true })
    },
    // eslint-disable-next-line react-hooks/exhaustive-deps
    [searchParams, pathname, view]
  )

  const closeEvent = React.useCallback(() => {
    setEventOpenId(null)
    patchUrl({ eventId: null }, { replace: true })
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [searchParams, pathname, view])

  const startChange = React.useCallback(
    async (command: AccessChangeCommand) => {
      setActionError(null)
      setLastResult(null)
      setImpact(null)
      idempotencyRef.current = null
      try {
        const preview = await previewMutation.mutateAsync(command)
        setPendingCommand(command)
        setImpact(preview)
        form.reset()
        setChangeOpen(true)
      } catch (err) {
        setActionError(err instanceof Error ? err.message : "影响预览失败")
      }
    },
    [previewMutation, form]
  )

  const applyOutcome = React.useCallback((outcome: AccessChangeOutcome) => {
    if (outcome.outcome === "CONFIRMED") {
      setLastResult({
        status: "succeeded",
        title: "授权变更已生效",
        description: outcome.message,
        reference: outcome.reference,
        facts: [
          {
            label: "配置版本",
            value: `v${outcome.permissionVersion.split("-").at(-1)}`,
          },
          {
            label: "影响主体数",
            value: String(outcome.affectedSubjectCount),
          },
          { label: "审计事件号", value: outcome.auditEventId },
          { label: "生效时间", value: formatDateTime(outcome.effectiveAt, "full") },
          {
            label: "下一步",
            value: outcome.nextSteps.join("；"),
          },
        ],
      })
      return
    }
    if (outcome.outcome === "REJECTED") {
      setLastResult({
        status:
          outcome.code === "REVIEW_POLICY_UNCONFIGURED"
            ? "blocked"
            : "rejected",
        title:
          outcome.code === "REVIEW_POLICY_UNCONFIGURED"
            ? "复核策略未确定，动作已阻断"
            : "授权变更被拒绝",
        description: outcome.message,
        facts: outcome.actionBlockers?.map((b) => ({
          label: b.code,
          value: b.message,
        })),
      })
      return
    }
    if (outcome.outcome === "CONFLICT") {
      setLastResult({
        status: "blocked",
        title: "权限已更新",
        description: outcome.message,
        facts: [
          {
            label: "当前版本",
            value: outcome.serverPermissionVersion,
          },
        ],
      })
      return
    }
    setLastResult({
      status: "unknown",
      title: resultText.unknown,
      description: outcome.message,
      pendingIdempotencyKey: outcome.idempotencyKey,
    })
  }, [])

  const confirmChange = React.useCallback(async () => {
    if (!pendingCommand || !impact) return
    if (impact.reviewPolicyBlocker) {
      applyOutcome({
        outcome: "REJECTED",
        code: impact.reviewPolicyBlocker.code,
        message: impact.reviewPolicyBlocker.message,
        actionBlockers: [impact.reviewPolicyBlocker],
      })
      setChangeOpen(false)
      return
    }

    if (!idempotencyRef.current) {
      idempotencyRef.current = `w19-${pendingCommand.action}-${Date.now()}`
    }
    const values = form.state.values
    const command: AccessChangeCommand = {
      ...pendingCommand,
      reasonCode: values.reasonCode,
      comment: values.comment?.trim() || undefined,
      idempotencyKey: idempotencyRef.current,
    }
    try {
      const outcome = await submitMutation.mutateAsync(command)
      applyOutcome(outcome)
      setChangeOpen(false)
      setPendingCommand(null)
      setImpact(null)
    } catch (err) {
      setActionError(err instanceof Error ? err.message : "提交失败")
    }
  }, [
    pendingCommand,
    impact,
    form,
    submitMutation,
    applyOutcome,
  ])

  const clearFilters = React.useCallback(() => {
    patchUrl({
      q: null,
      status: null,
      org: null,
      risk: null,
      actorId: null,
      action: null,
      objectType: null,
      objectId: null,
      result: null,
      traceId: null,
      from: null,
      to: null,
    })
    setSearchInput("")
    setDebouncedFilters({})
    lastPatchedFilters.current = ""
    setPagination((p) => ({ ...p, pageIndex: 0 }))
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [searchParams, pathname, view])

  const exportBlocked =
    !data?.allowedActions.includes("EXPORT_AUDIT") &&
    !data?.allowedActions.includes("EXPORT_CONFIGURATION")
  const exportBlocker = data?.actionBlockers.find(
    (b) =>
      b.action === "EXPORT_AUDIT" || b.action === "EXPORT_CONFIGURATION"
  )

  // ── columns ──
  const roleColumns = React.useMemo<ColumnDef<RoleRow>[]>(
    () => [
      {
        id: "identity",
        header: "角色",
        cell: ({ row }) => (
          <div className="min-w-[10rem]">
            <div className="font-medium">{row.original.name}</div>
            <div className="font-mono text-xs text-muted-foreground">
              {row.original.roleCode}
            </div>
          </div>
        ),
      },
      {
        id: "org",
        header: "组织",
        cell: ({ row }) => row.original.organizationLabel,
      },
      {
        id: "perms",
        header: "模块与动作权限",
        cell: ({ row }) => (
          <span className="text-sm text-muted-foreground">
            {row.original.permissionSummary}
          </span>
        ),
      },
      {
        id: "scope",
        header: "数据范围",
        cell: ({ row }) => row.original.dataScopeSummary,
      },
      {
        id: "status",
        header: "状态",
        cell: ({ row }) => (
          <BusinessStatusBadge
            label={row.original.statusLabel}
            tone={row.original.statusTone}
          />
        ),
      },
      {
        id: "version",
        header: "版本",
        cell: ({ row }) => (
          <span className="num text-xs">
            v{row.original.permissionVersion.split("-").at(-1)}
          </span>
        ),
      },
      {
        id: "risk",
        header: "风险",
        cell: ({ row }) =>
          row.original.riskFlags.length ? (
            <div className="flex flex-wrap gap-1">
              {row.original.riskFlags.map((f) => (
                <Badge key={f} variant="warning">
                  {riskLabel(f)}
                </Badge>
              ))}
            </div>
          ) : (
            "—"
          ),
      },
      {
        id: "actions",
        header: "操作",
        cell: ({ row }) => {
          const role = row.original
          const version =
            data?.permissionVersion ?? role.permissionVersion
          const canAdjust =
            role.status === "enabled" &&
            !role.riskFlags.includes("HIGH_PRIVILEGE")
          const canExpand = role.riskFlags.includes("HIGH_PRIVILEGE")
          const canDisable =
            role.status === "enabled" &&
            role.riskFlags.includes("PENDING_DISABLE")

          return (
            <div className="flex items-center justify-end gap-1">
              <Button
                type="button"
                size="xs"
                variant="ghost"
                ref={(el) => {
                  rowFocusRef.current.set(role.id, el)
                }}
                onClick={() => openExplain("ROLE", role.id)}
              >
                <EyeIcon data-icon="inline-start" aria-hidden="true" />
                有效权限
              </Button>
              <Button
                type="button"
                size="xs"
                variant="outline"
                onClick={() => router.push(`/system/roles/${role.id}/edit`)}
              >
                编辑
              </Button>
              <DropdownMenu>
                <DropdownMenuTrigger
                  render={
                    <Button
                      type="button"
                      size="icon-xs"
                      variant="ghost"
                      aria-label={`${role.name} 更多操作`}
                    />
                  }
                >
                  <MoreHorizontalIcon aria-hidden="true" />
                </DropdownMenuTrigger>
                <DropdownMenuContent align="end" className="min-w-40">
                  {canAdjust ? (
                    <DropdownMenuItem
                      onClick={() =>
                        void startChange({
                          subjectType: "ROLE",
                          subjectId: role.id,
                          action: "UPDATE_ROLE_PERMISSIONS",
                          expectedPermissionVersion: version,
                          reasonCode: "SECURITY_OPS",
                          idempotencyKey: "pending",
                          changeSet: [
                            {
                              targetReference: "W22.publish",
                              operation: "REMOVE",
                            },
                          ],
                        })
                      }
                    >
                      调整权限
                    </DropdownMenuItem>
                  ) : null}
                  {canExpand ? (
                    <DropdownMenuItem
                      onClick={() =>
                        void startChange({
                          subjectType: "ROLE",
                          subjectId: role.id,
                          action: "UPDATE_ROLE_PERMISSIONS",
                          expectedPermissionVersion: version,
                          reasonCode: "SECURITY_OPS",
                          idempotencyKey: "pending",
                          changeSet: [
                            {
                              targetReference: "sensitive.field.expand",
                              operation: "ADD",
                              valueReference: "FULL_COMPANY",
                            },
                          ],
                        })
                      }
                    >
                      扩权（将阻断）
                    </DropdownMenuItem>
                  ) : null}
                  {canDisable ? (
                    <DropdownMenuItem
                      onClick={() =>
                        void startChange({
                          subjectType: "ROLE",
                          subjectId: role.id,
                          action: "DISABLE_ROLE",
                          expectedPermissionVersion: version,
                          reasonCode: "SECURITY_OPS",
                          idempotencyKey: "pending",
                          changeSet: [
                            {
                              targetReference: "status",
                              operation: "REPLACE",
                              valueReference: "disabled",
                            },
                          ],
                        })
                      }
                    >
                      停用
                    </DropdownMenuItem>
                  ) : null}
                  {(canAdjust || canExpand || canDisable) && (
                    <DropdownMenuSeparator />
                  )}
                  <DropdownMenuItem
                    variant="destructive"
                    onClick={() =>
                      setDeletingRole({ id: role.id, name: role.name })
                    }
                  >
                    <Trash2Icon aria-hidden="true" />
                    删除
                  </DropdownMenuItem>
                </DropdownMenuContent>
              </DropdownMenu>
            </div>
          )
        },
      },
    ],
    [openExplain, startChange, router, data?.permissionVersion]
  )

  const userColumns = React.useMemo<ColumnDef<UserRow>[]>(
    () => [
      {
        id: "identity",
        header: "用户",
        cell: ({ row }) => (
          <div className="min-w-[9rem]">
            <div className="font-medium">{row.original.displayName}</div>
            <div className="font-mono text-xs text-muted-foreground">
              {row.original.userId}
            </div>
          </div>
        ),
      },
      {
        id: "roles",
        header: "当前角色",
        cell: ({ row }) => row.original.activeRoles,
      },
      {
        id: "period",
        header: "有效期间",
        cell: ({ row }) => (
          <span
            className="num text-xs text-muted-foreground"
            title="只读记录；策略未配置时不可编辑预约/到期"
          >
            {formatDateTime(row.original.effectiveFrom, "full")}
            {" ~ "}
            {row.original.effectiveTo
              ? formatDateTime(row.original.effectiveTo, "full")
              : "长期"}
          </span>
        ),
      },
      {
        id: "scope",
        header: "数据范围",
        cell: ({ row }) => row.original.dataScopeSummary,
      },
      {
        id: "status",
        header: "账号状态",
        cell: ({ row }) => (
          <BusinessStatusBadge
            label={row.original.statusLabel}
            tone={row.original.statusTone}
          />
        ),
      },
      {
        id: "risk",
        header: "风险",
        cell: ({ row }) =>
          row.original.riskFlags.length ? (
            <div className="flex flex-wrap gap-1">
              {row.original.riskFlags.map((f) => (
                <Badge key={f} variant="warning">
                  {riskLabel(f)}
                </Badge>
              ))}
            </div>
          ) : (
            "—"
          ),
      },
      {
        id: "actions",
        header: "操作",
        cell: ({ row }) => {
          const user = row.original
          return (
            <div className="flex items-center justify-end gap-1">
              <Button
                type="button"
                size="xs"
                variant="ghost"
                ref={(el) => {
                  rowFocusRef.current.set(user.id, el)
                }}
                onClick={() => openExplain("USER", user.userId)}
              >
                <EyeIcon data-icon="inline-start" aria-hidden="true" />
                有效权限
              </Button>
              <Button
                type="button"
                size="xs"
                variant="outline"
                onClick={() =>
                  setAccountForm({
                    mode: "edit",
                    account: {
                      id: user.userId,
                      account: user.accountName,
                      name: user.displayName,
                      role_ids: [...user.roleIds],
                    },
                  })
                }
              >
                编辑
              </Button>
              <DropdownMenu>
                <DropdownMenuTrigger
                  render={
                    <Button
                      type="button"
                      size="icon-xs"
                      variant="ghost"
                      aria-label={`${user.displayName} 更多操作`}
                    />
                  }
                >
                  <MoreHorizontalIcon aria-hidden="true" />
                </DropdownMenuTrigger>
                <DropdownMenuContent align="end" className="min-w-40">
                  {user.roleAssignmentId ? (
                    <>
                      <DropdownMenuItem
                        variant="destructive"
                        onClick={() =>
                          void startChange({
                            subjectType: "USER",
                            subjectId: user.userId,
                            action: "EMERGENCY_REVOKE_USER_ROLE",
                            roleAssignmentId: user.roleAssignmentId!,
                            expectedPermissionVersion:
                              data?.permissionVersion ??
                              user.permissionVersion,
                            reasonCode: "EMERGENCY_STOP_LOSS",
                            idempotencyKey: "pending",
                          })
                        }
                      >
                        <ShieldOffIcon aria-hidden="true" />
                        紧急撤权
                      </DropdownMenuItem>
                      <DropdownMenuSeparator />
                    </>
                  ) : null}
                  <DropdownMenuItem
                    variant="destructive"
                    onClick={() =>
                      setDeletingAccount({
                        id: user.userId,
                        account: user.accountName,
                      })
                    }
                  >
                    <Trash2Icon aria-hidden="true" />
                    删除
                  </DropdownMenuItem>
                </DropdownMenuContent>
              </DropdownMenu>
            </div>
          )
        },
      },
    ],
    [openExplain, startChange, data?.permissionVersion]
  )

  const scopeColumns = React.useMemo<ColumnDef<ScopeRow>[]>(
    () => [
      {
        id: "subject",
        header: "主体",
        cell: ({ row }) => (
          <div>
            <div className="font-medium">{row.original.subjectLabel}</div>
            <div className="text-xs text-muted-foreground">
              {row.original.subjectType === "ROLE" ? "角色" : "用户"}
            </div>
          </div>
        ),
      },
      {
        id: "type",
        header: "范围类型",
        cell: ({ row }) => row.original.scopeTypeLabel,
      },
      {
        id: "targets",
        header: "范围对象",
        cell: ({ row }) => row.original.scopeTargets,
      },
      {
        id: "risk",
        header: "风险",
        cell: ({ row }) =>
          row.original.riskFlags.length
            ? row.original.riskFlags.map((f) => riskLabel(f)).join("、")
            : "—",
      },
      {
        id: "actions",
        header: "操作",
        cell: ({ row }) => (
          <div className="flex justify-end">
            <Button
              type="button"
              size="xs"
              variant="ghost"
              ref={(el) => {
                rowFocusRef.current.set(row.original.id, el)
              }}
              onClick={() =>
                openExplain(row.original.subjectType, row.original.subjectId)
              }
            >
              <EyeIcon data-icon="inline-start" aria-hidden="true" />
              有效权限
            </Button>
          </div>
        ),
      },
    ],
    [openExplain]
  )

  const fieldColumns = React.useMemo<ColumnDef<FieldPolicyRow>[]>(
    () => [
      {
        id: "target",
        header: "策略目标",
        cell: ({ row }) => (
          <div>
            <div className="font-medium">{row.original.targetLabel}</div>
            <div className="font-mono text-xs text-muted-foreground">
              {row.original.policyTargetId}
            </div>
          </div>
        ),
      },
      {
        id: "subject",
        header: "适用",
        cell: ({ row }) => row.original.subjectLabel,
      },
      {
        id: "caps",
        header: "访问能力",
        cell: ({ row }) =>
          data?.emptyReason === "FIELD_MASKED"
            ? "****"
            : row.original.capabilitySummary,
      },
      {
        id: "mode",
        header: "可编辑",
        cell: ({ row }) =>
          row.original.editable ? (
            <Badge variant="success">可调整</Badge>
          ) : (
            <Badge variant="default">只读</Badge>
          ),
      },
      {
        id: "actions",
        header: "操作",
        cell: ({ row }) =>
          row.original.editable ? (
            <Button
              type="button"
              size="sm"
              variant="outline"
              onClick={() => {
                const gp = policies?.fieldPolicyGranularity
                if (!gp || gp.state !== "CONFIGURED") return
                void startChange({
                  subjectType: "FIELD_POLICY",
                  subjectId: row.original.id,
                  action: "UPDATE_FIELD_POLICY",
                  granularityPolicyVersion: gp.policyVersion,
                  policyTargetId: row.original.policyTargetId,
                  accessCapabilities: ["MASKED", "VISIBLE"],
                  expectedPermissionVersion:
                    data?.permissionVersion ?? row.original.permissionVersion,
                  reasonCode: "SECURITY_OPS",
                  idempotencyKey: "pending",
                })
              }}
            >
              调整能力
            </Button>
          ) : (
            <span className="text-xs text-muted-foreground">策略缺失时只读</span>
          ),
      },
    ],
    [data?.emptyReason, data?.permissionVersion, policies, startChange]
  )

  const auditColumns = React.useMemo<ColumnDef<AuditEventRow>[]>(
    () => [
      {
        id: "time",
        header: "时间",
        cell: ({ row }) => (
          <span className="num text-xs">
            {formatDateTime(row.original.recordedAt, "full")}
          </span>
        ),
      },
      {
        id: "actor",
        header: "操作者",
        cell: ({ row }) => (
          <div className="min-w-[7rem]">
            <div className="font-medium">{row.original.actorLabel}</div>
            <div className="font-mono text-xs text-muted-foreground">
              {row.original.actorId}
            </div>
          </div>
        ),
      },
      {
        id: "role",
        header: "责任角色",
        cell: ({ row }) => (
          <span className="text-sm text-muted-foreground">
            {row.original.actorRole}
          </span>
        ),
      },
      {
        id: "action",
        header: "动作",
        cell: ({ row }) => row.original.actionLabel,
      },
      {
        id: "object",
        header: "对象",
        cell: ({ row }) => <span>{row.original.objectLabel}</span>,
      },
      {
        id: "result",
        header: "结果",
        cell: ({ row }) => (
          <BusinessStatusBadge
            label={row.original.resultLabel}
            tone={row.original.resultTone}
          />
        ),
      },
      {
        id: "fields",
        header: "变更字段",
        cell: ({ row }) => (
          <span className="text-sm">
            {row.original.changedFieldDisplay !== "—"
              ? row.original.changedFieldDisplay
              : "—"}
          </span>
        ),
      },
      {
        id: "trace",
        header: "请求追踪号",
        cell: ({ row }) => (
          <span className="font-mono text-xs">{row.original.traceId}</span>
        ),
      },
      {
        id: "actions",
        header: "查看",
        cell: ({ row }) => (
          <div className="flex justify-end">
            <Button
              type="button"
              size="xs"
              variant="outline"
              ref={(el) => {
                rowFocusRef.current.set(row.original.auditEventId, el)
              }}
              onClick={() => openEvent(row.original.auditEventId)}
            >
              详情
            </Button>
          </div>
        ),
      },
    ],
    [openEvent]
  )

  if (pageQuery.isPending) {
    return (
      <div className="mx-auto flex w-full max-w-shell flex-col gap-3 p-3 md:p-4">
        <div className="h-9 w-40 animate-pulse rounded-lg bg-muted" />
        <div className="h-9 animate-pulse rounded-lg bg-muted" />
        <div className="h-10 animate-pulse rounded-xl bg-muted" />
        <div className="h-[32rem] animate-pulse rounded-2xl bg-muted" />
      </div>
    )
  }

  if (pageQuery.isError || !data) {
    return (
      <div className="mx-auto flex w-full max-w-shell flex-col gap-3 p-3 md:p-4">
        <PageHeader title="权限与审计" />
        <BusinessFailureState
          kind="system"
          description="加载权限/审计数据失败。"
          action={
            <Button type="button" onClick={() => void pageQuery.refetch()}>
              重试
            </Button>
          }
        />
      </div>
    )
  }

  const isAudit = view === "audit"
  const rows =
    view === "roles"
      ? data.roles
      : view === "users"
        ? data.users
        : view === "scopes"
          ? data.scopes
          : view === "fields"
            ? data.fieldPolicies
            : data.auditEvents

  const pagedRows = rows.slice(
    pagination.pageIndex * pagination.pageSize,
    pagination.pageIndex * pagination.pageSize + pagination.pageSize
  )

  const switchView = (next: AccessView) => {
    if (next === view) return
    setPagination({ pageIndex: 0, pageSize: 20 })
    setExplainSubject(null)
    setEventOpenId(null)
    // 切换视图时保留 q，清空主体/事件与审计专属筛选
    patchUrl({
      view: next,
      subjectId: null,
      subjectType: null,
      eventId: null,
      ...(next === "audit"
        ? {}
        : {
            actorId: null,
            action: null,
            objectType: null,
            objectId: null,
            result: null,
            traceId: null,
            from: null,
            to: null,
          }),
    })
  }

  const advancedAuditActive = Boolean(
    actorId || traceId || objectType || objectId
  )
  const hasActiveFilters = isAudit
    ? Boolean(
        qParam ||
          action ||
          resultFilter ||
          fromParam ||
          toParam ||
          advancedAuditActive
      )
    : Boolean(qParam || status || risk || org)

  const handleExport = () => {
    if (exportBlocked) {
      setActionError(
        exportBlocker?.message ?? "导出策略未配置，导出已禁用。"
      )
      return
    }
    setLastResult({
      status: "blocked",
      title: "导出功能待接入",
      description:
        "导出尚未接入后端；正式环境将按权限策略生成导出文件。",
    })
  }

  const listToolbar = (
    <ListToolbar
      search={
        <InputGroup>
          <InputGroupAddon>
            <SearchIcon aria-hidden="true" />
          </InputGroupAddon>
          <InputGroupInput
            ref={searchInputRef}
            value={searchInput}
            onChange={(e) => setSearchInput(e.target.value)}
            placeholder={
              isAudit
                ? "操作者、动作、对象、追踪号"
                : "角色代码/名称、用户账号"
            }
            aria-label="搜索"
          />
        </InputGroup>
      }
      filters={
        <>
          {!isAudit ? (
            <>
              <OptionCombobox
                value={status ?? "all"}
                onValueChange={(v) =>
                  patchFilterUrl({
                    status: (v ?? "all") === "all" ? null : (v ?? "all"),
                  })
                }
                options={[
                  { value: "all", label: "全部状态" },
                  { value: "enabled", label: "启用" },
                  { value: "disabled", label: "停用" },
                ]}
                className="w-[8rem]"
                size="sm"
                allowClear={false}
                aria-label="状态"
                placeholder="全部状态"
              />
              <OptionCombobox
                value={risk ?? "all"}
                onValueChange={(v) =>
                  patchFilterUrl({
                    risk: (v ?? "all") === "all" ? null : (v ?? "all"),
                  })
                }
                options={[
                  { value: "all", label: "全部风险" },
                  { value: "HIGH_PRIVILEGE", label: "高权限" },
                  { value: "EMPTY_SCOPE", label: "空数据范围" },
                  { value: "EXPIRING_SOON", label: "即将过期" },
                  { value: "ACCESS_ADMIN", label: "权限管理" },
                ]}
                className="w-[9rem]"
                size="sm"
                allowClear={false}
                aria-label="权限风险"
                placeholder="全部风险"
              />
            </>
          ) : (
            <>
              <label className="flex items-center gap-1.5 text-xs text-muted-foreground">
                起始
                <Input
                  type="date"
                  className="h-8 w-36"
                  value={fromParam ?? ""}
                  onChange={(e) =>
                    patchFilterUrl({
                      from: e.target.value || null,
                    })
                  }
                  aria-label="审计起始日期"
                />
              </label>
              <label className="flex items-center gap-1.5 text-xs text-muted-foreground">
                截止
                <Input
                  type="date"
                  className="h-8 w-36"
                  value={toParam ?? ""}
                  min={fromParam}
                  onChange={(e) =>
                    patchFilterUrl({
                      to: e.target.value || null,
                    })
                  }
                  aria-label="审计截止日期"
                />
              </label>
              <OptionCombobox
                value={action ?? "all"}
                onValueChange={(v) =>
                  patchFilterUrl({
                    action: (v ?? "all") === "all" ? null : (v ?? "all"),
                  })
                }
                options={[
                  { value: "all", label: "全部动作" },
                  {
                    value: "UPDATE_ROLE_PERMISSIONS",
                    label: "修改模块权限",
                  },
                  {
                    value: "EMERGENCY_REVOKE_USER_ROLE",
                    label: "紧急撤权",
                  },
                  {
                    value: "UPDATE_FIELD_POLICY",
                    label: "修改字段策略",
                  },
                  {
                    value: "MANAGE_DATA_SCOPE",
                    label: "修改数据范围",
                  },
                  { value: "QUERY_AUDIT", label: "查询审计" },
                  { value: "OPEN_SUPPLIER", label: "打开供应商" },
                  { value: "EXPORT_RECEIVABLE", label: "导出应收明细" },
                  { value: "CREATE_ADJUSTMENT", label: "创建库存调整" },
                  {
                    value: "VIEW_CUSTOMER_SENSITIVE",
                    label: "短时揭示敏感字段",
                  },
                  {
                    value: "PERMISSION_VERSION_BUMP",
                    label: "权限版本推进",
                  },
                ]}
                className="w-[10rem]"
                size="sm"
                allowClear={false}
                aria-label="动作"
                placeholder="全部动作"
              />
              <OptionCombobox
                value={resultFilter ?? "all"}
                onValueChange={(v) =>
                  patchFilterUrl({
                    result: (v ?? "all") === "all" ? null : (v ?? "all"),
                  })
                }
                options={[
                  { value: "all", label: "全部结果" },
                  { value: "SUCCESS", label: "成功" },
                  { value: "DENIED", label: "拒绝" },
                  { value: "FAILED", label: "失败" },
                  { value: "UNKNOWN", label: "未知" },
                ]}
                className="w-[8rem]"
                size="sm"
                allowClear={false}
                aria-label="结果"
                placeholder="全部结果"
              />
              <Popover>
                <PopoverTrigger
                  render={
                    <Button type="button" variant="outline" size="sm" />
                  }
                >
                  <FilterIcon data-icon="inline-start" aria-hidden="true" />
                  高级筛选
                  {advancedAuditActive ? (
                    <Badge variant="info">已启用</Badge>
                  ) : null}
                </PopoverTrigger>
                <PopoverContent align="end" className="w-80 space-y-3">
                  <div>
                    <div className="font-medium">高级筛选</div>
                    <p className="mt-1 text-xs text-muted-foreground">
                      操作者、对象与请求追踪号。
                    </p>
                  </div>
                  <label className="grid gap-1.5 text-sm">
                    <span>操作者</span>
                    <InputGroup>
                      <InputGroupInput
                        value={debouncedFilters.actorId ?? actorId ?? ""}
                        onChange={(e) =>
                          setDebouncedFilters((prev) => ({
                            ...prev,
                            actorId: e.target.value,
                          }))
                        }
                        placeholder="操作者姓名或 ID"
                        aria-label="操作者"
                      />
                    </InputGroup>
                  </label>
                  <label className="grid gap-1.5 text-sm">
                    <span>请求追踪号</span>
                    <InputGroup>
                      <InputGroupInput
                        value={debouncedFilters.traceId ?? traceId ?? ""}
                        onChange={(e) =>
                          setDebouncedFilters((prev) => ({
                            ...prev,
                            traceId: e.target.value,
                          }))
                        }
                        placeholder="精确匹配"
                        aria-label="请求追踪号"
                      />
                    </InputGroup>
                  </label>
                  <label className="grid gap-1.5 text-sm">
                    <span>对象类型</span>
                    <InputGroup>
                      <InputGroupInput
                        value={
                          debouncedFilters.objectType ?? objectType ?? ""
                        }
                        onChange={(e) =>
                          setDebouncedFilters((prev) => ({
                            ...prev,
                            objectType: e.target.value,
                          }))
                        }
                        placeholder="如 role / sales_order"
                        aria-label="对象类型"
                      />
                    </InputGroup>
                  </label>
                  <label className="grid gap-1.5 text-sm">
                    <span>对象编号</span>
                    <InputGroup>
                      <InputGroupInput
                        value={debouncedFilters.objectId ?? objectId ?? ""}
                        onChange={(e) =>
                          setDebouncedFilters((prev) => ({
                            ...prev,
                            objectId: e.target.value,
                          }))
                        }
                        placeholder="对象名称或编号"
                        aria-label="对象名称或编号"
                      />
                    </InputGroup>
                  </label>
                  <Button
                    type="button"
                    variant="ghost"
                    size="sm"
                    disabled={!advancedAuditActive}
                    onClick={() => {
                      setDebouncedFilters({})
                      patchFilterUrl({
                        actorId: null,
                        traceId: null,
                        objectType: null,
                        objectId: null,
                      })
                    }}
                  >
                    清除高级筛选
                  </Button>
                </PopoverContent>
              </Popover>
            </>
          )}
          {hasActiveFilters ? (
            <Button
              type="button"
              size="sm"
              variant="ghost"
              onClick={clearFilters}
            >
              <XIcon data-icon="inline-start" aria-hidden="true" />
              清除筛选
            </Button>
          ) : null}
        </>
      }
      actions={
        <Button
          type="button"
          size="sm"
          variant="outline"
          disabled={exportBlocked}
          title={exportBlocker?.message}
          onClick={handleExport}
        >
          <DownloadIcon data-icon="inline-start" aria-hidden="true" />
          {isAudit ? "导出审计" : "导出配置"}
        </Button>
      }
    />
  )

  return (
    <div className="mx-auto flex w-full max-w-shell flex-col gap-3 p-3 md:p-4">
      <PageHeader
        title="权限与审计"
        description={
          isAudit
            ? "查询追加式审计事件；无记录不等于动作未发生。"
            : "配置角色、用户授权与数据范围，并查看有效权限来源。"
        }
        metadata={
          <div className="flex flex-wrap items-center gap-x-3 gap-y-1">
            <DataFreshness
              label={isAudit ? "审计更新时间" : "权限配置更新时间"}
              state={pageQuery.isFetching ? "syncing" : "fresh"}
              updatedAt={formatDateTime(data.calculatedAt, "full")}
              dateTime={data.calculatedAt}
            />
            {!isAudit ? (
              <span className="text-xs text-muted-foreground" aria-live="polite">
                配置版本{" "}
                <span className="num">
                  v{data.permissionVersion.split("-").at(-1)}
                </span>
              </span>
            ) : null}
          </div>
        }
        actions={
          <div className="flex flex-wrap items-center gap-2">
            {view === "roles" ? (
              <Button
                type="button"
                size="sm"
                onClick={() => router.push("/system/roles/new")}
              >
                <PlusIcon className="size-3.5" aria-hidden="true" />
                新建角色
              </Button>
            ) : null}
            {view === "users" ? (
              <Button
                type="button"
                size="sm"
                onClick={() =>
                  setAccountForm({ mode: "create", account: null })
                }
              >
                <PlusIcon className="size-3.5" aria-hidden="true" />
                新建账号
              </Button>
            ) : null}
          </div>
        }
      />

      <nav aria-label="权限与审计二级导航">
        <Tabs
          value={view}
          onValueChange={(v) => switchView(parseView(v))}
        >
          <TabsList variant="line" className="h-auto w-full flex-wrap justify-start">
            {(["roles", "users", "scopes", "audit"] as AccessView[]).map(
              (v) => (
                <TabsTrigger key={v} value={v}>
                  {ACCESS_VIEW_LABEL[v]}
                  <span className="ml-1.5 text-xs text-muted-foreground tabular-nums">
                    {v === "roles"
                      ? data.metrics.roleCount
                      : v === "users"
                        ? data.metrics.userCount
                        : v === "scopes"
                          ? data.metrics.scopeCount
                          : data.metrics.auditEventCount}
                  </span>
                </TabsTrigger>
              )
            )}
          </TabsList>
        </Tabs>
      </nav>

      <PolicyBanner policies={data.governancePolicies} view={view} />

      {actionError ? (
        <Alert variant="destructive">
          <TriangleAlertIcon aria-hidden="true" />
          <AlertTitle>操作提示</AlertTitle>
          <AlertDescription>{actionError}</AlertDescription>
        </Alert>
      ) : null}

      {lastResult ? (
        <FormalActionResult
          status={lastResult.status === "failed" ? "blocked" : lastResult.status}
          title={lastResult.title}
          description={lastResult.description}
          reference={lastResult.reference}
          facts={lastResult.facts}
          actions={
            <Button
              type="button"
              size="sm"
              variant="ghost"
              onClick={() => setLastResult(null)}
            >
              关闭
            </Button>
          }
        />
      ) : null}

      {data.fieldMaskNote ? (
        <Alert variant="info">
          <LockIcon aria-hidden="true" />
          <AlertTitle>字段打码</AlertTitle>
          <AlertDescription>{data.fieldMaskNote}</AlertDescription>
        </Alert>
      ) : null}

      {data.emptyReason && data.emptyReason !== "FIELD_MASKED" ? (
        <>
          {listToolbar}
          <EmptyByReason
            reason={data.emptyReason}
            onClearFilters={
              data.emptyReason === "FILTER_NO_RESULT"
                ? clearFilters
                : undefined
            }
          />
        </>
      ) : (
        <BusinessTableFrame
          title={ACCESS_VIEW_LABEL[view]}
          description={
            isAudit && data.auditCoverageFrom && data.auditCoverageTo
              ? `共 ${rows.length} 条 · 覆盖 ${formatDateTime(data.auditCoverageFrom, "full")} ~ ${formatDateTime(data.auditCoverageTo, "full")} · 无记录不等于动作未发生`
              : `共 ${rows.length} 条`
          }
          toolbar={listToolbar}
          table={
            view === "roles" ? (
              <DataTable
                columns={roleColumns}
                data={pagedRows as RoleRow[]}
                getRowId={(row) => row.id}
                rowCount={rows.length}
                pagination={pagination}
                onPaginationChange={setPagination}
                layout="flush"
                density="compact"
                loading={pageQuery.isFetching && !pageQuery.isPending}
                showRefreshingBanner={pageQuery.isFetching}
                defaultColumnPinning={{
                  left: ["identity"],
                  right: ["actions"],
                }}
              />
            ) : view === "users" ? (
              <DataTable
                columns={userColumns}
                data={pagedRows as UserRow[]}
                getRowId={(row) => row.id}
                rowCount={rows.length}
                pagination={pagination}
                onPaginationChange={setPagination}
                layout="flush"
                density="compact"
                loading={pageQuery.isFetching && !pageQuery.isPending}
                showRefreshingBanner={pageQuery.isFetching}
                defaultColumnPinning={{
                  left: ["identity"],
                  right: ["actions"],
                }}
              />
            ) : view === "scopes" ? (
              <DataTable
                columns={scopeColumns}
                data={pagedRows as ScopeRow[]}
                getRowId={(row) => row.id}
                rowCount={rows.length}
                pagination={pagination}
                onPaginationChange={setPagination}
                layout="flush"
                density="compact"
                loading={pageQuery.isFetching && !pageQuery.isPending}
                showRefreshingBanner={pageQuery.isFetching}
                defaultColumnPinning={{
                  left: ["subject"],
                  right: ["actions"],
                }}
              />
            ) : view === "fields" ? (
              <DataTable
                columns={fieldColumns}
                data={pagedRows as FieldPolicyRow[]}
                getRowId={(row) => row.id}
                rowCount={rows.length}
                pagination={pagination}
                onPaginationChange={setPagination}
                layout="flush"
                density="compact"
                loading={pageQuery.isFetching && !pageQuery.isPending}
                showRefreshingBanner={pageQuery.isFetching}
                defaultColumnPinning={{
                  left: ["target"],
                  right: ["actions"],
                }}
              />
            ) : (
              <DataTable
                columns={auditColumns}
                data={pagedRows as AuditEventRow[]}
                getRowId={(row) => row.auditEventId}
                rowCount={rows.length}
                pagination={pagination}
                onPaginationChange={setPagination}
                layout="flush"
                density="compact"
                loading={pageQuery.isFetching && !pageQuery.isPending}
                showRefreshingBanner={pageQuery.isFetching}
                defaultColumnPinning={{
                  left: ["time"],
                  right: ["actions"],
                }}
              />
            )
          }
        />
      )}

      {/* 有效权限解释 Sheet — 服务端投影，前端不合并 */}
      <QuickPreviewSheet
        open={Boolean(explainSubject)}
        onOpenChange={(open) => {
          if (!open) closeExplain()
        }}
        size="detail"
        onOpenChangeComplete={(open) => {
          if (!open) restoreRowFocus()
        }}
        title="有效权限解释"
        description="此处展示的权限结果为系统统一计算，可能与页面其它位置显示略有差异。"
      >
        <div className="min-h-0 flex-1 space-y-4 overflow-y-auto p-6">
        {effectiveQuery.isPending ? (
          <div className="h-40 animate-pulse rounded-lg bg-muted" />
        ) : effectiveQuery.isError ? (
          <BusinessFailureState
            kind="system"
            description="加载有效权限失败。"
            action={
              <Button
                type="button"
                size="sm"
                onClick={() => void effectiveQuery.refetch()}
              >
                重试
              </Button>
            }
          />
        ) : !effectiveQuery.data ? (
          <BusinessEmptyState
            kind="no-data"
            title="主体不存在或无权解释"
            description="仅解释当前用户有权管理的主体。"
          />
        ) : (
          <div className="flex flex-col gap-4">
            <div className="flex flex-wrap items-center gap-2">
              <Badge variant="secondary">
                {effectiveQuery.data.subject.type === "ROLE"
                  ? "角色"
                  : "用户"}
              </Badge>
              <span className="font-medium">
                {effectiveQuery.data.subject.label}
              </span>
              <Badge variant="outline">
                版本 v{effectiveQuery.data.permissionVersion.split("-").at(-1)}
              </Badge>
              <span className="text-xs text-muted-foreground">
                计算于 {formatDateTime(effectiveQuery.data.calculatedAt, "full")}
              </span>
            </div>

            <section className="space-y-2">
              <h3 className="text-sm font-semibold">模块与动作权限来源</h3>
              {effectiveQuery.data.moduleAndActionGrants.length === 0 ? (
                <p className="text-sm text-muted-foreground">无有效模块授权</p>
              ) : (
                effectiveQuery.data.moduleAndActionGrants.map((g) => (
                  <div
                    key={g.id}
                    className="rounded-lg border border-border p-3 text-sm"
                  >
                    <div className="font-medium">{g.targetLabel}</div>
                    <div className="text-muted-foreground">
                      {g.capability} · 来源 {g.sourceLabel}（{g.sourceType}）
                    </div>
                  </div>
                ))
              )}
            </section>

            <section className="space-y-2">
              <h3 className="text-sm font-semibold">数据范围来源</h3>
              {effectiveQuery.data.dataScopes.map((g) => (
                <div
                  key={g.id}
                  className="rounded-lg border border-border p-3 text-sm"
                >
                  <div className="font-medium">{g.targetLabel}</div>
                  <div className="text-muted-foreground">
                    来源 {g.sourceLabel}
                  </div>
                </div>
              ))}
            </section>

            <section className="space-y-2">
              <h3 className="text-sm font-semibold">字段策略来源</h3>
              {effectiveQuery.data.fieldPolicies.map((g) => (
                <div
                  key={g.id}
                  className="rounded-lg border border-border p-3 text-sm"
                >
                  <div className="font-medium">{g.targetLabel}</div>
                  <div className="text-muted-foreground">
                    {g.capability} · {g.sourceLabel}
                  </div>
                </div>
              ))}
            </section>

            <section className="space-y-2">
              <h3 className="text-sm font-semibold">历史参与者</h3>
              {effectiveQuery.data.historicalParticipantRules.map((e) => (
                <div
                  key={e.id}
                  className="rounded-lg border border-border p-3 text-sm"
                >
                  <div className="font-medium">{e.sourceLabel}</div>
                  <div className="text-muted-foreground">{e.message}</div>
                </div>
              ))}
            </section>

            <section className="space-y-2">
              <h3 className="text-sm font-semibold">
                拒绝 / 阻塞（含对象状态，不混淆为配置缺失）
              </h3>
              {effectiveQuery.data.deniedOrBlocked.map((e) => (
                <div
                  key={e.id}
                  className="rounded-lg border border-warning/40 bg-warning/5 p-3 text-sm"
                >
                  <div className="flex flex-wrap items-center gap-2">
                    <Badge variant="warning">{e.layerLabel}</Badge>
                    <span className="font-mono text-xs">{e.code}</span>
                  </div>
                  <p className="mt-1 text-muted-foreground">{e.message}</p>
                  <p className="mt-1 text-xs text-muted-foreground">
                    来源 {e.sourceLabel}（{e.sourceType}）
                  </p>
                </div>
              ))}
            </section>

            {effectiveQuery.data.actionBlockers.length > 0 ? (
              <section className="space-y-2">
                <h3 className="text-sm font-semibold">当前被阻断的操作</h3>
                {effectiveQuery.data.actionBlockers.map((b) => (
                  <Alert key={`${b.action}-${b.code}`} variant="warning">
                    <AlertTitle>{b.message}</AlertTitle>
                    <AlertDescription>{b.message}</AlertDescription>
                  </Alert>
                ))}
              </section>
            ) : null}
          </div>
        )}
        </div>
      </QuickPreviewSheet>

      {/* 审计详情 — 敏感字段仅字段名 + 已变更 */}
      <QuickPreviewSheet
        open={Boolean(eventOpenId)}
        onOpenChange={(open) => {
          if (!open) closeEvent()
        }}
        size="detail"
        onOpenChangeComplete={(open) => {
          if (!open) restoreRowFocus()
        }}
        title="审计事件详情"
        description="追加式事件只读；不展示敏感旧值/新值或密钥。"
      >
        <div className="min-h-0 flex-1 space-y-4 overflow-y-auto p-6">
        {eventQuery.isPending ? (
          <div className="h-32 animate-pulse rounded-lg bg-muted" />
        ) : !eventQuery.data ? (
          <BusinessEmptyState
            kind="no-data"
            title="事件不存在或无权查看"
            description="仅展示你有权查看的审计记录。"
          />
        ) : (
          <div className="flex flex-col gap-3 text-sm">
            <dl className="grid gap-2 sm:grid-cols-2">
              <div>
                <dt className="text-xs text-muted-foreground">审计事件号</dt>
                <dd className="font-mono">{eventQuery.data.auditEventId}</dd>
              </div>
              <div>
                <dt className="text-xs text-muted-foreground">发生时间</dt>
                <dd className="num">
                  {formatDateTime(eventQuery.data.recordedAt, "full")}
                </dd>
              </div>
              <div>
                <dt className="text-xs text-muted-foreground">操作者</dt>
                <dd>
                  {eventQuery.data.actorLabel}（{eventQuery.data.actorId}）
                </dd>
              </div>
              <div>
                <dt className="text-xs text-muted-foreground">责任角色</dt>
                <dd>{eventQuery.data.actorRole}</dd>
              </div>
              <div>
                <dt className="text-xs text-muted-foreground">动作</dt>
                <dd>{eventQuery.data.actionLabel}</dd>
              </div>
              <div>
                <dt className="text-xs text-muted-foreground">结果</dt>
                <dd>
                  <BusinessStatusBadge
                    label={eventQuery.data.resultLabel}
                    tone={eventQuery.data.resultTone}
                  />
                </dd>
              </div>
              <div>
                <dt className="text-xs text-muted-foreground">对象</dt>
                <dd>{eventQuery.data.objectLabel}</dd>
              </div>
              <div>
                <dt className="text-xs text-muted-foreground">请求追踪号</dt>
                <dd className="font-mono text-xs">
                  {eventQuery.data.traceId}
                  <div className="text-muted-foreground">
                    req {eventQuery.data.requestId}
                  </div>
                </dd>
              </div>
            </dl>
            <Separator />
            <div>
              <h3 className="text-sm font-semibold">变更字段</h3>
              <p className="mt-1 text-muted-foreground">
                {eventQuery.data.changedFieldDisplay !== "—"
                  ? eventQuery.data.changedFieldDisplay
                  : "无字段变更记录"}
              </p>
              <p className="mt-2 text-xs text-muted-foreground">
                敏感字段不返回完整旧值或新值；安全摘要默认仅作引用。
              </p>
              {eventQuery.data.safeDigest ? (
                <p className="mt-1 font-mono text-xs">
                  安全摘要 {eventQuery.data.safeDigest}
                </p>
              ) : null}
            </div>
            <p className="text-xs text-muted-foreground">
              审计记录不可编辑或删除。打开关联对象时将重新鉴权。
            </p>
          </div>
        )}
        </div>
      </QuickPreviewSheet>

      {/* 影响预览 + 提交 */}
      <Dialog
        open={changeOpen}
        onOpenChange={(open) => {
          setChangeOpen(open)
          if (!open) {
            setPendingCommand(null)
            setImpact(null)
          }
        }}
      >
        <DialogContent className="max-h-[90vh] overflow-y-auto sm:max-w-2xl">
          <DialogHeader>
            <DialogTitle>授权变更影响预览</DialogTitle>
            <DialogDescription>
              提交前先查看变更预览与受影响人员；若数据已被他人更新，需确认后重新提交。
            </DialogDescription>
          </DialogHeader>

          {impact ? (
            <div className="flex flex-col gap-4">
              <BatchImpactPreview
                title={impact.actionLabel}
                description={impact.changeSummary}
                filterSummary={`主体：${impact.subjectLabel}`}
                selectionScope={impact.affectedWorkSurfaceSummary}
                estimated={impact.affectedSubjectCount}
                processable={
                  impact.reviewPolicyBlocker
                    ? 0
                    : impact.affectedSubjectCount
                }
                skipped={
                  impact.reviewPolicyBlocker
                    ? impact.affectedSubjectCount
                    : 0
                }
                background={false}
                sensitiveFields={["密钥", "卡密", "完整银行账号"]}
                skippedReason={impact.reviewPolicyBlocker?.message}
              />

              <Alert
                variant={
                  impact.riskLevel === "high"
                    ? "warning"
                    : impact.riskLevel === "medium"
                      ? "info"
                      : "default"
                }
              >
                <ShieldAlertIcon aria-hidden="true" />
                <AlertTitle>
                  风险{" "}
                  {impact.riskLevel === "high"
                    ? "高"
                    : impact.riskLevel === "medium"
                      ? "中"
                      : "低"}
                  {impact.riskFlags.length
                    ? ` · ${impact.riskFlags.map(riskLabel).join("、")}`
                    : ""}
                </AlertTitle>
                <AlertDescription>{impact.riskSummary}</AlertDescription>
              </Alert>

              {impact.reviewPolicyBlocker ? (
                <Alert variant="destructive">
                  <AlertTitle>
                    {impact.reviewPolicyBlocker.code}
                  </AlertTitle>
                  <AlertDescription>
                    {impact.reviewPolicyBlocker.message}
                  </AlertDescription>
                </Alert>
              ) : null}

              <BusinessDiffPanel
                title="配置差异"
                changes={impact.diffs.map((d) => ({
                  id: d.id,
                  field: d.field,
                  before: d.before,
                  after: d.after,
                  note: d.note,
                }))}
              />

              {!impact.reviewPolicyBlocker ? (
                <form
                  className="space-y-3"
                  onSubmit={async (e) => {
                    e.preventDefault()
                    // 校验通过后才执行提交：说明超长等校验失败时不再绕过
                    await form.handleSubmit()
                    if (form.state.isFieldsValid) {
                      await confirmChange()
                    }
                  }}
                >
                  <div className="space-y-1.5">
                    <Label htmlFor="w19-reason">变更原因</Label>
                    <form.AppField
                      name="reasonCode"
                      children={(field) => (
                        <OptionCombobox
                          id="w19-reason"
                          value={field.state.value}
                          onValueChange={(v) =>
                            field.handleChange(v ?? field.state.value)
                          }
                          options={[
                            { value: "SECURITY_OPS", label: "安全运维" },
                            {
                              value: "EMERGENCY_STOP_LOSS",
                              label: "紧急止损",
                            },
                            { value: "ORG_CHANGE", label: "组织调整" },
                          ]}
                          className="w-full"
                          allowClear={false}
                          aria-label="变更原因"
                          placeholder="变更原因"
                        />
                      )}
                    />
                  </div>
                  <div className="space-y-1.5">
                    <Label htmlFor="w19-comment">说明（可选，勿填密钥）</Label>
                    <form.AppField
                      name="comment"
                      children={(field) => (
                        <Textarea
                          id="w19-comment"
                          value={field.state.value ?? ""}
                          onChange={(e) =>
                            field.handleChange(e.target.value)
                          }
                          rows={2}
                          placeholder="不包含密钥或敏感业务正文"
                        />
                      )}
                    />
                  </div>
                  <p className="text-xs text-muted-foreground">
                    提交前系统会按最新配置核对版本；若配置已被他人更新，将提示你重新确认。
                  </p>
                  <DialogFooter>
                    <Button
                      type="button"
                      variant="ghost"
                      onClick={() => setChangeOpen(false)}
                    >
                      取消
                    </Button>
                    <Button
                      type="submit"
                      disabled={submitMutation.isPending}
                      variant={
                        pendingCommand?.action ===
                        "EMERGENCY_REVOKE_USER_ROLE"
                          ? "destructive"
                          : "default"
                      }
                    >
                      {submitMutation.isPending
                        ? "提交中…"
                        : "确认提交"}
                    </Button>
                  </DialogFooter>
                </form>
              ) : (
                <DialogFooter>
                  <Button
                    type="button"
                    variant="ghost"
                    onClick={() => {
                      applyOutcome({
                        outcome: "REJECTED",
                        code: impact.reviewPolicyBlocker!.code,
                        message: impact.reviewPolicyBlocker!.message,
                        actionBlockers: [impact.reviewPolicyBlocker!],
                      })
                      setChangeOpen(false)
                    }}
                  >
                    关闭并记录阻断
                  </Button>
                </DialogFooter>
              )}
            </div>
          ) : (
            <div className="h-24 animate-pulse rounded-lg bg-muted" />
          )}
        </DialogContent>
      </Dialog>

      {/* 账号新建 / 编辑（字段少，弹窗承载；角色走整页表单） */}
      {accountForm ? (
        <AccountFormDialog
          key={
            accountForm.mode === "edit"
              ? accountForm.account?.id ?? "edit"
              : "create"
          }
          mode={accountForm.mode}
          account={accountForm.account}
          roleOptions={assignableRolesQuery.data ?? []}
          onOpenChange={(open) => {
            if (!open) setAccountForm(null)
          }}
        />
      ) : null}

      {deletingAccount ? (
        <DeleteAdminDialog
          key={deletingAccount.id}
          account={deletingAccount}
          onOpenChange={(open) => {
            if (!open) setDeletingAccount(null)
          }}
        />
      ) : null}

      {deletingRole ? (
        <DeleteRoleDialog
          key={deletingRole.id}
          role={deletingRole}
          onOpenChange={(open) => {
            if (!open) setDeletingRole(null)
          }}
        />
      ) : null}
    </div>
  )
}
