"use client"

import * as React from "react"
import type { PaginationState } from "@tanstack/react-table"

import { type ResultState } from "@/components/business/feedback"
import {
    useAccessListQuery,
    useAuditEventQuery,
    useEffectiveAccessQuery,
} from "@/features/access-audit/hooks/queries"
import { useAccessColumns } from "@/features/access-audit/hooks/use-access-columns"
import type { DeletingRoleState } from "@/features/access-audit/hooks/access-columns-input"
import type { RoleAssignmentTarget } from "@/features/access-audit/components/role-assignment-dialog"
import { useAccessChangeFlow } from "@/features/access-audit/pages/hooks/use-access-change-flow"
import {
    useAccessDetailPanels,
    type ExplainSubject,
} from "@/features/access-audit/pages/hooks/use-access-detail-panels"
import { useAccessListFilters } from "@/features/access-audit/pages/hooks/use-access-list-filters"
import { useAccessUrlState } from "@/features/access-audit/pages/hooks/use-access-url-state"
import type { AccessAppliedChip } from "@/features/access-audit/components/access-list-toolbar"
import {
    actionFilterLabel,
    resultFilterLabel,
} from "@/features/access-audit/lib/filter-options"
import { buildAccessListQuery } from "@/features/access-audit/pages/lib/build-list-query"
import { useAssignableRolesQuery } from "@/features/admin/queries"
import type { AccessView } from "@/features/access-audit/types"

export type { ExplainSubject }

/**
 * 权限与审计页面状态。
 *
 * @param surface `access` = 权限配置（角色 / 用户授权）；`audit` = 审计查询独立页，
 * 视图由路由固定，不再由 URL 的 view 参数决定。
 */
