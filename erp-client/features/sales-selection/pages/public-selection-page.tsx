"use client"

import * as React from "react"
import { useStore } from "@tanstack/react-form"
import { useMutation, useQueryClient } from "@tanstack/react-query"
import { useAppForm } from "@/components/form"
import { Button } from "@/components/ui/button"
import { Input } from "@/components/ui/input"
import { Badge } from "@/components/ui/badge"
import {
    Dialog,
    DialogContent,
    DialogHeader,
    DialogTitle,
} from "@/components/ui/dialog"
import {
    AlertCircle,
    Check,
    CheckCircle2,
    Clock,
    FileCheck,
    Gift,
    Info,
    Layers,
    Megaphone,
    Minus,
    Package,
    Plus,
    RefreshCw,
    ShieldAlert,
    ShoppingBag,
    Store,
} from "lucide-react"
import { cn } from "@/lib/utils"
import { publicImageUrl, savePublicSession, submitPublicSession } from "../api"
import { usePublicSelectionQuery, salesSelectionKeys } from "../queries"
import {
    picksFromChoices,
    readPendingRequest,
    requestFailure,
    type PendingSelectionRequest,
    type Pick,
} from "../lib/public-flow"
import { quantityStringSchema } from "../lib/validation"
import type { PublicDisplayItemView, PublicPageView } from "../types"

/** 读取公开选品事实，明确区分加载、结束、回执和可编辑状态。 */
export const PublicSelectionPage = ({ token }: { token: string }) => {
    const query = usePublicSelectionQuery(token)
    const page = query.data

    if (query.isError && (!page || requestFailure(query.error) === "ended"))
        return (
            <main className="mx-auto flex min-h-screen max-w-md flex-col items-center justify-center p-6 text-center">
                <div className="mb-4 flex h-14 w-14 items-center justify-center rounded-2xl bg-slate-100 text-slate-400">
                    <Clock className="h-7 w-7" />
                </div>
                <h1 className="text-xl font-bold text-slate-900">
                    {requestFailure(query.error) === "ended"
                        ? "选品链接已失效"
                        : "暂时无法打开选品页"}
                </h1>
                <p className="my-3 text-sm leading-relaxed text-slate-500">
                    {requestFailure(query.error) === "ended"
                        ? "请联系销售核对当前链接。"
                        : "请检查网络连接后重试。"}
                </p>
                {requestFailure(query.error) !== "ended" && (
                    <Button
                        id="sales-selection-public-retry"
                        onClick={() => void query.refetch()}
                        className="mt-2"
                    >
                        重新打开
                    </Button>
                )}
            </main>
        )

    if (!page)
        return (
            <main className="mx-auto flex min-h-screen max-w-md flex-col items-center justify-center p-6 text-center text-sm text-slate-500">
                <RefreshCw className="mb-3 h-6 w-6 animate-spin text-rose-500" />
                正在打开选品页…
            </main>
        )

    if (page.kind === "ENDED") return <Ended />
    if (page.kind === "RECEIPT" && page.receipt) return <Receipt page={page} />

    return (
        <SelectionForm
            key={token}
            token={token}
            page={page}
            refresh={async () => (await query.refetch()).data}
        />
    )
}

/** 结束后移除所有可写表单。 */
const Ended = () => {
    return (
        <main className="mx-auto flex min-h-screen max-w-md flex-col items-center justify-center p-6 text-center bg-slate-50">
            <div className="mb-4 flex h-16 w-16 items-center justify-center rounded-3xl bg-slate-200/70 text-slate-400">
                <Clock className="h-8 w-8" />
            </div>
            <h1 className="text-xl font-bold text-slate-900">选品已结束</h1>
            <p className="mt-2 text-sm text-slate-500">
                链接已失效，请联系销售核对。
            </p>
            <p className="mt-4 text-xs text-slate-400">
                如需重新选品或确认报价，请联系您的专属销售经理获取新链接。
            </p>
        </main>
    )
}

/** 显示服务端已提交的客户明细和金额。 */
const Receipt = ({ page }: { page: PublicPageView }) => {
    const receipt = page.receipt!
    return (
        <main className="mx-auto min-h-screen max-w-lg bg-slate-100 p-4 pb-12">
            <div className="mb-4 rounded-3xl bg-white p-6 text-center shadow-xs border border-slate-200/80">
                <div className="mx-auto mb-3 flex h-14 w-14 items-center justify-center rounded-full bg-emerald-50 text-emerald-600">
                    <CheckCircle2 className="h-8 w-8" />
                </div>
                <h1 className="text-xl font-bold text-slate-900">已提交选品</h1>
                <p className="mt-1 text-sm font-semibold text-slate-800">
                    {receipt.customer_name}
                </p>
                <div className="mt-3 inline-flex flex-col items-center gap-1 rounded-xl bg-slate-50 px-4 py-2 text-xs text-slate-600 border border-slate-200">
                    <span className="font-mono font-medium">
                        方案编号 {receipt.proposal_no}
                    </span>
                    <span className="text-slate-400">
                        提交时间：{formatTime(receipt.submitted_at)}
                    </span>
                </div>
            </div>

            <div className="space-y-3">
                <div className="flex items-center gap-2 px-1">
                    <FileCheck className="h-4 w-4 text-rose-600" />
                    <h2 className="text-sm font-semibold text-slate-900">
                        确认选品清单
                    </h2>
                </div>
                <ChoiceSummary page={page} receipt />
                {page.notices.map((notice) => (
                    <div
                        key={notice}
                        className="rounded-xl bg-white p-3 text-xs text-slate-500 border border-slate-200/60 leading-relaxed"
                    >
                        {notice}
                    </div>
                ))}
            </div>
        </main>
    )
}

