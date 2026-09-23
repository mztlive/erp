import { QueryClient, type Query } from "@tanstack/react-query"

function numberField(
    data: Record<string, unknown>,
    key: string,
): number | undefined {
    const value = data[key]
    return typeof value === "number" ? value : undefined
}

function stringField(
    data: Record<string, unknown>,
    key: string,
): string | undefined {
    const value = data[key]
    return typeof value === "string" ? value : undefined
}

/** 会话身份和全局权限版本变化必须清除其他功能已缓存的业务内容。 */
function authorizationVersion(data: unknown): string | null {
    if (!data || typeof data !== "object") return null
    const value = data as Record<string, unknown>
    if (typeof value.userid !== "string") return null
    return JSON.stringify([
        value.userid,
        value.policy_version,
        value.organization_version,
        value.role_ids,
        value.permissions,
    ])
}

/** 跨页或导出范围变化必须清缓存并从第一页重查。 */
export function isDataScopeChanged(error: unknown): boolean {
    if (!error || typeof error !== "object") return false
    const value = error as { status?: number; code?: string; message?: string }
    return (
        value.code === "DATA_SCOPE_CHANGED" ||
        (value.status === 409 &&
            value.message?.startsWith("DATA_SCOPE_CHANGED") === true)
    )
}

function scopeFailure(error: unknown): boolean {
    if (!error || typeof error !== "object") return false
    const value = error as { status?: number; code?: string; message?: string }
    return (
        value.status === 401 ||
        value.status === 403 ||
        value.status === 404 ||
        isDataScopeChanged(error)
    )
}

const isProfile = (query: Query) =>
    query.queryKey[0] === "account" && query.queryKey[1] === "profile"

/** 独立目录按类别隔离；业务查询按所属资源失效，不跨到其他查询族。 */
function scopeFamily(query: Query): string {
    const key = query.queryKey
    if (key[0] === "customer-receivables" &&
        (key[1] === "counterparty-options" || key[1] === "counterparty-selected")) {
        return JSON.stringify([key[0], "counterparty-directory"])
    }
    if (key[0] === "master-data" && key[1] === "product-filter-options") {
        return JSON.stringify(key.slice(0, 2))
    }
    const depth = key[0] === "entity-selectors"
        ? (key[1] === "person-directory" ? 3 : 2)
        : key[0] === "historical-directory" ? 2 : 1
    return JSON.stringify(key.slice(0, depth))
}

/**
 * 绑定 QueryClient 的范围失效订阅。
 * 普通 403/404、目录内容版本变化只清除同类查询；确认全局授权变化才跨功能清理。
 * 成功的业务变更清除其他缓存并重读活动查询，覆盖责任交接与配置变更。
 */
