/**
 * 销售选品领域 Hooks：查询与变更统一入口。
 * 组件只消费本模块 hooks，不直接调用 api/。
 */

"use client"

import { useMutation, useQuery, useQueryClient } from "@tanstack/react-query"
import { toast } from "@/components/ui/toast"
import {
    closeBook,
    copyBookLink,
    createBook,
    deleteDisplayItem,
    fetchBookDetail,
    fetchBooks,
    publishBook,
    replaceBookLink,
    revokeBookLink,
    fetchActionableBookCount,
    startPrepareTask,
    voidBook,
    type PrepareKind,
} from "@/features/sales-selection/api/books"
import { fetchBookSession } from "@/features/sales-selection/api/sessions"
import {
    fetchProposalByBook,
    fetchProposalDetail,
} from "@/features/sales-selection/api/proposals"
import {
    fetchPublicReceipt,
    fetchPublicSelection,
    savePublicSession,
    submitPublicSelection,
} from "@/features/sales-selection/api/public"
import type {
    BookListQuery,
    CreateBookInput,
    PoolFilterSnapshot,
} from "@/features/sales-selection/types"

/** 选品领域缓存键：list / detail / preview / proposal 分层。 */
export const salesSelectionKeys = {
    all: ["sales-selection"] as const,
    list: (query: BookListQuery) =>
        [...salesSelectionKeys.all, "list", query] as const,
    detail: (bookId: string) =>
        [...salesSelectionKeys.all, "detail", bookId] as const,
    preview: (token: string) =>
        [...salesSelectionKeys.all, "preview", token] as const,
    proposal: (proposalId: string) =>
        [...salesSelectionKeys.all, "proposal", proposalId] as const,
    session: (bookId: string) =>
        [...salesSelectionKeys.all, "session", bookId] as const,
    actionableCount: () =>
        [...salesSelectionKeys.all, "actionable-count"] as const,
}

/** 详情与列表同时失效。 */
const invalidateBookCaches = async (
    queryClient: ReturnType<typeof useQueryClient>,
    bookId?: string,
) => {
    await Promise.all([
        queryClient.invalidateQueries({ queryKey: salesSelectionKeys.all }),
        bookId
            ? queryClient.invalidateQueries({
                  queryKey: salesSelectionKeys.detail(bookId),
              })
            : Promise.resolve(),
    ])
}

/** 选品册列表查询。 */
export const useBooks = (query: BookListQuery) =>
    useQuery({
        queryKey: salesSelectionKeys.list(query),
        queryFn: () => fetchBooks(query),
    })

/**
 * 侧栏「选品册」待处理角标。
 * @param enabled 无列表权限时不请求
 */
export const useActionableBookCountQuery = (enabled = true) =>
    useQuery({
        queryKey: salesSelectionKeys.actionableCount(),
        queryFn: fetchActionableBookCount,
        enabled,
        refetchInterval: 30_000,
    })

/** 选品册详情查询。 */
export const useBookDetail = (bookId: string) =>
    useQuery({
        queryKey: salesSelectionKeys.detail(bookId),
        queryFn: () => fetchBookDetail(bookId),
        enabled: Boolean(bookId),
    })

/** 内部会话快照查询（销售查看客户已保存选择）。 */
export const useBookSession = (bookId: string, enabled = false) =>
    useQuery({
        queryKey: salesSelectionKeys.session(bookId),
        queryFn: () => fetchBookSession(bookId),
        enabled: Boolean(bookId) && enabled,
    })

/** 方案详情查询（内部只读）。 */
export const useProposalDetail = (proposalId: string) =>
    useQuery({
        queryKey: salesSelectionKeys.proposal(proposalId),
        queryFn: () => fetchProposalDetail(proposalId),
        enabled: Boolean(proposalId),
    })

/** 按选品册查询其唯一方案（已提交行链方案详情用）。 */
export const useProposalByBook = (bookId: string, enabled = false) =>
    useQuery({
        queryKey: [...salesSelectionKeys.proposal(bookId), "by-book"] as const,
        queryFn: () => fetchProposalByBook(bookId),
        enabled: Boolean(bookId) && enabled,
    })

/** 公开选品会话查询（客户页事实来源，不带员工鉴权）。 */
export const usePublicSelection = (token: string) =>
    useQuery({
        queryKey: salesSelectionKeys.preview(token),
        queryFn: () => fetchPublicSelection(token),
        enabled: Boolean(token),
        retry: 1,
    })

/** 公开回执查询（已提交且链接仍有效时）。 */
export const usePublicReceipt = (token: string, enabled = false) =>
    useQuery({
        queryKey: [...salesSelectionKeys.preview(token), "receipt"] as const,
        queryFn: () => fetchPublicReceipt(token),
        enabled: Boolean(token) && enabled,
        retry: 1,
    })