/** 使用快照名称和后端金额核对所选内容。 */
const ChoiceSummary = ({
    page,
    receipt = false,
}: {
    page: PublicPageView
    receipt?: boolean
}) => {
    const choices = receipt ? page.receipt!.items : page.choices
    const total = receipt ? page.receipt!.total_amount : page.total_amount
    return (
        <div className="space-y-3 rounded-2xl border border-slate-200/80 bg-white p-4 shadow-xs">
            {choices.map((choice) => {
                const item = page.items.find(
                    (item) => item.item_id === choice.item_id,
                )
                return (
                    <div
                        key={choice.item_id}
                        className="border-b border-slate-100 pb-3 last:border-b-0 last:pb-0"
                    >
                        <div className="flex items-baseline justify-between gap-2">
                            <p className="font-medium text-slate-900 text-sm leading-snug">
                                {item?.name ??
                                    "商品资料暂不可用，请联系销售核对"}
                                {choice.quantity != null
                                    ? ` × ${choice.quantity} 份`
                                    : ""}
                            </p>
                            {choice.line_amount != null && (
                                <p className="font-semibold text-rose-600 text-sm shrink-0">
                                    ¥ {choice.line_amount}
                                </p>
                            )}
                        </div>
                        {item?.specification &&
                            item.specification.length > 0 && (
                                <div className="mt-1 flex flex-wrap gap-1">
                                    {item.specification.map((spec) => (
                                        <span
                                            key={spec.name}
                                            className="rounded-md bg-slate-100 px-1.5 py-0.5 text-[11px] text-slate-500"
                                        >
                                            {spec.name}：{spec.value}
                                        </span>
                                    ))}
                                </div>
                            )}
                        {item?.members && item.members.length > 0 && (
                            <div className="mt-1.5 space-y-0.5 pl-2 border-l-2 border-slate-200 text-xs text-slate-500">
                                {item.members.map((member, index) => (
                                    <p
                                        key={`${member.name}-${index}`}
                                        className="text-[11px]"
                                    >
                                        {member.name} ·{" "}
                                        {member.specification
                                            .map((s) => `${s.name}：${s.value}`)
                                            .join(" / ")}{" "}
                                        · 1 {member.unit}
                                    </p>
                                ))}
                            </div>
                        )}
                    </div>
                )
            })}
            {!choices.length && (
                <p className="py-2 text-center text-xs text-slate-400">
                    尚未选择商品
                </p>
            )}
            {page.submit_mode === "BY_QUANTITY" && total != null && (
                <div className="flex items-center justify-between pt-2 border-t border-slate-100">
                    <span className="text-xs font-medium text-slate-500">
                        方案合计金额（含税）
                    </span>
                    <p className="font-bold text-base text-rose-600">
                        合计 ¥ {total}
                    </p>
                </div>
            )}
        </div>
    )
}