export function subscribeScopeCache(client: QueryClient): () => void {
    let version: string | null = null
    let policy: number | undefined
    let organization: number | undefined
    const pendingClears: { source: Query | undefined; error?: unknown; local: boolean }[] = []
    const scopeVersions = new Map<string, string>()
    let clearing = false
    let disposed = false
    const rejected = new Set<string>()

    const clear = async (source: Query | undefined, error?: unknown, local = false) => {
        if (disposed) return
        if (clearing) {
            pendingClears.push({ source, error, local })
            return
        }
        clearing = true
        try {
            const matches = (query: Query) =>
                query !== source && !isProfile(query) &&
                (!local || (source != null && scopeFamily(query) === scopeFamily(source)))
            // 先取消旧请求；取消完成之前也立即撤下已经显示的旧数据。
            const pending = client.cancelQueries(
                { predicate: matches },
                { revert: false },
            )
            for (const query of client
                .getQueryCache()
                .findAll({ predicate: matches })) {
                if (query.getObserversCount() === 0) {
                    client.removeQueries({
                        queryKey: query.queryKey,
                        exact: true,
                    })
                } else {
                    query.setState({
                        data: undefined,
                        dataUpdatedAt: 0,
                        status: "error",
                        error:
                            error instanceof Error
                                ? error
                                : new Error("数据范围已更新，请重新查询"),
                        fetchStatus: "idle",
                    })
                }
            }
            await pending
            if (!disposed && !error) {
                // 不等待网络完成，后续全局撤权必须能立即取消这些重查。
                void client.refetchQueries({
                    predicate: matches,
                    type: "active",
                })
            }
        } finally {
            clearing = false
            const pending = pendingClears.shift()
            if (pending) void clear(pending.source, pending.error, pending.local)
        }
    }

    const handleFailure = (error: unknown, source?: Query) => {
        const status = (error as { status?: number }).status
        if (status === 401) {
            void clear(source, error)
            return
        }
        if (source) void clear(source, error, true)
        // 某个资源不可读，不代表其他资源也不可读。重验账号版本来识别真正撤权。
        void client.refetchQueries({ predicate: isProfile, type: "active" })
    }

    const unsubscribeQueries = client.getQueryCache().subscribe((event) => {
        if (event.type === "removed")
            scopeVersions.delete(event.query.queryHash)
        if (disposed || event.type !== "updated") return
        const query = event.query
        if (event.action.type === "success") {
            rejected.delete(query.queryHash)
            const data = query.state.data
            if (!data || typeof data !== "object") return
            const fields = data as Record<string, unknown>
            const nextPolicy =
                numberField(fields, "policyVersion") ??
                numberField(fields, "policy_version")
            const nextOrganization =
                numberField(fields, "organizationVersion") ??
                numberField(fields, "organization_version")
            // 已观察到新版本后，迟到或被中间缓存返回的旧版本不得成为新的可见数据。
            if (
                (nextPolicy != null && policy != null && nextPolicy < policy) ||
                (nextOrganization != null &&
                    organization != null &&
                    nextOrganization < organization)
            ) {
                const error = Object.assign(
                    new Error("DATA_SCOPE_CHANGED：数据范围已变化，请重新查询"),
                    {
                        status: 409,
                        code: "DATA_SCOPE_CHANGED",
                    },
                )
                query.setState({
                    data: undefined,
                    dataUpdatedAt: 0,
                    status: "error",
                    error,
                })
                void clear(query, error)
                return
            }
            let changed = false
            if (nextPolicy != null) {
                changed ||= policy != null && nextPolicy > policy
                policy = Math.max(policy ?? nextPolicy, nextPolicy)
            }
            if (nextOrganization != null) {
                changed ||=
                    organization != null && nextOrganization > organization
                organization = Math.max(
                    organization ?? nextOrganization,
                    nextOrganization,
                )
            }
            const nextScope =
                stringField(fields, "scopeVersion") ??
                stringField(fields, "scope_version")
            if (nextScope != null) {
                const previous = scopeVersions.get(query.queryHash)
                if (!changed && previous != null && previous !== nextScope) {
                    void clear(query, undefined, true)
                }
                scopeVersions.set(query.queryHash, nextScope)
            }
            const next = authorizationVersion(data)
            if (next != null) {
                changed ||= version != null && version !== next
                version = next
            }
            if (changed) void clear(query)
        }
        if (event.action.type === "error" && scopeFailure(query.state.error)) {
            // 同一失败在重新成功前只处理一次，避免重试导致全站刷新循环。
            if (rejected.has(query.queryHash)) return
            rejected.add(query.queryHash)
            const error = query.state.error
            query.setState({ data: undefined, dataUpdatedAt: 0 })
            handleFailure(error, query)
        }
    })
    const unsubscribeMutations = client
        .getMutationCache()
        .subscribe((event) => {
            if (
                event.type === "updated" &&
                event.action.type === "success" &&
                event.mutation.options.meta?.affectsDataScope === true
            ) {
                void clear(undefined)
            }
            if (
                event.type === "updated" &&
                event.action.type === "error" &&
                scopeFailure(event.mutation.state.error)
            ) {
                handleFailure(event.mutation.state.error)
            }
        })
    return () => {
        disposed = true
        unsubscribeQueries()
        unsubscribeMutations()
    }
}