/** 册级操作集合：创建/准备/重生成/发布/链接/关闭/撤销/作废/删项/复制链接。 */
export const useBookOperations = () => {
    const queryClient = useQueryClient()

    /** 创建选品册。 */
    const create = useMutation({
        mutationFn: (input: CreateBookInput & { silent?: boolean }) => {
            const { silent: _silent, ...payload } = input
            return createBook(payload)
        },
        onSuccess: async (_data, variables) => {
            queryClient.setQueryData(
                salesSelectionKeys.actionableCount(),
                (previous: number | undefined) => (previous ?? 0) + 1,
            )
            await queryClient.invalidateQueries({
                queryKey: salesSelectionKeys.all,
            })
            if (variables.silent) return
            toast.add({
                title: "选品册已创建",
                description: "正在准备陈列，可在选品册中查看进度。",
                type: "success",
                timeout: 4000,
            })
        },
    })

    /** 启动准备任务（含首次准备与重生成）。 */
    const prepare = useMutation({
        mutationFn: (input: {
            bookId: string
            kind: PrepareKind
            tier_ids?: readonly string[]
            filter?: PoolFilterSnapshot
            sku_ids?: readonly string[]
            tiers?: CreateBookInput["tiers"]
            expected_version: number
            idempotency_key: string
            silent?: boolean
        }) => {
            const { silent: _silent, ...payload } = input
            return startPrepareTask(payload.bookId, {
                kind: payload.kind,
                tier_ids: payload.tier_ids,
                filter: payload.filter,
                sku_ids: payload.sku_ids,
                tiers: payload.tiers,
                expected_version: payload.expected_version,
                idempotency_key: payload.idempotency_key,
            })
        },
        onSuccess: async (_data, variables) => {
            await invalidateBookCaches(queryClient, variables.bookId)
            if (variables.silent) return
            toast.add({
                title: "准备任务已启动",
                description: "正在冻结商品池并生成陈列，请稍后查看进度。",
                type: "success",
                timeout: 4000,
            })
        },
    })

    /** 整册重新准备（固定来源类型内改筛选/勾选/档位）。 */
    const reprepare = useMutation({
        mutationFn: (input: {
            bookId: string
            filter?: PoolFilterSnapshot
            sku_ids?: readonly string[]
            tiers?: CreateBookInput["tiers"]
            expected_version: number
            idempotency_key: string
        }) =>
            startPrepareTask(input.bookId, {
                kind: "RE_PREPARE",
                filter: input.filter,
                sku_ids: input.sku_ids,
                tiers: input.tiers,
                expected_version: input.expected_version,
                idempotency_key: input.idempotency_key,
            }),
        onSuccess: async (_data, variables) => {
            await invalidateBookCaches(queryClient, variables.bookId)
            toast.add({
                title: "已发起重新准备",
                description: "新结果通过后将整体替换当前陈列。",
                type: "success",
                timeout: 4000,
            })
        },
    })

    /** 按档或整册重生成套餐（沿用当前批次商品池）。 */
    const regenerate = useMutation({
        mutationFn: (input: {
            bookId: string
            tier_ids?: readonly string[]
            expected_version: number
            idempotency_key: string
        }) =>
            startPrepareTask(input.bookId, {
                kind: input.tier_ids?.length
                    ? "REGENERATED_TIERS"
                    : "REGENERATED_ALL",
                tier_ids: input.tier_ids,
                expected_version: input.expected_version,
                idempotency_key: input.idempotency_key,
            }),
        onSuccess: async (_data, variables) => {
            await invalidateBookCaches(queryClient, variables.bookId)
            toast.add({
                title: "重生成任务已启动",
                description: "沿用当前商品池，完成后替换对应档位陈列。",
                type: "success",
                timeout: 4000,
            })
        },
    })

    /** 发布选品册。 */
    const publish = useMutation({
        mutationFn: (input: {
            batch_id?: string
            bookId: string
            expected_version: number
            idempotency_key: string
        }) => publishBook(input.bookId, input),
        onSuccess: async (_data, variables) => {
            await invalidateBookCaches(queryClient, variables.bookId)
            toast.add({
                title: "选品册已发布",
                description: "公开链接已生成，可复制后发给客户。",
                type: "success",
                timeout: 4000,
            })
        },
    })

    /** 更换公开链接。 */
    const replaceLink = useMutation({
        mutationFn: (input: {
            bookId: string
            expected_version: number
            idempotency_key: string
        }) => replaceBookLink(input.bookId, input),
        onSuccess: async (_data, variables) => {
            copyLink.reset()
            await invalidateBookCaches(queryClient, variables.bookId)
            toast.add({
                title: "链接已更换",
                description: "原链接已失效，请使用新链接联系客户。",
                type: "success",
                timeout: 4000,
            })
        },
    })

    /** 关闭未提交选品册。 */
    const close = useMutation({
        mutationFn: (input: {
            bookId: string
            expected_version: number
            idempotency_key: string
        }) => closeBook(input.bookId, input),
        onSuccess: async (_data, variables) => {
            await invalidateBookCaches(queryClient, variables.bookId)
            toast.add({
                title: "选品册已关闭",
                description: "公开页将只展示结束态，不再接受提交。",
                type: "success",
                timeout: 4000,
            })
        },
    })

    /** 撤销已提交选品册的链接访问。 */
    const revoke = useMutation({
        mutationFn: (input: {
            bookId: string
            expected_version: number
            idempotency_key: string
        }) => revokeBookLink(input.bookId, input),
        onSuccess: async (_data, variables) => {
            await invalidateBookCaches(queryClient, variables.bookId)
            toast.add({
                title: "链接访问已撤销",
                description: "已提交方案不受影响，公开页不再展示内容。",
                type: "success",
                timeout: 4000,
            })
        },
    })

    /** 作废发布前选品册。 */
    const voidBookMutation = useMutation({
        mutationFn: (input: {
            bookId: string
            expected_version: number
            idempotency_key: string
        }) => voidBook(input.bookId, input),
        onSuccess: async (_data, variables) => {
            await invalidateBookCaches(queryClient, variables.bookId)
            toast.add({
                title: "选品册已作废",
                description: "该选品册已终止，不可恢复。",
                type: "success",
                timeout: 4000,
            })
        },
    })

    /** 删除单个陈列项。 */
    const deleteItem = useMutation({
        mutationFn: (input: {
            bookId: string
            itemId: string
            expected_version: number
        }) =>
            deleteDisplayItem(
                input.bookId,
                input.itemId,
                input.expected_version,
            ),
        onSuccess: async (_data, variables) => {
            await invalidateBookCaches(queryClient, variables.bookId)
            toast.add({
                title: "陈列项已删除",
                description: "发布前可继续删减，发布后将冻结。",
                type: "success",
                timeout: 4000,
            })
        },
    })

    /** 复制当前有效链接。 */
    const copyLink = useMutation({
        mutationFn: (bookId: string) => copyBookLink(bookId),
        onSuccess: async (data) => {
            const href = data.public_url.startsWith("http")
                ? data.public_url
                : `${window.location.origin}${data.public_url}`
            try {
                await navigator.clipboard.writeText(href)
                toast.add({
                    title: "链接已复制",
                    description: "请将链接发给该客户。",
                    type: "success",
                })
            } catch {
                toast.add({
                    title: "请手动复制链接",
                    description: href,
                    type: "info",
                    timeout: 0,
                })
            }
        },
    })

    return {
        create,
        prepare,
        regenerate,
        reprepare,
        publish,
        replaceLink,
        close,
        revoke,
        void: voidBookMutation,
        deleteItem,
        copyLink,
    }
}

