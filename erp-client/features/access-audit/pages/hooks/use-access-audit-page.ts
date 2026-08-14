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
import { useAccessChangeFlow } from "@/features/access-audit/pages/hooks/use-access-change-flow"
import {
    useAccessDetailPanels,
    type ExplainSubject,
} from "@/features/access-audit/pages/hooks/use-access-detail-panels"
import {
    useAccessListControls,
    type DebouncedAuditFilters,
} from "@/features/access-audit/pages/hooks/use-access-list-controls"
import { useAccessUrlState } from "@/features/access-audit/pages/hooks/use-access-url-state"
import { buildAccessListQuery } from "@/features/access-audit/pages/lib/build-list-query"
import type { AccountDraft } from "@/features/admin/account-form-dialog"
import { useAssignableRolesQuery } from "@/features/admin/queries"
import type { AccessView } from "@/features/access-audit/types"

export type { ExplainSubject }

export type AccountFormState = {
    mode: "create" | "edit"
    account: AccountDraft | null
}

export type DeletingAccountState = {
    id: string
    account: string
}

export type DeletingRoleState = {
    id: string
    name: string
}

export type { DebouncedAuditFilters }

function useAccessAuditPage() {
    const {
        router,
        pathname,
        searchParams,
        view,
        qParam,
        status,
        org,
        risk,
        subjectTypeParam,
        subjectIdParam,
        eventIdParam,
        fromParam,
        toParam,
        actorId,
        action,
        objectType,
        objectId,
        resultFilter,
        traceId,
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

    /** 筛选变更统一回到第一页：避免第二页起筛选变窄后「共 N 条」与空态并存。 */
    const patchFilterUrl = (
        patch: Record<string, string | null | undefined>,
    ) => {
        patchUrl({ ...patch, page: null }, { replace: true })
        setPagination((p) => (p.pageIndex === 0 ? p : { ...p, pageIndex: 0 }))
    }

    const {
        searchInput,
        searchInputRef,
        setSearchInput,
        debouncedFilters,
        setDebouncedFilters,
    } = useAccessListControls({
        qParam,
        patchUrl,
        patchFilterUrl,
        resetPaginationToFirstPage: () =>
            setPagination((p) =>
                p.pageIndex === 0 ? p : { ...p, pageIndex: 0 },
            ),
    })

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
                q: qParam,
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
    // 账号表单角色选项：仅当前操作者可分配的角色（API 层失败时回落全部角色）
    const assignableRolesQuery = useAssignableRolesQuery()

    const data = pageQuery.data
    const policies = data?.governancePolicies

    const changeFlow = useAccessChangeFlow({
        setActionError,
        setLastResult,
    })
    const startChange = changeFlow.startChange

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
    const isAudit = view === "audit"
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

    const routerPush = router.push

    return {
        // URL 与视图
        routerPush,
        view,
        isAudit,
        rejectedWorkItemId,
        qParam,
        status,
        org,
        risk,
        fromParam,
        toParam,
        action,
        resultFilter,
        actorId,
        traceId,
        objectType,
        objectId,
        advancedAuditActive,
        // 查询
        pageQuery,
        data,
        effectiveQuery,
        eventQuery,
        assignableRolesQuery,
        // 列表状态
        pagination,
        searchInput,
        searchInputRef,
        setSearchInput,
        debouncedFilters,
        setDebouncedFilters,
        hasActiveFilters,
        patchUrl,
        patchFilterUrl,
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
