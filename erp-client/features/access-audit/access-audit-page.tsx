"use client"

import * as React from "react"
import { usePathname, useRouter, useSearchParams } from "next/navigation"
import {
  DownloadIcon,
  EyeIcon,
  LockIcon,
  SearchIcon,
  ShieldAlertIcon,
  ShieldOffIcon,
  TriangleAlertIcon,
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
  PageActions,
  PageHeader,
  QuickPreviewSheet,
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
  useAccessListQuery,
  useAuditEventQuery,
  useEffectiveAccessQuery,
  usePreviewAccessChangeMutation,
  useSetAccessDemoFlagsMutation,
  useSubmitAccessChangeMutation,
} from "@/features/access-audit/queries"
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
  if (
    raw === "roles" ||
    raw === "users" ||
    raw === "scopes" ||
    raw === "fields" ||
    raw === "audit"
  ) {
    return raw
  }
  return "roles"
}

function formatDateTime(iso?: string) {
  if (!iso) return "—"
  try {
    return new Date(iso).toLocaleString("zh-CN", {
      hour12: false,
      year: "numeric",
      month: "2-digit",
      day: "2-digit",
      hour: "2-digit",
      minute: "2-digit",
    })
  } catch {
    return iso
  }
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

type ResultState = {
  status: "succeeded" | "rejected" | "blocked" | "unknown"
  title: string
  description: string
  reference?: string
  facts?: { label: string; value: React.ReactNode }[]
  pendingIdempotencyKey?: string
} | null

const changeReasonSchema = z.object({
  reasonCode: z.string().min(1, "请选择变更原因"),
  comment: z.string().trim().max(200),
})

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

  return (
    <Alert
      className="py-2"
      data-slot="policy-banner"
      variant={
        time.state === "MISSING" ||
        field.state === "MISSING" ||
        audit.state === "MISSING"
          ? "warning"
          : "info"
      }
    >
      <ShieldAlertIcon aria-hidden="true" />
      <AlertTitle className="flex flex-wrap items-center gap-2">
        治理策略门闩
        <Badge variant="outline">本期不支持任务流</Badge>
      </AlertTitle>
      <AlertDescription className="grid gap-x-4 gap-y-1 text-xs lg:grid-cols-2 [&_p:not(:last-child)]:mb-0">
        {(view === "users" || view === "roles") && (
          <p>
            <strong className="text-foreground">用户角色时间：</strong>
            {time.state === "MISSING" ? (
              <>
                <span className="font-mono">{time.blockerCode}</span> ·
                按保守策略：仅允许立即紧急撤权
              </>
            ) : (
              <>
                <span className="font-mono">{time.policyVersion}</span> · 预约
                {time.schedulingAllowed ? "允许" : "禁用"} · 到期
                {time.expirationAllowed ? "允许" : "禁用"}
              </>
            )}
          </p>
        )}
        {(view === "fields" || view === "roles") && (
          <p>
            <strong className="text-foreground">字段粒度：</strong>
            {field.state === "MISSING" ? (
              <>
                <span className="font-mono">{field.blockerCode}</span> ·
                只读，不自由输入字段名
              </>
            ) : (
              <>
                <span className="font-mono">{field.policyVersion}</span> ·
                {field.editableTargets.map((target) => target.label).join("、")}
              </>
            )}
          </p>
        )}
        {(view === "audit" || view === "roles" || view === "users") && (
          <p>
            <strong className="text-foreground">审计 / 导出：</strong>
            {audit.state === "MISSING" ? (
              <>
                <span className="font-mono">{audit.blockerCode}</span> ·
                保守短窗口 {formatDateTime(audit.fallbackFrom)} ~{" "}
                {formatDateTime(audit.fallbackTo)}，导出禁用
              </>
            ) : (
              <>
                <span className="font-mono">{audit.policyVersion}</span> ·
                最大在线 {Math.round(audit.maxOnlineWindowSeconds / 3600)} 小时
              </>
            )}
          </p>
        )}
        <p className="lg:col-span-2">
          <TriangleAlertIcon className="mr-1 inline size-3" aria-hidden="true" />
          {ACCESS_LAYER_HELP.map((item) => item.title).join(" · ")}；命中复核要求时以{" "}
          <span className="font-mono">REVIEW_POLICY_UNCONFIGURED</span> 阻断。
        </p>
      </AlertDescription>
    </Alert>
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
          description="管理范围有效，但当前视图下尚未配置角色/范围或没有审计事件。可创建配置（有权时）或调整视图。"
        />
      )
    case "FIELD_MASKED":
      return (
        <BusinessEmptyState
          kind="no-data"
          title="字段级掩码（非空列表）"
          description="列表与标签保留，敏感值按字段策略掩码显示。权限管理员不会因为能配置权限而自动看到业务敏感正文。"
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
    const next = new URLSearchParams(searchParams.toString())
    for (const [key, value] of Object.entries(patch)) {
      if (value == null || value === "") next.delete(key)
      else next.set(key, value)
    }
    if (!next.get("view")) next.set("view", view)
    const qs = next.toString()
    const href = qs ? `${pathname}?${qs}` : pathname
    if (options?.replace) router.replace(href)
    else router.push(href)
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

  const listQuery: AccessListQuery = React.useMemo(
    () => ({
      view,
      q: qParam || undefined,
      status,
      org,
      risk,
      subjectType: subjectTypeParam,
      subjectId: subjectIdParam,
      from: fromParam,
      to: toParam,
      actorId,
      action,
      objectType,
      objectId,
      result: resultFilter,
      traceId,
      eventId: eventIdParam,
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
      eventIdParam,
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
  const demoMutation = useSetAccessDemoFlagsMutation()

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
          { label: "配置版本", value: outcome.permissionVersion },
          {
            label: "影响主体数",
            value: String(outcome.affectedSubjectCount),
          },
          { label: "审计事件号", value: outcome.auditEventId },
          { label: "生效时间", value: formatDateTime(outcome.effectiveAt) },
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
            ? "复核策略未固化，动作已阻断"
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
        title: "权限版本冲突",
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
        id: "fields",
        header: "字段策略",
        cell: ({ row }) => (
          <span className="text-sm">
            {data?.emptyReason === "FIELD_MASKED"
              ? "****（已掩码）"
              : row.original.fieldPolicySummary}
          </span>
        ),
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
          <span className="font-mono text-xs">
            {row.original.permissionVersion}
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
        cell: ({ row }) => (
          <div className="flex flex-wrap gap-1">
            <Button
              type="button"
              size="sm"
              variant="ghost"
              ref={(el) => {
                rowFocusRef.current.set(row.original.id, el)
              }}
              onClick={() => openExplain("ROLE", row.original.id)}
            >
              <EyeIcon data-icon="inline-start" aria-hidden="true" />
              有效权限
            </Button>
            {row.original.status === "enabled" &&
            !row.original.riskFlags.includes("HIGH_PRIVILEGE") ? (
              <Button
                type="button"
                size="sm"
                variant="outline"
                onClick={() =>
                  void startChange({
                    subjectType: "ROLE",
                    subjectId: row.original.id,
                    action: "UPDATE_ROLE_PERMISSIONS",
                    expectedPermissionVersion:
                      data?.permissionVersion ?? row.original.permissionVersion,
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
              </Button>
            ) : null}
            {row.original.riskFlags.includes("HIGH_PRIVILEGE") ? (
              <Button
                type="button"
                size="sm"
                variant="outline"
                onClick={() =>
                  void startChange({
                    subjectType: "ROLE",
                    subjectId: row.original.id,
                    action: "UPDATE_ROLE_PERMISSIONS",
                    expectedPermissionVersion:
                      data?.permissionVersion ?? row.original.permissionVersion,
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
              </Button>
            ) : null}
            {row.original.status === "enabled" &&
            row.original.riskFlags.includes("PENDING_DISABLE") ? (
              <Button
                type="button"
                size="sm"
                variant="destructive"
                onClick={() =>
                  void startChange({
                    subjectType: "ROLE",
                    subjectId: row.original.id,
                    action: "DISABLE_ROLE",
                    expectedPermissionVersion:
                      data?.permissionVersion ?? row.original.permissionVersion,
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
              </Button>
            ) : null}
          </div>
        ),
      },
    ],
    [openExplain, startChange, data?.permissionVersion, data?.emptyReason]
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
        header: "已记录有效期间",
        cell: ({ row }) => (
          <span className="text-xs text-muted-foreground">
            {formatDateTime(row.original.effectiveFrom)}
            {row.original.effectiveTo
              ? ` ~ ${formatDateTime(row.original.effectiveTo)}`
              : " ~ 长期"}
            <span className="mt-0.5 block text-[11px]">
              （只读记录；策略未配置时不可编辑预约/到期）
            </span>
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
        cell: ({ row }) => (
          <div className="flex flex-wrap gap-1">
            <Button
              type="button"
              size="sm"
              variant="ghost"
              ref={(el) => {
                rowFocusRef.current.set(row.original.id, el)
              }}
              onClick={() => openExplain("USER", row.original.userId)}
            >
              <EyeIcon data-icon="inline-start" aria-hidden="true" />
              有效权限
            </Button>
            {row.original.roleAssignmentId ? (
              <Button
                type="button"
                size="sm"
                variant="destructive"
                onClick={() =>
                  void startChange({
                    subjectType: "USER",
                    subjectId: row.original.userId,
                    action: "EMERGENCY_REVOKE_USER_ROLE",
                    roleAssignmentId: row.original.roleAssignmentId!,
                    expectedPermissionVersion:
                      data?.permissionVersion ?? row.original.permissionVersion,
                    reasonCode: "EMERGENCY_STOP_LOSS",
                    idempotencyKey: "pending",
                  })
                }
              >
                <ShieldOffIcon data-icon="inline-start" aria-hidden="true" />
                紧急撤权
              </Button>
            ) : null}
          </div>
        ),
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
          <Button
            type="button"
            size="sm"
            variant="ghost"
            ref={(el) => {
              rowFocusRef.current.set(row.original.id, el)
            }}
            onClick={() =>
              openExplain(row.original.subjectType, row.original.subjectId)
            }
          >
            有效权限
          </Button>
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
            <Badge variant="success">可提交 policyTargetId</Badge>
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
            {formatDateTime(row.original.recordedAt)}
          </span>
        ),
      },
      {
        id: "actor",
        header: "操作者",
        cell: ({ row }) => (
          <div className="flex items-center gap-1.5">
            <span className="font-medium">{row.original.actorLabel}</span>
            <span className="font-mono text-xs text-muted-foreground">
              {row.original.actorId}
            </span>
          </div>
        ),
      },
      {
        id: "role",
        header: "责任角色",
        cell: ({ row }) => row.original.actorRole,
      },
      {
        id: "action",
        header: "动作",
        cell: ({ row }) => row.original.actionLabel,
      },
      {
        id: "object",
        header: "对象",
        cell: ({ row }) => (
          <div className="flex items-center gap-1.5">
            <span>{row.original.objectLabel}</span>
            <span className="font-mono text-xs text-muted-foreground">
              {row.original.objectType}/{row.original.objectId}
            </span>
          </div>
        ),
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
            {row.original.changedFieldNames.length
              ? row.original.changedFieldNames
                  .map((n) => `${n} · 已变更`)
                  .join("；")
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
          <Button
            type="button"
            size="xs"
            variant="ghost"
            ref={(el) => {
              rowFocusRef.current.set(row.original.auditEventId, el)
            }}
            onClick={() => openEvent(row.original.auditEventId)}
          >
            详情
          </Button>
        ),
      },
    ],
    [openEvent]
  )

  if (pageQuery.isPending) {
    return (
      <div className="mx-auto flex w-full max-w-shell flex-col gap-4 p-4 md:p-5">
        <div className="h-10 w-48 animate-pulse rounded-lg bg-muted" />
        <div className="h-12 animate-pulse rounded-xl bg-muted" />
        <div className="h-24 animate-pulse rounded-xl bg-muted" />
        <div className="h-[28rem] animate-pulse rounded-2xl bg-muted" />
      </div>
    )
  }

  if (pageQuery.isError || !data) {
    return (
      <div className="mx-auto flex w-full max-w-shell flex-col gap-4 p-4 md:p-5">
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

  return (
    <div className="mx-auto flex w-full max-w-shell flex-col gap-2 p-3 md:p-4">
      <PageHeader
        title={isAudit ? "审计查询" : "权限与数据范围"}
        metadata={
          <DataFreshness
            label={isAudit ? "审计更新时间" : "权限配置更新时间"}
            state="fresh"
            updatedAt={formatDateTime(data.calculatedAt)}
            dateTime={data.calculatedAt}
          />
        }
        actions={
          <PageActions
            actions={[
              {
                actionKey: "export",
                label: isAudit ? "导出审计" : "导出配置",
                icon: DownloadIcon,
                variant: "outline",
                mobileVisibility: "hide",
                disabled: exportBlocked,
                title: exportBlocker?.message,
                onClick: () => {
                  if (exportBlocked) {
                    setActionError(
                      exportBlocker?.message ??
                        "导出策略未配置，导出已禁用。"
                    )
                    return
                  }
                  setLastResult({
                    status: "blocked",
                    title: "导出暂不可用",
                    description: "当前账号尚未配置导出权限，无法生成导出文件。",
                  })
                },
              },
            ]}
          />
        }
      />

      <nav aria-label="权限与审计二级导航">
        <Tabs
          value={view}
          onValueChange={(v) => {
            const next = parseView(v)
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
          }}
        >
          <TabsList variant="line" className="h-auto flex-wrap">
            {(
              ["roles", "users", "scopes", "fields", "audit"] as AccessView[]
            ).map((v) => (
              <TabsTrigger key={v} value={v}>
                {ACCESS_VIEW_LABEL[v]}
              </TabsTrigger>
            ))}
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
          status={lastResult.status}
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

      <ListToolbar
        search={
          <InputGroup className="max-w-sm">
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
                    patchUrl({
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
                    patchUrl({
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
                <InputGroup className="w-auto max-w-[10rem]">
                  <InputGroupInput
                    value={actorId ?? ""}
                    onChange={(e) =>
                      patchUrl({
                        actorId: e.target.value.trim() || null,
                      })
                    }
                    placeholder="操作者 ID"
                    aria-label="操作者"
                  />
                </InputGroup>
                <OptionCombobox
                  value={action ?? "all"}
                  onValueChange={(v) =>
                    patchUrl({
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
                    patchUrl({
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
                <details className="group relative">
                  <summary className="flex h-8 cursor-pointer list-none items-center rounded-lg border px-3 text-sm [&::-webkit-details-marker]:hidden">
                    高级筛选
                  </summary>
                  <div className="absolute right-0 z-30 mt-2 grid w-80 gap-2 rounded-xl border bg-popover p-3 shadow-lg">
                    <InputGroup>
                      <InputGroupInput
                        value={traceId ?? ""}
                        onChange={(e) =>
                          patchUrl({
                            traceId: e.target.value.trim() || null,
                          })
                        }
                        placeholder="请求追踪号"
                        aria-label="请求追踪号"
                      />
                    </InputGroup>
                    <InputGroup>
                      <InputGroupInput
                        value={objectType ?? ""}
                        onChange={(e) =>
                          patchUrl({
                            objectType: e.target.value.trim() || null,
                          })
                        }
                        placeholder="对象类型"
                        aria-label="对象类型"
                      />
                    </InputGroup>
                    <InputGroup>
                      <InputGroupInput
                        value={objectId ?? ""}
                        onChange={(e) =>
                          patchUrl({
                            objectId: e.target.value.trim() || null,
                          })
                        }
                        placeholder="对象编号"
                        aria-label="对象编号"
                      />
                    </InputGroup>
                  </div>
                </details>
              </>
            )}
          </>
        }
        actions={
          <details className="group relative text-xs text-muted-foreground">
            <summary className="cursor-pointer">演示状态</summary>
            <div className="absolute right-0 z-30 mt-2 flex w-72 flex-col gap-2 rounded-xl border bg-popover p-3 shadow-lg">
              <OptionCombobox
                value="none"
                onValueChange={(v) => {
                  const next = v ?? "none"
                  void demoMutation.mutateAsync({
                    emptyReason:
                      next === "none" ? null : (next as AccessEmptyReason),
                  })
                }}
                options={[
                  { value: "none", label: "正常" },
                  { value: "NO_MODULE_PERMISSION", label: "无模块权限" },
                  { value: "NO_DATA_SCOPE", label: "无数据范围" },
                  { value: "NO_RECORDS_IN_SCOPE", label: "范围内无记录" },
                  { value: "FIELD_MASKED", label: "字段掩码" },
                ]}
                className="w-full"
                size="sm"
                allowClear={false}
                aria-label="演示空态"
                placeholder="演示空态"
              />
              <Button
                type="button"
                size="sm"
                variant="outline"
                onClick={() =>
                  void demoMutation.mutateAsync({
                    userRoleTimePolicyConfigured: true,
                  })
                }
              >
                启用时间策略
              </Button>
              <Button
                type="button"
                size="sm"
                variant="outline"
                onClick={() =>
                  void demoMutation.mutateAsync({
                    fieldGranularityConfigured: true,
                  })
                }
              >
                启用字段粒度
              </Button>
              <Button
                type="button"
                size="sm"
                variant="outline"
                onClick={() =>
                  void demoMutation.mutateAsync({
                    auditAccessPolicyConfigured: true,
                  })
                }
              >
                启用审计策略
              </Button>
            </div>
          </details>
        }
      />

      {!isAudit ? (
        <p className="text-xs text-muted-foreground" aria-live="polite">
          权限版本{" "}
          <span className="font-mono">{data.permissionVersion}</span> ·
          任务流：{data.workItemSupport === "DISABLED_Q1"
            ? "Q1 前关闭"
            : data.workItemSupport}
        </p>
      ) : null}

      {data.fieldMaskNote ? (
        <Alert variant="info">
          <LockIcon aria-hidden="true" />
          <AlertTitle>字段掩码</AlertTitle>
          <AlertDescription>{data.fieldMaskNote}</AlertDescription>
        </Alert>
      ) : null}

      {data.emptyReason && data.emptyReason !== "FIELD_MASKED" ? (
        <EmptyByReason
          reason={data.emptyReason}
          onClearFilters={
            data.emptyReason === "FILTER_NO_RESULT"
              ? clearFilters
              : undefined
          }
        />
      ) : (
        <BusinessTableFrame
          title={ACCESS_VIEW_LABEL[view]}
          description={
            isAudit && data.auditCoverageFrom && data.auditCoverageTo
              ? `共 ${rows.length} 条 · 覆盖 ${formatDateTime(data.auditCoverageFrom)} ~ ${formatDateTime(data.auditCoverageTo)} · 无记录不等于动作未发生 · 更新于 ${data.watermark}`
              : `共 ${rows.length} 条 · 首屏固定身份与操作列`
          }
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
              <span className="font-mono text-xs text-muted-foreground">
                {effectiveQuery.data.subject.id}
              </span>
              <Badge variant="outline">
                版本 {effectiveQuery.data.permissionVersion}
              </Badge>
              <span className="text-xs text-muted-foreground">
                计算于 {formatDateTime(effectiveQuery.data.calculatedAt)}
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
                <h3 className="text-sm font-semibold">当前动作 blocker</h3>
                {effectiveQuery.data.actionBlockers.map((b) => (
                  <Alert key={`${b.action}-${b.code}`} variant="warning">
                    <AlertTitle>
                      {b.action} · {b.code}
                    </AlertTitle>
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
                  {formatDateTime(eventQuery.data.recordedAt)}
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
                <dd>
                  {eventQuery.data.actionLabel}
                  <span className="ml-1 font-mono text-xs text-muted-foreground">
                    {eventQuery.data.actionType}
                  </span>
                </dd>
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
                <dd>
                  {eventQuery.data.objectLabel}
                  <div className="font-mono text-xs text-muted-foreground">
                    {eventQuery.data.objectType}/{eventQuery.data.objectId}
                  </div>
                </dd>
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
                {eventQuery.data.changedFieldNames.length
                  ? eventQuery.data.changedFieldNames
                      .map((n) => `${n} · 已变更`)
                      .join("；")
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
                  风险 {impact.riskLevel.toUpperCase()}
                  {impact.riskFlags.length
                    ? ` · ${impact.riskFlags.join(", ")}`
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
                  onSubmit={(e) => {
                    e.preventDefault()
                    void form.handleSubmit()
                    void confirmChange()
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
                    期望权限版本{" "}
                    <span className="font-mono">
                      {pendingCommand?.expectedPermissionVersion}
                    </span>
                    ；Q1 前本命令不携带 workItemId / claimToken。
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
    </div>
  )
}