/** 公开会话保存：携带会话版本、幂等键与完整选择。 */
export const useSessionSave = (token: string) => {
    const queryClient = useQueryClient()
    return useMutation({
        mutationFn: (input: {
            expected_session_version: number
            idempotency_key: string
            choices: { item_id: string; quantity?: number | null }[]
        }) =>
            savePublicSession({
                token,
                expected_session_version: input.expected_session_version,
                idempotency_key: input.idempotency_key,
                choices: input.choices,
            }),
        onSuccess: async (data) => {
            await queryClient.invalidateQueries({
                queryKey: salesSelectionKeys.preview(token),
            })
            toast.add({
                title: "选择已保存",
                description: `已保存 ${data.choices.length} 项，可继续调整或提交。`,
                type: "success",
                timeout: 4000,
            })
        },
    })
}

/** 公开提交：提交前展示后端确认清单版本，版本变化重示再提交。 */
export const useSubmit = (token: string) => {
    const queryClient = useQueryClient()
    return useMutation({
        mutationFn: (input: {
            expected_session_version: number
            idempotency_key: string
        }) =>
            submitPublicSelection({
                token,
                expected_session_version: input.expected_session_version,
                idempotency_key: input.idempotency_key,
            }),
        onSuccess: async () => {
            await queryClient.invalidateQueries({
                queryKey: salesSelectionKeys.preview(token),
            })
            toast.add({
                title: "选品已提交",
                description: "已生成销售方案，请保留回执以备核对。",
                type: "success",
                timeout: 6000,
            })
        },
    })
}
