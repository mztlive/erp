"use client"

import * as React from "react"
import { usePathname, useRouter, useSearchParams } from "next/navigation"
import {
    LockIcon,
    PlusIcon,
    ShieldAlertIcon,
    TriangleAlertIcon,
} from "lucide-react"
import type { PaginationState } from "@tanstack/react-table"

import {
    BatchImpactPreview,
    BusinessDiffPanel,
    BusinessFailureState,
    BusinessTableFrame,
    DataFreshness,
    DataTable,
    FormalActionResult,
    OptionCombobox,
    PageHeader,
    PageScaffold,
} from "@/components/business"
import { formatDateTime } from "@/lib/datetime"
import { getErrorMessage } from "@/lib/api/errors"
import { patchUrl as patchSearchParams } from "@/lib/patch-search-params"
import { type ResultState } from "@/components/business/feedback"
import { useAppForm } from "@/components/form"
import { Alert, AlertDescription, AlertTitle } from "@/components/ui/alert"
import { Button } from "@/components/ui/button"
import {
    Dialog,
    DialogContent,
    DialogDescription,
    DialogFooter,
    DialogHeader,
    DialogTitle,
} from "@/components/ui/dialog"
import { Label } from "@/components/ui/label"
import { Tabs, TabsList, TabsTrigger } from "@/components/ui/tabs"
import { Textarea } from "@/components/ui/textarea"
import {
    useAccessListQuery,
    useAuditEventQuery,
    useEffectiveAccessQuery,
    usePreviewAccessChangeMutation,
    useSubmitAccessChangeMutation,
} from "@/features/access-audit/hooks/queries"
import { useAccessColumns } from "@/features/access-audit/hooks/use-access-columns"
import { AccessListToolbar } from "@/features/access-audit/components/access-list-toolbar"
import { AccessPreviewSheets } from "@/features/access-audit/components/access-preview-sheets"
import { changeReasonSchema } from "@/features/access-audit/lib/change-reason-schema"
import { EmptyByReason } from "@/features/access-audit/components/empty-by-reason"
import { parseView } from "@/features/access-audit/lib/url-state"
import { PolicyBanner } from "@/features/access-audit/components/policy-banner"
import { riskLabel } from "@/features/access-audit/lib/risk-labels"
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
    AccessImpactPreview,
    AccessListQuery,
    AccessView,
    AuditEventRow,
    FieldPolicyRow,
    RoleRow,
    ScopeRow,
    UserRow,
} from "@/features/access-audit/types"
import { ACCESS_VIEW_LABEL } from "@/features/access-audit/types"
import { resultText } from "@/lib/ui-text"

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
    const pageParamRaw = Number(searchParams.get("page"))
    const pageParamIndex =
        Number.isFinite(pageParamRaw) && pageParamRaw > 0
            ? Math.max(0, Math.floor(pageParamRaw) - 1)
            : 0
    const [pagination, setPagination] = React.useState<PaginationState>({
        pageIndex: pageParamIndex,
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
            : null,
    )
    const [eventOpenId, setEventOpenId] = React.useState<string | null>(
        eventIdParam ?? null,
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
        new Map(),
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
                    subjectTypeParam === "USER" || view === "users"
                        ? "USER"
                        : "ROLE",
                id: subjectIdParam,
            })
        }
    }, [subjectIdParam, subjectTypeParam, view])

    React.useEffect(() => {
        const onKey = (event: KeyboardEvent) => {
            if (
                event.key !== "/" ||
                event.metaKey ||
                event.ctrlKey ||
                event.altKey
            )
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
        options?: { replace?: boolean },
    ) {
        patchSearchParams(
            { router, pathname, searchParams, view },
            patch,
            options,
        )
    }

    /** 筛选变更统一回到第一页：避免第二页起筛选变窄后「共 N 条」与空态并存。 */
    const patchFilterUrl = (
        patch: Record<string, string | null | undefined>,
    ) => {
        patchUrl({ ...patch, page: null }, { replace: true })
        setPagination((p) => (p.pageIndex === 0 ? p : { ...p, pageIndex: 0 }))
    }

    React.useEffect(() => {
        const handle = globalThis.setTimeout(() => {
            if (searchInput === qParam) return
            patchUrl(
                {
                    q: searchInput.trim() || null,
                    page: null,
                },
                { replace: true },
            )
            setPagination((p) =>
                p.pageIndex === 0 ? p : { ...p, pageIndex: 0 },
            )
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
        ],
    )

    const pageQuery = useAccessListQuery(listQuery)
    const effectiveQuery = useEffectiveAccessQuery(
        explainSubject?.type ?? null,
        explainSubject?.id ?? null,
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
                { replace: true },
            )
        },
        // eslint-disable-next-line react-hooks/exhaustive-deps
        [searchParams, pathname, view],
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
        [searchParams, pathname, view],
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
                setActionError(getErrorMessage(err, "影响预览失败"))
            }
        },
        [previewMutation, form],
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
                    {
                        label: "生效时间",
                        value: formatDateTime(outcome.effectiveAt, "full"),
                    },
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
            setActionError(getErrorMessage(err, "提交失败"))
        }
    }, [pendingCommand, impact, form, submitMutation, applyOutcome])

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
            page: null,
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
            b.action === "EXPORT_AUDIT" || b.action === "EXPORT_CONFIGURATION",
    )

    // ── columns ──
    const {
        auditColumns,
        fieldColumns,
        roleColumns,
        scopeColumns,
        userColumns,
    } = useAccessColumns({
        data,
        policies,
        router,
        rowFocusRef,
        openExplain,
        openEvent,
        startChange,
        setAccountForm,
        setDeletingAccount,
        setDeletingRole,
    })
    if (pageQuery.isPending) {
        return (
            <PageScaffold density="compact">
                <div className="h-9 w-40 animate-pulse rounded-lg bg-muted" />
                <div className="h-9 animate-pulse rounded-lg bg-muted" />
                <div className="h-10 animate-pulse rounded-lg bg-muted" />
                <div className="h-[32rem] animate-pulse rounded-lg bg-muted" />
            </PageScaffold>
        )
    }

    if (pageQuery.isError || !data) {
        return (
            <PageScaffold density="compact">
                <PageHeader title="权限与审计" />
                <BusinessFailureState
                    error={pageQuery.error}
                    action={
                        <Button
                            type="button"
                            variant="secondary"
                            className="rounded-lg shadow-none"
                            onClick={() => void pageQuery.refetch()}
                        >
                            重试
                        </Button>
                    }
                />
            </PageScaffold>
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
        pagination.pageIndex * pagination.pageSize + pagination.pageSize,
    )

    const handlePaginationChange = (next: PaginationState) => {
        setPagination(next)
        const page = next.pageIndex + 1
        patchUrl({ page: page > 1 ? String(page) : null }, { replace: true })
    }

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
            page: null,
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
        actorId || traceId || objectType || objectId,
    )
    const hasActiveFilters = isAudit
        ? Boolean(
              qParam ||
              action ||
              resultFilter ||
              fromParam ||
              toParam ||
              advancedAuditActive,
          )
        : Boolean(qParam || status || risk || org)

    const handleExport = () => {
        if (exportBlocked) {
            setActionError(
                exportBlocker?.message ?? "导出策略未配置，导出已禁用。",
            )
            return
        }
        setLastResult({
            status: "blocked",
            title: "导出功能待接入",
            description: "导出尚未接入后端；正式环境将按权限策略生成导出文件。",
        })
    }

    // 组织维度选项：取自当前列表的角色/用户组织标签；选中值不在选项时并入，保证可回退
    const orgOptions = (() => {
        const seen = new Set<string>()
        const options: { value: string; label: string }[] = []
        for (const row of [...(data?.roles ?? []), ...(data?.users ?? [])]) {
            const label = row.organizationLabel
            if (!label || seen.has(label)) continue
            seen.add(label)
            options.push({ value: label, label })
        }
        if (org && !seen.has(org)) {
            options.unshift({ value: org, label: org })
        }
        return options
    })()

    const listToolbar = (
        <AccessListToolbar
            isAudit={isAudit}
            searchInput={searchInput}
            searchInputRef={searchInputRef}
            setSearchInput={setSearchInput}
            org={org}
            status={status}
            risk={risk}
            orgOptions={orgOptions}
            fromParam={fromParam}
            toParam={toParam}
            action={action}
            resultFilter={resultFilter}
            advancedAuditActive={advancedAuditActive}
            debouncedFilters={debouncedFilters}
            setDebouncedFilters={setDebouncedFilters}
            actorId={actorId}
            traceId={traceId}
            objectType={objectType}
            objectId={objectId}
            patchFilterUrl={patchFilterUrl}
            hasActiveFilters={hasActiveFilters}
            clearFilters={clearFilters}
            exportBlocked={exportBlocked}
            exportBlocker={exportBlocker}
            handleExport={handleExport}
        />
    )

    return (
        <PageScaffold density="compact">
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
                            label={
                                isAudit ? "审计更新时间" : "权限配置更新时间"
                            }
                            state={pageQuery.isFetching ? "syncing" : "fresh"}
                            updatedAt={formatDateTime(
                                data.calculatedAt,
                                "full",
                            )}
                            dateTime={data.calculatedAt}
                        />
                        {!isAudit ? (
                            <span
                                className="text-xs text-muted-foreground"
                                aria-live="polite"
                            >
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
                                <PlusIcon
                                    className="size-3.5"
                                    aria-hidden="true"
                                />
                                新建角色
                            </Button>
                        ) : null}
                        {view === "users" ? (
                            <Button
                                type="button"
                                size="sm"
                                onClick={() =>
                                    setAccountForm({
                                        mode: "create",
                                        account: null,
                                    })
                                }
                            >
                                <PlusIcon
                                    className="size-3.5"
                                    aria-hidden="true"
                                />
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
                    <TabsList
                        variant="line"
                        className="h-auto w-full flex-wrap justify-start"
                    >
                        {(
                            [
                                "roles",
                                "users",
                                "scopes",
                                "audit",
                            ] as AccessView[]
                        ).map((v) => (
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
                    status={
                        lastResult.status === "failed"
                            ? "blocked"
                            : lastResult.status
                    }
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

            <BusinessTableFrame
                title={ACCESS_VIEW_LABEL[view]}
                description={
                    data.emptyReason && data.emptyReason !== "FIELD_MASKED"
                        ? "当前无列表数据，可调整筛选后重试"
                        : isAudit &&
                            data.auditCoverageFrom &&
                            data.auditCoverageTo
                          ? `共 ${rows.length} 条 · 覆盖 ${formatDateTime(data.auditCoverageFrom, "full")} ~ ${formatDateTime(data.auditCoverageTo, "full")} · 无记录不等于动作未发生`
                          : `共 ${rows.length} 条`
                }
                toolbar={listToolbar}
                table={
                    data.emptyReason && data.emptyReason !== "FIELD_MASKED" ? (
                        <EmptyByReason
                            reason={data.emptyReason}
                            onClearFilters={
                                data.emptyReason === "FILTER_NO_RESULT"
                                    ? clearFilters
                                    : undefined
                            }
                        />
                    ) : view === "roles" ? (
                        <DataTable
                            columns={roleColumns}
                            data={pagedRows as RoleRow[]}
                            getRowId={(row) => row.id}
                            rowCount={rows.length}
                            pagination={pagination}
                            onPaginationChange={handlePaginationChange}
                            layout="flush"
                            density="compact"
                            loading={
                                pageQuery.isFetching && !pageQuery.isPending
                            }
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
                            onPaginationChange={handlePaginationChange}
                            layout="flush"
                            density="compact"
                            loading={
                                pageQuery.isFetching && !pageQuery.isPending
                            }
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
                            onPaginationChange={handlePaginationChange}
                            layout="flush"
                            density="compact"
                            loading={
                                pageQuery.isFetching && !pageQuery.isPending
                            }
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
                            onPaginationChange={handlePaginationChange}
                            layout="flush"
                            density="compact"
                            loading={
                                pageQuery.isFetching && !pageQuery.isPending
                            }
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
                            onPaginationChange={handlePaginationChange}
                            layout="flush"
                            density="compact"
                            loading={
                                pageQuery.isFetching && !pageQuery.isPending
                            }
                            showRefreshingBanner={pageQuery.isFetching}
                            defaultColumnPinning={{
                                left: ["time"],
                                right: ["actions"],
                            }}
                        />
                    )
                }
            />
            <AccessPreviewSheets
                explainSubject={explainSubject}
                eventOpenId={eventOpenId}
                effectiveQuery={effectiveQuery}
                eventQuery={eventQuery}
                closeExplain={closeExplain}
                closeEvent={closeEvent}
                restoreRowFocus={restoreRowFocus}
            />
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
                                selectionScope={
                                    impact.affectedWorkSurfaceSummary
                                }
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
                                sensitiveFields={[
                                    "密钥",
                                    "卡密",
                                    "完整银行账号",
                                ]}
                                skippedReason={
                                    impact.reviewPolicyBlocker?.message
                                }
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
                                <AlertDescription>
                                    {impact.riskSummary}
                                </AlertDescription>
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
                                        <Label htmlFor="w19-reason">
                                            变更原因
                                        </Label>
                                        <form.AppField
                                            name="reasonCode"
                                            children={(field) => (
                                                <OptionCombobox
                                                    id="w19-reason"
                                                    value={field.state.value}
                                                    onValueChange={(v) =>
                                                        field.handleChange(
                                                            v ??
                                                                field.state
                                                                    .value,
                                                        )
                                                    }
                                                    options={[
                                                        {
                                                            value: "SECURITY_OPS",
                                                            label: "安全运维",
                                                        },
                                                        {
                                                            value: "EMERGENCY_STOP_LOSS",
                                                            label: "紧急止损",
                                                        },
                                                        {
                                                            value: "ORG_CHANGE",
                                                            label: "组织调整",
                                                        },
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
                                        <Label htmlFor="w19-comment">
                                            说明（可选，勿填密钥）
                                        </Label>
                                        <form.AppField
                                            name="comment"
                                            children={(field) => (
                                                <Textarea
                                                    id="w19-comment"
                                                    value={
                                                        field.state.value ?? ""
                                                    }
                                                    onChange={(e) =>
                                                        field.handleChange(
                                                            e.target.value,
                                                        )
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
                                                code: impact
                                                    .reviewPolicyBlocker!.code,
                                                message:
                                                    impact.reviewPolicyBlocker!
                                                        .message,
                                                actionBlockers: [
                                                    impact.reviewPolicyBlocker!,
                                                ],
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
                            ? (accountForm.account?.id ?? "edit")
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
        </PageScaffold>
    )
}
