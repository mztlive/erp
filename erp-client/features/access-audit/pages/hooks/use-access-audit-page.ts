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
import type {
    AccountFormState,
    DeletingAccountState,
    DeletingRoleState,
} from "@/features/access-audit/hooks/access-columns-input"
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
    riskFilterLabel,
    statusFilterLabel,
} from "@/features/access-audit/lib/filter-options"
import { buildAccessListQuery } from "@/features/access-audit/pages/lib/build-list-query"
import { useAssignableRolesQuery } from "@/features/admin/queries"
import type { AccessView } from "@/features/access-audit/types"

export type { ExplainSubject }

function useAccessAuditPage() {
    const {
        router,
        searchParams,
        view,
        subjectTypeParam,
        subjectIdParam,
        eventIdParam,
        rejectedWorkItemId,
        patchUrl,
    } = useAccessUrlState()

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
    // 账号新建 / 编辑弹窗（账号字段少，弹窗足够；角色编辑走整页表单）
    const [accountForm, setAccountForm] = React.useState<AccountFormState | null>(
        null,
    )
    const [deletingAccount, setDeletingAccount] =
        React.useState<DeletingAccountState | null>(null)
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

    const listQuery = React.useMemo(
        () =>
            buildAccessListQuery({
                view,
                q: applied.q,
                status: applied.status,
                org: applied.org,
                risk: applied.risk,
                subjectType: subjectTypeParam,
                subjectId: subjectIdParam,
                from: applied.from,
                to: applied.to,
                actorId: applied.actorId,
                action: applied.action,
                objectType: applied.objectType,
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
    const isScopes = view === "scopes"
    const hasActiveFilters = isAudit
        ? Boolean(
              applied.q ||
                  applied.from ||
                  applied.to ||
                  applied.action ||
                  applied.result ||
                  applied.actorId ||
                  applied.traceId ||
                  applied.objectType ||
                  applied.objectId,
          )
        : Boolean(
              applied.q ||
                  applied.org ||
                  applied.status ||
                  applied.risk ||
                  // 数据范围视图的来源锁定主体也是查询消费参数
                  (isScopes && Boolean(subjectTypeParam || subjectIdParam)),
          )

    // 组织维度选项：取自当前列表的角色/用户组织标签；选中值不在选项时并入，保证可回退
    const orgOptions = React.useMemo(() => {
        const seen = new Set<string>()
        const options: { value: string; label: string }[] = []
        for (const row of [...(data?.roles ?? []), ...(data?.users ?? [])]) {
            const label = row.organizationLabel
            if (!label || seen.has(label)) continue
            seen.add(label)
            options.push({ value: label, label })
        }
        if (applied.org && !seen.has(applied.org)) {
            options.unshift({ value: applied.org, label: applied.org })
        }
        return options
    }, [applied.org, data])

    const subjectLabel = React.useMemo(() => {
        if (!subjectIdParam) return ""
        if (subjectTypeParam === "USER" || view === "users") {
            return (
                data?.users.find((user) => user.id === subjectIdParam)
                    ?.displayName ?? subjectIdParam
            )
        }
        return (
            data?.roles.find((role) => role.id === subjectIdParam)?.name ??
            subjectIdParam
        )
    }, [data, subjectIdParam, subjectTypeParam, view])

    /** 已生效条件全部显性化为可移除 chip（含来源锁定主体）。 */
    const appliedChips = React.useMemo<readonly AccessAppliedChip[]>(() => {
        const chips: AccessAppliedChip[] = []
        const trimmedQ = applied.q.trim()
        if (trimmedQ) chips.push({ key: "q", label: `搜索：${trimmedQ}` })
        if (isScopes && subjectTypeParam && subjectIdParam) {
            chips.push({
                key: "subject",
                label: `范围主体：${subjectLabel}`,
            })
        }
        if (applied.org) {
            const label =
                orgOptions.find((option) => option.value === applied.org)
                    ?.label ?? applied.org
            chips.push({ key: "org", label: `组织：${label}` })
        }
        if (applied.status) {
            chips.push({
                key: "status",
                label: `状态：${statusFilterLabel(applied.status)}`,
            })
        }
        if (applied.risk) {
            chips.push({
                key: "risk",
                label: `风险：${riskFilterLabel(applied.risk)}`,
            })
        }
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
        if (applied.objectType) {
            chips.push({
                key: "objectType",
                label: `对象类型：${applied.objectType}`,
            })
        }
        if (applied.objectId) {
            chips.push({
                key: "objectId",
                label: `对象编号：${applied.objectId}`,
            })
        }
        return chips
    }, [
        applied,
        isScopes,
        orgOptions,
        subjectIdParam,
        subjectLabel,
        subjectTypeParam,
    ])

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
        orgOptions,
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
        // 账号 / 角色管理弹窗
        accountForm,
        setAccountForm,
        deletingAccount,
        setDeletingAccount,
        deletingRole,
        setDeletingRole,
        // 表格列
        auditColumns,
        fieldColumns,
        roleColumns,
        scopeColumns,
        userColumns,
    }
}

export { useAccessAuditPage }