function useAccessAuditPage(surface: "access" | "audit" = "access") {
    const {
        router,
        searchParams,
        view,
        subjectTypeParam,
        subjectIdParam,
        eventIdParam,
        rejectedWorkItemId,
        patchUrl,
    } = useAccessUrlState(surface === "audit" ? "audit" : undefined)

    /** 旧链接兼容：权限配置页收到 `view=audit` 时转到独立的审计查询页。 */
    const legacyAuditView =
        surface === "access" && searchParams.get("view") === "audit"
    React.useEffect(() => {
        if (!legacyAuditView) return
        const next = new URLSearchParams(searchParams.toString())
        next.delete("view")
        next.delete("page")
        const query = next.toString()
        router.replace(`/system/audit${query ? `?${query}` : ""}`)
    }, [legacyAuditView, router, searchParams])

    const pageParamRaw = Number(searchParams.get("page"))
    const pageParamIndex =
        Number.isFinite(pageParamRaw) && pageParamRaw > 0
            ? Math.max(0, Math.floor(pageParamRaw) - 1)
            : 0
    const [pagination, setPagination] = React.useState<PaginationState>({
        pageIndex: pageParamIndex,
        pageSize: 20,
    })

    /** 筛选写 URL 统一出口：replace + scroll:false，并回到第一页。 */
    const patchFilterUrl = (
        patch: Record<string, string | null | undefined>,
    ) => {
        patchUrl(patch, { replace: true, scroll: false })
        setPagination((p) => (p.pageIndex === 0 ? p : { ...p, pageIndex: 0 }))
    }

    const searchInputRef = React.useRef<HTMLInputElement | null>(null)
    const filters = useAccessListFilters({
        view,
        patchFilterUrl,
        searchInputRef,
    })
    const {
        applied,
        draft,
        updateDraft,
        searchDraft,
        setSearchDraft,
        panelOpen,
        setPanelOpen,
        hasStructuredFilters,
        applyFilters,
        resetMoreFilters,
        clearAllFilters,
        removeFilter,
        filterError,
    } = filters

    const [lastResult, setLastResult] = React.useState<ResultState>(null)
    const [actionError, setActionError] = React.useState<string | null>(null)
    // 角色绑定弹窗（账号资料与密码在「账号管理」维护，不在权限工作面）
    const [roleAssignment, setRoleAssignment] =
        React.useState<RoleAssignmentTarget | null>(null)
    const [deletingRole, setDeletingRole] =
        React.useState<DeletingRoleState | null>(null)

    const {
        explainSubject,
        setExplainSubject,
        eventOpenId,
        setEventOpenId,
        rowFocusRef,
        restoreRowFocus,
        openExplain,
        closeExplain,
        openEvent,
        closeEvent,
    } = useAccessDetailPanels({
        view,
        subjectTypeParam,
        subjectIdParam,
        eventIdParam,
        patchUrl,
    })

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
            // 弹层（Dialog / Sheet）打开时不聚焦背景搜索框
            if (
                document.querySelector(
                    '[role="dialog"], [data-slot="sheet"]',
                )
            ) {
                return
            }
            event.preventDefault()
            searchInputRef.current?.focus()
        }
        window.addEventListener("keydown", onKey)
        return () => window.removeEventListener("keydown", onKey)
        // eslint-disable-next-line react-hooks/exhaustive-deps
    }, [])

    /** 本地时区的 YYYY-MM-DD，供日期筛选控件与 URL 使用。 */
    const toDateInput = (date: Date) => {
        const pad = (value: number) => String(value).padStart(2, "0")
        return `${date.getFullYear()}-${pad(date.getMonth() + 1)}-${pad(date.getDate())}`
    }

    /**
     * 审计查询默认窗口：首次进入且 URL 未带时间范围时落最近 7 天。
     * 只在挂载时写一次，用户清空后不再自动补回。
     */
    const auditWindowApplied = React.useRef(false)
    React.useEffect(() => {
        if (surface !== "audit" || auditWindowApplied.current) return
        auditWindowApplied.current = true
        if (applied.from || applied.to) return
        const today = new Date()
        const start = new Date(today)
        start.setDate(start.getDate() - 6)
        patchFilterUrl({
            from: toDateInput(start),
            to: toDateInput(today),
        })
        // eslint-disable-next-line react-hooks/exhaustive-deps
    }, [surface])

    const listQuery = React.useMemo(
        () =>
            buildAccessListQuery({
                view,
                q: applied.q,
                subjectType: subjectTypeParam,
                subjectId: subjectIdParam,
                from: applied.from,
                to: applied.to,
                actorId: applied.actorId,
                action: applied.action,
                objectId: applied.objectId,
                result: applied.result,
                traceId: applied.traceId,
            }),
        [
            view,
            applied,
            subjectTypeParam,
            subjectIdParam,
        ],
    )

    const pageQuery = useAccessListQuery(listQuery)
    const effectiveQuery = useEffectiveAccessQuery(
        explainSubject?.type ?? null,
        explainSubject?.id ?? null,
    )
    const eventQuery = useAuditEventQuery(eventOpenId)
    // 账号表单角色选项：仅当前操作者可分配的角色（API 层失败时回落全部角色）
    const assignableRolesQuery = useAssignableRolesQuery()

    const data = pageQuery.data
    const policies = data?.governancePolicies

    const changeFlow = useAccessChangeFlow({
        setActionError,
        setLastResult,
    })
    const startChange = changeFlow.startChange

    const clearFilters = clearAllFilters

    const exportAction =
        surface === "audit" ? "EXPORT_AUDIT" : "EXPORT_CONFIGURATION"
    const exportBlocked = !data?.allowedActions.includes(exportAction)
    const exportBlocker = data?.actionBlockers.find(
        (b) => b.action === exportAction,
    )

    // ── columns ──
    const { auditColumns, roleColumns, userColumns } = useAccessColumns({
        data,
        policies,
        router,
        rowFocusRef,
        openExplain,
        openEvent,
        startChange,
        setRoleAssignment,
        setDeletingRole,
    })

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

    const isAudit = view === "audit"
    const hasActiveFilters = isAudit
        ? Boolean(
              applied.q ||
                  applied.from ||
                  applied.to ||
                  applied.action ||
                  applied.result ||
                  applied.actorId ||
                  applied.traceId ||
                  applied.objectId,
          )
        : Boolean(applied.q)

    /** 动作选项：取自当前查询结果里出现过的动作，保证选项都能筛出记录。 */
    const actionOptions = React.useMemo(() => {
        const seen = new Set<string>()
        const options: { value: string; label: string }[] = []
        for (const event of data?.auditEvents ?? []) {
            if (!event.actionType || seen.has(event.actionType)) continue
            seen.add(event.actionType)
            options.push({
                value: event.actionType,
                label: actionFilterLabel(event.actionType),
            })
        }
        if (applied.action && !seen.has(applied.action)) {
            options.unshift({
                value: applied.action,
                label: actionFilterLabel(applied.action),
            })
        }
        return options.sort((a, b) => a.label.localeCompare(b.label))
    }, [applied.action, data?.auditEvents])

    /** 已生效条件全部显性化为可移除 chip。 */
    const appliedChips = React.useMemo<readonly AccessAppliedChip[]>(() => {
        const chips: AccessAppliedChip[] = []
        const trimmedQ = applied.q.trim()
        if (trimmedQ) chips.push({ key: "q", label: `搜索：${trimmedQ}` })
        if (applied.from || applied.to) {
            chips.push({
                key: "time",
                label: `时间：${applied.from ?? "不限"} 至 ${applied.to ?? "不限"}`,
            })
        }
        if (applied.action) {
            chips.push({
                key: "action",
                label: `动作：${actionFilterLabel(applied.action)}`,
            })
        }
        if (applied.result) {
            chips.push({
                key: "result",
                label: `结果：${resultFilterLabel(applied.result)}`,
            })
        }
        if (applied.actorId) {
            chips.push({ key: "actorId", label: `操作者：${applied.actorId}` })
        }
        if (applied.traceId) {
            chips.push({
                key: "traceId",
                label: `请求追踪号：${applied.traceId}`,
            })
        }
        if (applied.objectId) {
            chips.push({
                key: "objectId",
                label: `对象编号：${applied.objectId}`,
            })
        }
        return chips
    }, [applied])

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

    const routerPush = router.push

    return {
        // URL 与视图
        routerPush,
        view,
        isAudit,
        rejectedWorkItemId,
        // 筛选状态（Applied / Draft / UI）
        applied,
        draft,
        updateDraft,
        searchDraft,
        setSearchDraft,
        searchInputRef,
        panelOpen,
        setPanelOpen,
        hasStructuredFilters,
        applyFilters,
        resetMoreFilters,
        removeFilter,
        filterError,
        appliedChips,
        actionOptions,
        // 查询
        pageQuery,
        data,
        effectiveQuery,
        eventQuery,
        assignableRolesQuery,
        // 列表状态
        pagination,
        hasActiveFilters,
        patchUrl,
        clearFilters,
        handlePaginationChange,
        switchView,
        handleExport,
        exportBlocked,
        exportBlocker,
        // 详情弹层
        explainSubject,
        eventOpenId,
        openExplain,
        closeExplain,
        openEvent,
        closeEvent,
        restoreRowFocus,
        // 变更预览（useAccessChangeFlow）
        ...changeFlow,
        lastResult,
        setLastResult,
        actionError,
        // 角色绑定 / 角色删除弹窗
        roleAssignment,
        setRoleAssignment,
        deletingRole,
        setDeletingRole,
        // 表格列
        auditColumns,
        roleColumns,
        userColumns,
    }
}

export { useAccessAuditPage }