/** 本地编辑撤销确认，冲突保留选择，未知结果保留原请求。 */
const SelectionForm = ({
    token,
    page,
    refresh,
}: {
    token: string
    page: PublicPageView
    refresh: () => Promise<PublicPageView | undefined>
}) => {
    const client = useQueryClient()
    const recoveryKey = `sales-selection-pending:${token}`
    const [request, setRequest] =
        React.useState<PendingSelectionRequest | null>(() =>
            readPendingRequest(recoveryKey),
        )
    const [defaults] = React.useState(() => ({
        picks: picksFromChoices(
            request?.kind === "save" ? request.input.choices : page.choices,
        ),
    }))
    const form = useAppForm({ defaultValues: defaults })
    const picks = useStore(form.store, (state) => state.values.picks)
    const [version, setVersion] = React.useState(page.session_version ?? 1)
    const [confirmed, setConfirmed] = React.useState<PublicPageView | null>(
        null,
    )
    const [latest, setLatest] = React.useState<PublicPageView | null>(null)
    const [conflict, setConflict] = React.useState(false)
    const [ended, setEnded] = React.useState(false)
    const [dirty, setDirty] = React.useState(false)
    const [message, setMessage] = React.useState("")
    const mall = page.submit_mode === "MALL_REDEEM"

    // 电商交互状态：当前选中的详情商品、购物车抽屉、排序与已选过滤
    const [detailItem, setDetailItem] =
        React.useState<PublicDisplayItemView | null>(null)
    const [cartDrawerOpen, setCartDrawerOpen] = React.useState(false)
    const [showOnlySelected, setShowOnlySelected] = React.useState(false)
    const [priceSort, setPriceSort] = React.useState<"NONE" | "ASC" | "DESC">(
        "NONE",
    )

    // 收集全部档位供快捷导航（套餐形态）
    const tiers = React.useMemo(() => {
        const set = new Set<string>()
        for (const item of page.items) {
            if (item.tier_name) set.add(item.tier_name)
        }
        return Array.from(set)
    }, [page.items])
    const [activeTier, setActiveTier] = React.useState<string>("ALL")

    // 商品过滤与排序逻辑
    const displayedItems = React.useMemo(() => {
        let list = page.items
        if (activeTier !== "ALL") {
            list = list.filter((item) => item.tier_name === activeTier)
        }
        if (showOnlySelected) {
            list = list.filter((item) => picks[item.item_id]?.selected)
        }
        if (priceSort !== "NONE") {
            list = [...list].sort((a, b) => {
                const aVal = Number.parseInt(a.price.replace(".", ""), 10) || 0
                const bVal = Number.parseInt(b.price.replace(".", ""), 10) || 0
                return priceSort === "ASC" ? aVal - bVal : bVal - aVal
            })
        }
        return list
    }, [page.items, activeTier, showOnlySelected, priceSort, picks])

    const keepRequest = (next: PendingSelectionRequest | null) => {
        setRequest(next)
        try {
            if (next) sessionStorage.setItem(recoveryKey, JSON.stringify(next))
            else sessionStorage.removeItem(recoveryKey)
        } catch {
            /* 当前页面仍保留请求 */
        }
    }

    const mutation = useMutation({
        meta: { suppressErrorToast: true },
        mutationFn: (operation: PendingSelectionRequest) =>
            operation.kind === "save"
                ? savePublicSession(token, operation.input)
                : submitPublicSession(token, operation.input),
        onSuccess: (saved, operation) => {
            client.setQueryData(salesSelectionKeys.public(token), saved)
            keepRequest(null)
            setMessage(operation.kind === "save" ? "选择已保存" : "选品已提交")
            setVersion(saved.session_version ?? version)
            setDirty(false)
            form.setFieldValue("picks", picksFromChoices(saved.choices))
            setConfirmed(
                operation.kind === "save" &&
                    operation.confirm &&
                    saved.kind === "SELECTING"
                    ? saved
                    : null,
            )
        },
        onError: async (error) => {
            setConfirmed(null)
            const kind = requestFailure(error)
            if (kind === "ended") {
                keepRequest(null)
                setEnded(true)
                return
            }
            if (kind === "unknown") {
                setMessage(
                    "操作结果尚未确认。已保留本次请求，请核对结果后继续。",
                )
                return
            }
            keepRequest(null)
            if (kind === "conflict") {
                setConflict(true)
                setMessage(
                    "其他页面已修改选择。你的本地选择已保留，请核对最新清单后继续。",
                )
                setLatest((await refresh()) ?? null)
            } else
                setMessage(
                    error instanceof Error
                        ? error.message
                        : "保存未成功，请检查份数后重试。",
                )
        },
    })

    const locked = mutation.isPending || request !== null

    const change = (id: string, patch: Partial<Pick>) => {
        if (locked || conflict) return
        form.setFieldValue("picks", (current) => ({
            ...current,
            [id]: {
                ...(current[id] ?? { selected: false, quantity: "1" }),
                ...patch,
            },
        }))
        setConfirmed(null)
        setDirty(true)
        setMessage("")
    }

    const persist = (confirm: boolean) => {
        if (locked || conflict) return
        const choices: { item_id: string; quantity?: number }[] = []
        for (const item of page.items) {
            const pick = picks[item.item_id]
            if (!pick?.selected) continue
            if (
                !mall &&
                !quantityStringSchema.safeParse(pick.quantity).success
            ) {
                setMessage("每项份数必须为 1 到 100000 的整数。")
                return
            }
            choices.push({
                item_id: item.item_id,
                ...(mall
                    ? {}
                    : { quantity: Number.parseInt(pick.quantity, 10) }),
            })
        }
        if (confirm && !choices.length) {
            setMessage("请至少选择一项商品。")
            return
        }
        const next: PendingSelectionRequest = {
            kind: "save",
            confirm,
            input: {
                idempotencyKey: crypto.randomUUID(),
                expectedSessionVersion: version,
                choices,
            },
        }
        keepRequest(next)
        mutation.mutate(next)
    }

    const submit = () => {
        if (!confirmed || locked || conflict || dirty) return
        const next: PendingSelectionRequest = {
            kind: "submit",
            input: {
                idempotencyKey: crypto.randomUUID(),
                expectedSessionVersion: confirmed.session_version!,
            },
        }
        keepRequest(next)
        mutation.mutate(next)
    }

    if (ended) return <Ended />

    const selectedCount = Object.values(picks).filter(
        (pick) => pick.selected,
    ).length

    return (
        <main className="min-h-screen bg-slate-100 pb-32">
            <div className="mx-auto max-w-lg min-h-screen bg-white shadow-xl flex flex-col">
                {/* 1. 电商商城顶部导航条 */}
                <header className="sticky top-0 z-30 bg-white/95 backdrop-blur-md border-b border-slate-200/80 px-4 py-3">
                    <div className="flex items-center justify-between gap-3">
                        <div className="flex items-center gap-2.5 min-w-0">
                            <div className="flex h-9 w-9 shrink-0 items-center justify-center rounded-xl bg-gradient-to-tr from-rose-500 via-rose-600 to-amber-500 text-white shadow-xs">
                                <Store className="h-5 w-5" />
                            </div>
                            <div className="min-w-0">
                                <h1 className="text-sm font-bold text-slate-900 truncate">
                                    {page.customer_name}
                                </h1>
                                <p className="text-[11px] text-slate-500 flex items-center gap-1.5 mt-0.5">
                                    <span className="inline-block h-1.5 w-1.5 rounded-full bg-emerald-500 animate-pulse" />
                                    <span>专属选品会场</span>
                                    <span className="text-slate-300">|</span>
                                    <span>
                                        {mall ? "商城兑换" : "按份采购"}
                                    </span>
                                </p>
                            </div>
                        </div>
                        <Badge
                            variant="outline"
                            className="shrink-0 text-xs border-rose-200 bg-rose-50 text-rose-600 font-medium px-2.5 py-0.5"
                        >
                            {mall ? "意向可选库" : "批量采购"}
                        </Badge>
                    </div>
                </header>

                {/* 2. 电商通知走马灯 / 提示条 */}
                {page.notices.length > 0 && (
                    <div className="bg-amber-50/90 border-b border-amber-200/60 px-4 py-2 text-xs text-amber-900 flex items-center gap-2">
                        <Megaphone className="h-3.5 w-3.5 shrink-0 text-amber-600" />
                        <div className="truncate flex-1 space-x-2 font-medium">
                            {page.notices.map((notice, i) => (
                                <span key={notice}>
                                    {i > 0 ? " · " : ""}
                                    {notice}
                                </span>
                            ))}
                        </div>
                    </div>
                )}

                {/* 状态与未知结果恢复提示 */}
                {(message || request) && (
                    <div
                        role="status"
                        className="mx-3 mt-3 rounded-2xl border border-amber-200 bg-amber-50 p-3.5 text-xs text-amber-900 shadow-2xs"
                    >
                        <div className="flex items-start gap-2">
                            <AlertCircle className="h-4 w-4 shrink-0 text-amber-600 mt-0.5" />
                            <div className="flex-1">
                                <p className="font-semibold leading-relaxed">
                                    {message ||
                                        "上次操作结果待核对，请先恢复本次请求。"}
                                </p>
                                {request && !mutation.isPending && (
                                    <Button
                                        id="sales-selection-public-reconcile"
                                        size="sm"
                                        className="mt-2 h-7 rounded-lg bg-amber-600 text-white hover:bg-amber-700 text-xs"
                                        onClick={() => mutation.mutate(request)}
                                    >
                                        核对并恢复本次操作
                                    </Button>
                                )}
                            </div>
                        </div>
                    </div>
                )}

                {/* 冲突处理区域 */}
                {conflict && (
                    <section
                        className="mx-3 mt-3 rounded-2xl border-2 border-amber-300 bg-amber-50/80 p-3.5 space-y-2.5"
                        aria-label="最新保存的选择"
                    >
                        <div className="flex items-center gap-2 text-amber-900">
                            <ShieldAlert className="h-4 w-4 text-amber-600" />
                            <h2 className="font-bold text-xs sm:text-sm">
                                其他页面最新保存的选择
                            </h2>
                        </div>
                        <p className="text-xs text-amber-800 leading-relaxed">
                            其他设备刚刚更新了该选品册。您的本地修改已保留，请仔细核对最新清单：
                        </p>
                        {latest ? (
                            <ChoiceSummary page={latest} />
                        ) : (
                            <p className="text-xs text-slate-500">
                                最新清单暂时无法读取，请重试。
                            </p>
                        )}
                        <div className="flex flex-wrap gap-2 pt-1">
                            <Button
                                id="sales-selection-public-reload-conflict"
                                variant="outline"
                                size="sm"
                                className="h-7 text-xs rounded-lg"
                                onClick={async () =>
                                    setLatest((await refresh()) ?? null)
                                }
                            >
                                重新读取最新清单
                            </Button>
                            <Button
                                id="sales-selection-public-acknowledge"
                                size="sm"
                                className="h-7 text-xs rounded-lg bg-amber-600 text-white hover:bg-amber-700"
                                disabled={
                                    !latest || latest.kind !== "SELECTING"
                                }
                                onClick={() => {
                                    setVersion(latest!.session_version!)
                                    setConflict(false)
                                    setLatest(null)
                                    setDirty(true)
                                    setMessage(
                                        "已保留你的本地选择。请完成编辑后重新保存核对；保存将替换上面的最新清单。",
                                    )
                                }}
                            >
                                已核对，保留本地选择继续编辑
                            </Button>
                        </div>
                    </section>
                )}

                {/* 3. 分类、档位与快捷筛选横滑栏 */}
                <div className="sticky top-[57px] z-20 bg-white border-b border-slate-200/80 px-3 py-2 flex items-center justify-between gap-2 shadow-2xs">
                    <div className="flex items-center gap-1.5 overflow-x-auto no-scrollbar flex-1">
                        <button
                            type="button"
                            onClick={() => {
                                setActiveTier("ALL")
                                setShowOnlySelected(false)
                            }}
                            className={cn(
                                "shrink-0 rounded-full px-3 py-1 text-xs font-semibold transition-all",
                                activeTier === "ALL" && !showOnlySelected
                                    ? "bg-rose-600 text-white shadow-xs"
                                    : "bg-slate-100 text-slate-600 hover:bg-slate-200",
                            )}
                        >
                            全部 ({page.items.length})
                        </button>
                        {tiers.map((tier) => (
                            <button
                                key={tier}
                                type="button"
                                onClick={() => {
                                    setActiveTier(tier)
                                    setShowOnlySelected(false)
                                }}
                                className={cn(
                                    "shrink-0 rounded-full px-3 py-1 text-xs font-semibold transition-all",
                                    activeTier === tier && !showOnlySelected
                                        ? "bg-rose-600 text-white shadow-xs"
                                        : "bg-slate-100 text-slate-600 hover:bg-slate-200",
                                )}
                            >
                                {tier}
                            </button>
                        ))}
                        <button
                            type="button"
                            onClick={() =>
                                setShowOnlySelected(!showOnlySelected)
                            }
                            className={cn(
                                "shrink-0 rounded-full px-3 py-1 text-xs font-semibold transition-all flex items-center gap-1",
                                showOnlySelected
                                    ? "bg-rose-600 text-white shadow-xs"
                                    : "bg-slate-100 text-slate-600 hover:bg-slate-200",
                            )}
                        >
                            <Check className="h-3 w-3" />
                            已选 ({selectedCount})
                        </button>
                    </div>
                    {/* 价格排序微调 */}
                    <button
                        type="button"
                        onClick={() =>
                            setPriceSort((prev) =>
                                prev === "NONE"
                                    ? "ASC"
                                    : prev === "ASC"
                                      ? "DESC"
                                      : "NONE",
                            )
                        }
                        className={cn(
                            "shrink-0 flex items-center gap-0.5 rounded-full px-2.5 py-1 text-xs font-medium border border-slate-200 bg-white transition-all",
                            priceSort !== "NONE"
                                ? "text-rose-600 border-rose-300 font-bold bg-rose-50/50"
                                : "text-slate-600",
                        )}
                    >
                        价格
                        {priceSort === "ASC"
                            ? " ↑"
                            : priceSort === "DESC"
                              ? " ↓"
                              : " ↕"}
                    </button>
                </div>

                {/* 4. 标准电商双列瀑布流 / 宫格商品列表 */}
                <div className="p-3 sm:p-4 flex-1">
                    <fieldset
                        disabled={locked || conflict}
                        className="grid grid-cols-2 gap-2.5 sm:gap-3"
                    >
                        {displayedItems.map((item) => {
                            const pick = picks[item.item_id]
                            const isSelected = pick?.selected ?? false
                            const image = publicImageUrl(token, item.cover_path)
                            const [intPart, decPart] = item.price.split(".")

                            return (
                                <article
                                    key={item.item_id}
                                    className={cn(
                                        "group relative flex flex-col justify-between overflow-hidden rounded-2xl bg-white border transition-all duration-200 shadow-2xs hover:shadow-md",
                                        isSelected
                                            ? "border-rose-500 ring-2 ring-rose-500/15"
                                            : "border-slate-200/80 hover:border-slate-300",
                                    )}
                                >
                                    {/* 1:1 正方形电商大图 */}
                                    <button
                                        type="button"
                                        aria-label={`查看${item.name}详情`}
                                        className="relative aspect-square w-full overflow-hidden bg-slate-50 text-left block cursor-pointer border-0 p-0"
                                        onClick={() => setDetailItem(item)}
                                    >
                                        {image ? (
                                            // eslint-disable-next-line @next/next/no-img-element
                                            <img
                                                src={image}
                                                alt=""
                                                className="h-full w-full object-cover transition-transform duration-300 group-hover:scale-105"
                                                loading="lazy"
                                                referrerPolicy="no-referrer"
                                            />
                                        ) : (
                                            <div className="flex h-full w-full flex-col items-center justify-center bg-gradient-to-br from-rose-50/40 via-slate-50 to-amber-50/30 p-3 text-center">
                                                <div className="flex h-11 w-11 items-center justify-center rounded-2xl bg-white shadow-2xs text-rose-400">
                                                    <Gift className="h-6 w-6 stroke-[1.5]" />
                                                </div>
                                                <span className="mt-2 text-[10px] font-semibold text-slate-400 tracking-wider">
                                                    严选好物
                                                </span>
                                            </div>
                                        )}

                                        {/* 套餐档位徽标 */}
                                        {item.tier_name && (
                                            <span className="absolute left-2 top-2 rounded-md bg-slate-900/75 px-1.5 py-0.5 text-[10px] font-semibold text-white shadow-xs backdrop-blur-xs">
                                                {item.tier_name}
                                            </span>
                                        )}

                                        {/* 套餐件数提示 */}
                                        {item.members.length > 0 && (
                                            <span className="absolute left-2 bottom-2 rounded-md bg-black/60 px-1.5 py-0.5 text-[10px] text-white flex items-center gap-1 backdrop-blur-xs">
                                                <Layers className="h-2.5 w-2.5" />
                                                {item.members.length}件装
                                            </span>
                                        )}

                                        {/* 右上角选中状态徽标 */}
                                        <div className="absolute right-2 top-2">
                                            <span
                                                className={cn(
                                                    "flex h-6 w-6 items-center justify-center rounded-full transition-all shadow-xs",
                                                    isSelected
                                                        ? "bg-rose-600 text-white scale-100 ring-2 ring-white"
                                                        : "bg-black/20 text-transparent border border-white/60 backdrop-blur-xs scale-90",
                                                )}
                                            >
                                                <Check className="h-3.5 w-3.5 stroke-[3]" />
                                            </span>
                                        </div>
                                    </button>

                                    {/* 卡片详情与操作 */}
                                    <div className="p-2.5 sm:p-3 flex flex-col flex-1 justify-between gap-1.5">
                                        <div>
                                            <button
                                                type="button"
                                                onClick={() =>
                                                    setDetailItem(item)
                                                }
                                                className="text-left w-full text-xs sm:text-sm font-medium text-slate-900 leading-snug line-clamp-2 h-[2.5em] hover:text-rose-600 transition-colors p-0 border-0 bg-transparent"
                                            >
                                                {item.name}
                                            </button>

                                            {/* 规格标签 */}
                                            {item.specification.length > 0 && (
                                                <div className="mt-1 flex flex-wrap gap-1">
                                                    {item.specification
                                                        .slice(0, 2)
                                                        .map((s) => (
                                                            <span
                                                                key={s.name}
                                                                className="rounded bg-slate-100 px-1 py-0.5 text-[10px] text-slate-500 truncate max-w-full"
                                                            >
                                                                {s.value ||
                                                                    s.name}
                                                            </span>
                                                        ))}
                                                </div>
                                            )}
                                        </div>

                                        {/* 价格与操作区域 */}
                                        <div className="mt-1 pt-1.5 border-t border-slate-100">
                                            <div className="flex items-baseline text-rose-600 font-semibold">
                                                <span className="text-xs mr-0.5 font-bold">
                                                    ¥
                                                </span>
                                                <span className="text-base sm:text-lg font-bold tracking-tight">
                                                    {intPart}
                                                </span>
                                                {decPart !== undefined && (
                                                    <span className="text-[11px] font-medium">
                                                        .{decPart}
                                                    </span>
                                                )}
                                                {mall && (
                                                    <span className="ml-1 text-[10px] font-normal text-slate-400">
                                                        参考值
                                                    </span>
                                                )}
                                            </div>

                                            {/* 按钮行 */}
                                            <div className="mt-1.5 flex items-center justify-between gap-1">
                                                <label
                                                    aria-label={item.name}
                                                    htmlFor={`sales-selection-public-select-${item.item_id}`}
                                                    className={cn(
                                                        "flex items-center justify-center gap-1 rounded-full px-2.5 py-1 text-xs font-semibold transition-all cursor-pointer select-none flex-1 shadow-2xs active:scale-95",
                                                        isSelected
                                                            ? "bg-rose-600 text-white"
                                                            : "bg-rose-50 text-rose-600 border border-rose-200 hover:bg-rose-600 hover:text-white",
                                                    )}
                                                >
                                                    <input
                                                        id={`sales-selection-public-select-${item.item_id}`}
                                                        aria-label={item.name}
                                                        type="checkbox"
                                                        checked={isSelected}
                                                        onChange={(e) =>
                                                            change(
                                                                item.item_id,
                                                                {
                                                                    selected:
                                                                        e.target
                                                                            .checked,
                                                                },
                                                            )
                                                        }
                                                        className="sr-only"
                                                    />
                                                    {isSelected ? (
                                                        <>
                                                            <Check className="h-3 w-3 stroke-[3]" />
                                                            <span>已选</span>
                                                        </>
                                                    ) : (
                                                        <span>+ 选品</span>
                                                    )}
                                                </label>

                                                {/* 详情浮层快捷查看 */}
                                                <button
                                                    type="button"
                                                    onClick={() =>
                                                        setDetailItem(item)
                                                    }
                                                    className="flex h-6.5 w-6.5 items-center justify-center rounded-full text-slate-400 hover:bg-slate-100 hover:text-slate-700 transition-colors"
                                                    title="查看商品详情"
                                                >
                                                    <Info className="h-3.5 w-3.5" />
                                                </button>
                                            </div>

                                            {/* 按份步进器 */}
                                            {isSelected && !mall && (
                                                <div className="mt-2 flex items-center justify-between rounded-lg bg-slate-50 p-1 border border-slate-200">
                                                    <span className="text-[10px] text-slate-500 pl-1 font-medium">
                                                        份数
                                                    </span>
                                                    <div className="flex items-center gap-1">
                                                        <button
                                                            type="button"
                                                            className="flex h-5 w-5 items-center justify-center rounded bg-white text-slate-700 shadow-2xs disabled:opacity-40"
                                                            disabled={
                                                                locked ||
                                                                conflict ||
                                                                Number.parseInt(
                                                                    pick.quantity,
                                                                    10,
                                                                ) <= 1
                                                            }
                                                            onClick={(e) => {
                                                                e.preventDefault()
                                                                e.stopPropagation()
                                                                const current =
                                                                    Number.parseInt(
                                                                        pick.quantity,
                                                                        10,
                                                                    )
                                                                if (
                                                                    Number.isSafeInteger(
                                                                        current,
                                                                    ) &&
                                                                    current > 1
                                                                ) {
                                                                    change(
                                                                        item.item_id,
                                                                        {
                                                                            quantity:
                                                                                String(
                                                                                    current -
                                                                                        1,
                                                                                ),
                                                                        },
                                                                    )
                                                                }
                                                            }}
                                                        >
                                                            <Minus className="h-2.5 w-2.5" />
                                                        </button>
                                                        <Input
                                                            id={`sales-selection-public-qty-${item.item_id}`}
                                                            inputMode="numeric"
                                                            className="h-5 w-8 border-0 bg-transparent text-center text-xs font-bold p-0 shadow-none focus-visible:ring-0"
                                                            value={
                                                                pick.quantity
                                                            }
                                                            onChange={(e) =>
                                                                change(
                                                                    item.item_id,
                                                                    {
                                                                        quantity:
                                                                            e
                                                                                .target
                                                                                .value,
                                                                    },
                                                                )
                                                            }
                                                            onClick={(e) =>
                                                                e.stopPropagation()
                                                            }
                                                        />
                                                        <button
                                                            type="button"
                                                            className="flex h-5 w-5 items-center justify-center rounded bg-white text-slate-700 shadow-2xs"
                                                            disabled={
                                                                locked ||
                                                                conflict
                                                            }
                                                            onClick={(e) => {
                                                                e.preventDefault()
                                                                e.stopPropagation()
                                                                const current =
                                                                    Number.parseInt(
                                                                        pick.quantity,
                                                                        10,
                                                                    )
                                                                const val =
                                                                    Number.isSafeInteger(
                                                                        current,
                                                                    )
                                                                        ? current
                                                                        : 1
                                                                change(
                                                                    item.item_id,
                                                                    {
                                                                        quantity:
                                                                            String(
                                                                                val +
                                                                                    1,
                                                                            ),
                                                                    },
                                                                )
                                                            }}
                                                        >
                                                            <Plus className="h-2.5 w-2.5" />
                                                        </button>
                                                    </div>
                                                </div>
                                            )}
                                        </div>
                                    </div>
                                </article>
                            )
                        })}
                    </fieldset>
                </div>

                {/* 5. 正式待确认清单（点击核对并提交后展开） */}
                {confirmed && (
                    <section
                        className="mx-3 mb-6 rounded-3xl border-2 border-rose-500/20 bg-rose-50/20 p-4 space-y-3 shadow-sm"
                        aria-label="核对并提交"
                    >
                        <div className="flex items-center gap-2 text-rose-600">
                            <CheckCircle2 className="h-5 w-5" />
                            <h2 className="font-bold text-sm sm:text-base text-slate-900">
                                请核对本次提交清单
                            </h2>
                        </div>
                        <p className="text-xs text-slate-500 leading-relaxed">
                            确认提交后将生成唯一的正式销售方案编号，并锁定会话。请仔细核对以下所选项：
                        </p>
                        <ChoiceSummary page={confirmed} />
                    </section>
                )}
            </div>

            {/* 6. 商品详情 Dialog */}
            <Dialog
                open={detailItem !== null}
                onOpenChange={(open) => {
                    if (!open) setDetailItem(null)
                }}
            >
                <DialogContent className="max-w-lg rounded-3xl p-5 max-h-[85vh] overflow-y-auto space-y-4">
                    {detailItem && (
                        <>
                            <DialogHeader>
                                <DialogTitle className="text-base font-bold text-slate-900 line-clamp-1">
                                    {detailItem.name}
                                </DialogTitle>
                            </DialogHeader>

                            {/* 大图预览 */}
                            <div className="relative aspect-video w-full rounded-2xl overflow-hidden bg-slate-50">
                                {publicImageUrl(token, detailItem.cover_path) ? (
                                    // eslint-disable-next-line @next/next/no-img-element
                                    <img
                                        src={publicImageUrl(
                                            token,
                                            detailItem.cover_path,
                                        )}
                                        alt=""
                                        className="h-full w-full object-cover"
                                    />
                                ) : (
                                    <div className="flex h-full w-full flex-col items-center justify-center text-slate-400 bg-slate-100">
                                        <Package className="h-10 w-10 stroke-[1.5]" />
                                        <span className="mt-2 text-xs font-medium text-slate-500">
                                            精选商品快照
                                        </span>
                                    </div>
                                )}
                            </div>

                            {/* 价格与信息 */}
                            <div className="flex items-baseline justify-between">
                                <div className="flex items-baseline text-rose-600 font-bold">
                                    <span className="text-xs mr-0.5">¥</span>
                                    <span className="text-2xl">
                                        {detailItem.price}
                                    </span>
                                    {mall && (
                                        <span className="ml-2 text-xs text-slate-400 font-normal">
                                            商城兑换参考价值
                                        </span>
                                    )}
                                </div>
                                {detailItem.tier_name && (
                                    <Badge className="bg-slate-900 text-white">
                                        {detailItem.tier_name}
                                    </Badge>
                                )}
                            </div>

                            {/* 详细规格 */}
                            {detailItem.specification.length > 0 && (
                                <div className="rounded-2xl bg-slate-50 p-3 text-xs space-y-1.5 border border-slate-150">
                                    <p className="font-semibold text-slate-700">
                                        规格参数
                                    </p>
                                    <div className="grid grid-cols-2 gap-2 text-slate-600">
                                        {detailItem.specification.map(
                                            (spec) => (
                                                <p key={spec.name}>
                                                    <span className="text-slate-400">
                                                        {spec.name}：
                                                    </span>
                                                    {spec.value}
                                                </p>
                                            ),
                                        )}
                                    </div>
                                </div>
                            )}

                            {/* 套餐明细清单 */}
                            {detailItem.members.length > 0 && (
                                <div className="rounded-2xl bg-rose-50/40 p-3 text-xs space-y-2 border border-rose-100">
                                    <p className="font-bold text-rose-950 flex items-center gap-1.5">
                                        <Layers className="h-4 w-4 text-rose-600" />
                                        套餐包含 {detailItem.members.length}{" "}
                                        款组合商品
                                    </p>
                                    <div className="space-y-1.5">
                                        {detailItem.members.map((m, idx) => (
                                            <div
                                                key={idx}
                                                className="flex items-center justify-between text-slate-700 border-b border-rose-100/60 pb-1 last:border-0 last:pb-0"
                                            >
                                                <span className="font-medium">
                                                    {m.name}
                                                </span>
                                                <span className="text-slate-400">
                                                    {m.specification
                                                        .map((s) => s.value)
                                                        .join("/")}{" "}
                                                    · 1{m.unit}
                                                </span>
                                            </div>
                                        ))}
                                    </div>
                                </div>
                            )}

                            <div className="pt-2">
                                <Button
                                    className="w-full rounded-2xl bg-rose-600 hover:bg-rose-700 text-white font-bold h-11"
                                    onClick={() => {
                                        change(detailItem.item_id, {
                                            selected:
                                                !picks[detailItem.item_id]
                                                    ?.selected,
                                        })
                                        setDetailItem(null)
                                    }}
                                >
                                    {picks[detailItem.item_id]?.selected
                                        ? "从选品中取消"
                                        : "加入选品"}
                                </Button>
                            </div>
                        </>
                    )}
                </DialogContent>
            </Dialog>

            {/* 7. 已选清单 Dialog (Cart Sheet) */}
            <Dialog open={cartDrawerOpen} onOpenChange={setCartDrawerOpen}>
                <DialogContent className="max-w-lg rounded-3xl p-5 max-h-[75vh] flex flex-col">
                    <DialogHeader>
                        <DialogTitle className="text-base font-bold text-slate-900 flex items-center gap-2">
                            <ShoppingBag className="h-5 w-5 text-rose-600" />
                            已选商品清单 ({selectedCount} 款)
                        </DialogTitle>
                    </DialogHeader>

                    <div className="flex-1 overflow-y-auto py-2 space-y-3">
                        {page.items
                            .filter((item) => picks[item.item_id]?.selected)
                            .map((item) => {
                                const pick = picks[item.item_id]
                                return (
                                    <div
                                        key={item.item_id}
                                        className="flex items-center justify-between gap-3 border-b border-slate-100 pb-2.5"
                                    >
                                        <div className="min-w-0 flex-1">
                                            <p className="font-medium text-xs sm:text-sm text-slate-900 truncate">
                                                {item.name}
                                            </p>
                                            <p className="text-xs font-bold text-rose-600 mt-0.5">
                                                ¥ {item.price}
                                            </p>
                                        </div>
                                        <div className="flex items-center gap-2">
                                            {!mall && (
                                                <span className="text-xs font-semibold text-slate-700 bg-slate-100 px-2 py-0.5 rounded-md">
                                                    {pick?.quantity} 份
                                                </span>
                                            )}
                                            <Button
                                                variant="ghost"
                                                size="sm"
                                                className="h-7 text-xs text-rose-600 hover:bg-rose-50 px-2"
                                                onClick={() =>
                                                    change(item.item_id, {
                                                        selected: false,
                                                    })
                                                }
                                            >
                                                移除
                                            </Button>
                                        </div>
                                    </div>
                                )
                            })}
                        {selectedCount === 0 && (
                            <p className="text-center py-8 text-xs text-slate-400">
                                尚未选择任何商品
                            </p>
                        )}
                    </div>

                    <div className="pt-2 border-t border-slate-100">
                        <Button
                            className="w-full rounded-full bg-slate-900 text-white"
                            onClick={() => setCartDrawerOpen(false)}
                        >
                            继续选品
                        </Button>
                    </div>
                </DialogContent>
            </Dialog>

            {/* 8. 核心吸底结算栏 (Sticky Bottom Action Bar) */}
            <aside
                aria-label="核对并提交"
                className="fixed bottom-0 left-0 right-0 z-40 bg-white/95 backdrop-blur-md border-t border-slate-200/80 px-4 py-2.5 shadow-[0_-8px_20px_rgba(0,0,0,0.08)]"
            >
                <div className="mx-auto flex max-w-lg items-center justify-between gap-3">
                    {/* 左侧：点击呼出已选清单 */}
                    <button
                        type="button"
                        className="flex items-center gap-2.5 text-left cursor-pointer select-none active:opacity-80 transition-opacity border-0 bg-transparent p-0"
                        onClick={() => setCartDrawerOpen(true)}
                    >
                        <div className="relative flex h-11 w-11 items-center justify-center rounded-2xl bg-gradient-to-tr from-rose-500 to-rose-600 text-white shadow-md active:scale-95 transition-transform">
                            <ShoppingBag className="h-5 w-5" />
                            {selectedCount > 0 && (
                                <span className="absolute -right-1.5 -top-1.5 flex h-5 min-w-5 items-center justify-center rounded-full bg-amber-400 px-1 text-[11px] font-bold text-slate-950 shadow-sm animate-in zoom-in">
                                    {selectedCount}
                                </span>
                            )}
                        </div>
                        <div>
                            {mall ? (
                                <div>
                                    <p className="text-sm font-bold text-slate-900">
                                        已选{" "}
                                        <span className="text-rose-600">
                                            {selectedCount}
                                        </span>{" "}
                                        款
                                    </p>
                                    <p className="text-[11px] text-slate-400">
                                        {dirty
                                            ? "修改待保存"
                                            : "点击查看已选清单"}
                                    </p>
                                </div>
                            ) : (
                                <div>
                                    <p className="text-base font-bold text-rose-600 leading-none">
                                        已选 {selectedCount} 项
                                    </p>
                                    <p className="text-[11px] text-slate-500 mt-0.5">
                                        {dirty
                                            ? "尚有修改未保存"
                                            : "已选内容已同步"}
                                    </p>
                                </div>
                            )}
                        </div>
                    </button>

                    {/* 右侧：电商结算大按钮组 */}
                    <div className="flex items-center gap-2">
                        <Button
                            id="sales-selection-public-save"
                            variant="outline"
                            size="sm"
                            className="rounded-full border-slate-300 text-xs text-slate-700 px-3.5 h-9"
                            disabled={locked || conflict}
                            onClick={() => persist(false)}
                        >
                            保存选择
                        </Button>
                        {confirmed ? (
                            <Button
                                id="sales-selection-public-submit"
                                size="sm"
                                className="rounded-full bg-gradient-to-r from-emerald-500 to-emerald-600 text-xs font-bold text-white shadow-md hover:opacity-95 px-5 h-9 active:scale-95 transition-all"
                                disabled={locked || conflict || dirty}
                                onClick={submit}
                            >
                                确认并提交选品
                            </Button>
                        ) : (
                            <Button
                                id="sales-selection-public-review"
                                size="sm"
                                className="rounded-full bg-gradient-to-r from-rose-500 via-rose-600 to-red-600 text-xs font-bold text-white shadow-md hover:opacity-95 px-5 h-9 active:scale-95 transition-all"
                                disabled={locked || conflict}
                                onClick={() => persist(true)}
                            >
                                核对并提交
                            </Button>
                        )}
                    </div>
                </div>
            </aside>
        </main>
    )
}

/** 将服务端秒时间戳格式化为客户本地时间。 */
const formatTime = (value: number) => {
    return new Date(value * 1000).toLocaleString("zh-CN")
}
