"use client"

import * as React from "react"
import { useStore } from "@tanstack/react-form"
import { useMutation, useQueryClient } from "@tanstack/react-query"
import { useAppForm } from "@/components/form"
import { Button } from "@/components/ui/button"
import { Input } from "@/components/ui/input"
import {
    AlertCircle,
    Check,
    CheckCircle2,
    Clock,
    FileCheck,
    Info,
    Layers,
    Minus,
    Package,
    Plus,
    RefreshCw,
    ShieldAlert,
    ShoppingBag,
    Sparkles,
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
import type { PublicPageView } from "../types"

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
                <RefreshCw className="mb-3 h-6 w-6 animate-spin text-primary" />
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
        <main className="mx-auto flex min-h-screen max-w-md flex-col items-center justify-center p-6 text-center">
            <div className="mb-4 flex h-16 w-16 items-center justify-center rounded-3xl bg-slate-100 text-slate-400 shadow-inner">
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
        <main className="mx-auto min-h-screen max-w-lg bg-slate-50/50 p-4 pb-12">
            <div className="mb-6 rounded-3xl bg-white p-6 text-center shadow-xs border border-slate-100">
                <div className="mx-auto mb-3 flex h-14 w-14 items-center justify-center rounded-2xl bg-emerald-50 text-emerald-600">
                    <CheckCircle2 className="h-8 w-8" />
                </div>
                <h1 className="text-xl font-bold text-slate-900">已提交选品</h1>
                <p className="mt-1 text-sm font-semibold text-slate-800">
                    {receipt.customer_name}
                </p>
                <div className="mt-4 inline-flex flex-col items-center gap-1 rounded-xl bg-slate-50 px-4 py-2 text-xs text-slate-600 border border-slate-150">
                    <span className="font-mono font-medium">
                        方案编号 {receipt.proposal_no}
                    </span>
                    <span className="text-slate-400">
                        提交时间：{formatTime(receipt.submitted_at)}
                    </span>
                </div>
            </div>

            <div className="space-y-4">
                <div className="flex items-center gap-2 px-1">
                    <FileCheck className="h-4 w-4 text-primary" />
                    <h2 className="text-sm font-semibold text-slate-900">
                        确认选品清单
                    </h2>
                </div>
                <ChoiceSummary page={page} receipt />
                {page.notices.map((notice) => (
                    <div
                        key={notice}
                        className="rounded-xl bg-slate-100/80 p-3 text-xs text-slate-500 leading-relaxed"
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
                                <p className="font-semibold text-slate-900 text-sm shrink-0">
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

    // 收集全部档位供快捷导航（套餐形态）
    const tiers = React.useMemo(() => {
        const set = new Set<string>()
        for (const item of page.items) {
            if (item.tier_name) set.add(item.tier_name)
        }
        return Array.from(set)
    }, [page.items])
    const [activeTier, setActiveTier] = React.useState<string>("ALL")

    const displayedItems = React.useMemo(() => {
        if (activeTier === "ALL") return page.items
        return page.items.filter((item) => item.tier_name === activeTier)
    }, [page.items, activeTier])

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
        <main className="min-h-screen bg-slate-50/60 pb-32">
            <div className="mx-auto max-w-lg px-4 pt-4">
                {/* 顶部企业尊享卡片 */}
                <header className="relative mb-5 overflow-hidden rounded-3xl bg-gradient-to-br from-slate-900 via-slate-800 to-slate-900 p-5 text-white shadow-md">
                    <div className="pointer-events-none absolute -right-6 -top-6 h-28 w-28 rounded-full bg-primary/20 blur-2xl" />
                    <div className="pointer-events-none absolute -bottom-6 -left-6 h-28 w-28 rounded-full bg-emerald-500/15 blur-2xl" />

                    <div className="relative z-10 flex flex-col gap-2">
                        <div className="flex items-center justify-between">
                            <span className="inline-flex items-center gap-1.5 rounded-full bg-white/10 px-2.5 py-0.5 text-xs font-medium text-slate-200 backdrop-blur-xs">
                                <Sparkles className="h-3 w-3 text-amber-400" />
                                尊享选品手册
                            </span>
                            <span className="inline-flex items-center rounded-full bg-emerald-500/20 px-2.5 py-0.5 text-[11px] font-medium text-emerald-300 border border-emerald-500/30">
                                {mall ? "商城兑换模式" : "按份采购模式"}
                            </span>
                        </div>

                        <div>
                            <h1 className="text-xl font-bold tracking-tight text-white sm:text-2xl">
                                {page.customer_name} 选品
                            </h1>
                            <p className="mt-1 text-xs text-slate-300">
                                {mall
                                    ? "勾选意向商品形成商城可兑范围，无需填写份数。"
                                    : "选择意向商品并确认采购份数，系统将核算方案金额。"}
                            </p>
                        </div>

                        {page.notices.length > 0 && (
                            <div className="mt-2 rounded-2xl bg-white/10 p-3 text-xs text-slate-200 backdrop-blur-xs border border-white/10 flex items-start gap-2">
                                <Info className="h-4 w-4 shrink-0 text-amber-300 mt-0.5" />
                                <div className="space-y-0.5 leading-relaxed">
                                    {page.notices.map((notice) => (
                                        <p key={notice}>{notice}</p>
                                    ))}
                                </div>
                            </div>
                        )}
                    </div>
                </header>

                {/* 状态与未知结果恢复提示 */}
                {(message || request) && (
                    <div
                        role="status"
                        className="mb-4 rounded-2xl border border-amber-200 bg-amber-50/90 p-4 text-sm text-amber-900 shadow-xs"
                    >
                        <div className="flex items-start gap-2.5">
                            <AlertCircle className="h-5 w-5 shrink-0 text-amber-600 mt-0.5" />
                            <div className="flex-1">
                                <p className="font-medium leading-relaxed">
                                    {message ||
                                        "上次操作结果待核对，请先恢复本次请求。"}
                                </p>
                                {request && !mutation.isPending && (
                                    <Button
                                        id="sales-selection-public-reconcile"
                                        size="sm"
                                        className="mt-3 bg-amber-600 text-white hover:bg-amber-700"
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
                        className="mb-5 rounded-2xl border-2 border-amber-300 bg-amber-50/70 p-4 space-y-3"
                        aria-label="最新保存的选择"
                    >
                        <div className="flex items-center gap-2 text-amber-900">
                            <ShieldAlert className="h-5 w-5 text-amber-600" />
                            <h2 className="font-bold text-sm">
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
                                onClick={async () =>
                                    setLatest((await refresh()) ?? null)
                                }
                            >
                                重新读取最新清单
                            </Button>
                            <Button
                                id="sales-selection-public-acknowledge"
                                size="sm"
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

                {/* 套餐档位快捷切换 Tabs */}
                {tiers.length > 0 && (
                    <div className="sticky top-2 z-20 mb-4 flex gap-1.5 overflow-x-auto rounded-2xl bg-white/90 p-1.5 backdrop-blur-md border border-slate-200/80 shadow-xs no-scrollbar">
                        <button
                            type="button"
                            onClick={() => setActiveTier("ALL")}
                            className={cn(
                                "shrink-0 rounded-xl px-3 py-1.5 text-xs font-semibold transition-all",
                                activeTier === "ALL"
                                    ? "bg-slate-900 text-white shadow-xs"
                                    : "text-slate-600 hover:bg-slate-100",
                            )}
                        >
                            全部 ({page.items.length})
                        </button>
                        {tiers.map((tier) => {
                            const count = page.items.filter(
                                (i) => i.tier_name === tier,
                            ).length
                            return (
                                <button
                                    key={tier}
                                    type="button"
                                    onClick={() => setActiveTier(tier)}
                                    className={cn(
                                        "shrink-0 rounded-xl px-3 py-1.5 text-xs font-semibold transition-all",
                                        activeTier === tier
                                            ? "bg-slate-900 text-white shadow-xs"
                                            : "text-slate-600 hover:bg-slate-100",
                                    )}
                                >
                                    {tier} ({count})
                                </button>
                            )
                        })}
                    </div>
                )}

                {/* 商品卡片列表 */}
                <fieldset disabled={locked || conflict} className="space-y-4">
                    {displayedItems.map((item) => {
                        const pick = picks[item.item_id]
                        const isSelected = pick?.selected ?? false
                        const image = publicImageUrl(token, item.cover_path)
                        const [intPart, decPart] = item.price.split(".")

                        return (
                            <article
                                key={item.item_id}
                                className={cn(
                                    "group relative overflow-hidden rounded-3xl border bg-white transition-all duration-200 shadow-xs",
                                    isSelected
                                        ? "border-primary/80 ring-2 ring-primary/15 bg-primary/[0.015] shadow-sm"
                                        : "border-slate-200/80 hover:border-slate-300",
                                )}
                            >
                                {/* 封面图 / 高品质占位区 */}
                                <div className="relative aspect-[16/10] w-full overflow-hidden bg-slate-100 sm:aspect-[16/9]">
                                    {image ? (
                                        // eslint-disable-next-line @next/next/no-img-element
                                        <img
                                            src={image}
                                            alt=""
                                            className="h-full w-full object-cover transition-transform duration-300 group-hover:scale-102"
                                            loading="lazy"
                                            referrerPolicy="no-referrer"
                                        />
                                    ) : (
                                        <div className="flex h-full w-full flex-col items-center justify-center bg-gradient-to-br from-slate-100 via-stone-50 to-slate-200/60 p-4 text-center">
                                            <div className="flex h-12 w-12 items-center justify-center rounded-2xl bg-white/80 shadow-xs text-slate-400">
                                                <Package className="h-6 w-6 stroke-[1.5]" />
                                            </div>
                                            <span className="mt-2 text-xs font-medium text-slate-500">
                                                精选商品资料
                                            </span>
                                        </div>
                                    )}

                                    {/* 档位浮动徽标 */}
                                    {item.tier_name && (
                                        <span className="absolute left-3 top-3 rounded-lg bg-slate-900/80 px-2.5 py-1 text-[11px] font-semibold text-white shadow-xs backdrop-blur-xs">
                                            {item.tier_name}
                                        </span>
                                    )}

                                    {/* 右上角选中状态圆标 */}
                                    <div className="absolute right-3 top-3">
                                        <span
                                            className={cn(
                                                "flex h-7 w-7 items-center justify-center rounded-full transition-all shadow-xs",
                                                isSelected
                                                    ? "bg-primary text-primary-foreground scale-100 ring-2 ring-white"
                                                    : "bg-black/25 text-transparent border border-white/60 backdrop-blur-xs scale-90",
                                            )}
                                        >
                                            <Check className="h-4 w-4 stroke-[3]" />
                                        </span>
                                    </div>
                                </div>

                                {/* 卡片信息与交互 */}
                                <div className="p-4 sm:p-5">
                                    <div className="flex items-start justify-between gap-3">
                                        <div className="min-w-0 flex-1">
                                            <h3 className="font-semibold text-slate-900 text-base leading-snug">
                                                {item.name}
                                            </h3>

                                            {/* 规格标签 */}
                                            {item.specification.length > 0 && (
                                                <div className="mt-2 flex flex-wrap gap-1.5">
                                                    {item.specification.map(
                                                        (s) => (
                                                            <span
                                                                key={s.name}
                                                                className="inline-flex items-center rounded-md bg-slate-100 px-2 py-0.5 text-xs text-slate-600"
                                                            >
                                                                {s.name}：
                                                                {s.value}
                                                            </span>
                                                        ),
                                                    )}
                                                </div>
                                            )}
                                        </div>
                                    </div>

                                    {/* 套餐包含明细 */}
                                    {item.members.length > 0 && (
                                        <div className="mt-3 rounded-2xl bg-slate-50/90 p-3 border border-slate-150">
                                            <p className="flex items-center gap-1.5 text-xs font-semibold text-slate-700 mb-1.5">
                                                <Layers className="h-3.5 w-3.5 text-primary" />
                                                礼盒包含 {item.members.length}{" "}
                                                款精选单品
                                            </p>
                                            <ul className="space-y-1 text-xs text-slate-500">
                                                {item.members.map(
                                                    (member, index) => (
                                                        <li
                                                            key={`${member.name}-${index}`}
                                                            className="flex items-center justify-between text-[11px]"
                                                        >
                                                            <span className="truncate text-slate-600">
                                                                {member.name}
                                                            </span>
                                                            <span className="shrink-0 text-slate-400 ml-2">
                                                                {member.specification
                                                                    .map(
                                                                        (s) =>
                                                                            s.value,
                                                                    )
                                                                    .join(
                                                                        " / ",
                                                                    )}
                                                                {member
                                                                    .specification
                                                                    .length > 0
                                                                    ? " · "
                                                                    : ""}
                                                                1 {member.unit}
                                                            </span>
                                                        </li>
                                                    ),
                                                )}
                                            </ul>
                                        </div>
                                    )}

                                    {/* 价格与操作栏 */}
                                    <div className="mt-4 flex items-end justify-between gap-2 border-t border-slate-100 pt-3">
                                        {/* 价格展示 */}
                                        <div>
                                            <span className="text-[11px] text-slate-400 block mb-0.5">
                                                {mall
                                                    ? "商城参考价值"
                                                    : "含税单价"}
                                            </span>
                                            <div className="flex items-baseline text-slate-900 font-semibold">
                                                <span className="text-xs mr-0.5 font-bold text-rose-600">
                                                    ¥
                                                </span>
                                                <span className="text-xl font-bold tracking-tight text-rose-600">
                                                    {intPart}
                                                </span>
                                                {decPart !== undefined && (
                                                    <span className="text-xs font-medium text-rose-600">
                                                        .{decPart}
                                                    </span>
                                                )}
                                            </div>
                                        </div>

                                        {/* 操作区域 */}
                                        <div className="flex items-center gap-2">
                                            {/* 按份步进器 */}
                                            {isSelected && !mall && (
                                                <div className="flex items-center gap-1 rounded-xl bg-slate-50 p-1 border border-slate-200">
                                                    <Button
                                                        type="button"
                                                        variant="ghost"
                                                        size="icon"
                                                        className="h-7 w-7 rounded-lg text-slate-600 hover:bg-slate-200"
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
                                                        <Minus className="h-3.5 w-3.5" />
                                                    </Button>
                                                    <Input
                                                        id={`sales-selection-public-qty-${item.item_id}`}
                                                        inputMode="numeric"
                                                        className="h-7 w-12 text-center text-xs font-bold border-0 bg-transparent px-0 py-0 shadow-none focus-visible:ring-0"
                                                        value={pick.quantity}
                                                        onChange={(e) =>
                                                            change(
                                                                item.item_id,
                                                                {
                                                                    quantity:
                                                                        e.target
                                                                            .value,
                                                                },
                                                            )
                                                        }
                                                        onClick={(e) =>
                                                            e.stopPropagation()
                                                        }
                                                    />
                                                    <Button
                                                        type="button"
                                                        variant="ghost"
                                                        size="icon"
                                                        className="h-7 w-7 rounded-lg text-slate-600 hover:bg-slate-200"
                                                        disabled={
                                                            locked || conflict
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
                                                        <Plus className="h-3.5 w-3.5" />
                                                    </Button>
                                                    <span className="text-xs text-slate-400 pr-1">
                                                        份
                                                    </span>
                                                </div>
                                            )}

                                            {/* 选择勾选控件 */}
                                            <label
                                                aria-label={item.name}
                                                htmlFor={`sales-selection-public-select-${item.item_id}`}
                                                className={cn(
                                                    "flex items-center gap-1.5 rounded-xl px-3.5 py-1.5 text-xs font-semibold transition-all cursor-pointer select-none",
                                                    isSelected
                                                        ? "bg-primary text-primary-foreground shadow-xs"
                                                        : "bg-slate-100 text-slate-700 hover:bg-slate-200",
                                                )}
                                            >
                                                <input
                                                    id={`sales-selection-public-select-${item.item_id}`}
                                                    aria-label={item.name}
                                                    type="checkbox"
                                                    checked={isSelected}
                                                    onChange={(e) =>
                                                        change(item.item_id, {
                                                            selected:
                                                                e.target
                                                                    .checked,
                                                        })
                                                    }
                                                    className="sr-only"
                                                />
                                                {isSelected ? (
                                                    <>
                                                        <Check className="h-3.5 w-3.5 stroke-[2.5]" />
                                                        <span>已选</span>
                                                    </>
                                                ) : (
                                                    <span>选择</span>
                                                )}
                                            </label>
                                        </div>
                                    </div>
                                </div>
                            </article>
                        )
                    })}
                </fieldset>

                {/* 核对清单（点击核对后展开的正式待确认面板） */}
                {confirmed && (
                    <section
                        className="mt-6 rounded-3xl border-2 border-primary/20 bg-primary/[0.02] p-5 space-y-4 shadow-sm"
                        aria-label="核对并提交"
                    >
                        <div className="flex items-center gap-2 text-primary">
                            <CheckCircle2 className="h-5 w-5" />
                            <h2 className="font-bold text-base text-slate-900">
                                请核对本次提交清单
                            </h2>
                        </div>
                        <p className="text-xs text-slate-500">
                            确认提交后将生成唯一的正式销售方案，并冻结当前会话。
                        </p>
                        <ChoiceSummary page={confirmed} />
                    </section>
                )}
            </div>

            {/* 核心体验升级：移动端吸底浮动结算栏 (Sticky Bottom Action Bar) */}
            <aside
                aria-label="核对并提交"
                className="fixed bottom-0 left-0 right-0 z-30 border-t border-slate-200/80 bg-white/95 px-4 py-3 backdrop-blur-md shadow-[0_-8px_24px_rgba(0,0,0,0.06)]"
            >
                <div className="mx-auto flex max-w-lg items-center justify-between gap-3">
                    {/* 左侧：已选概览 */}
                    <div className="flex items-center gap-2.5">
                        <div className="relative flex h-10 w-10 items-center justify-center rounded-2xl bg-slate-100 text-slate-800">
                            <ShoppingBag className="h-5 w-5" />
                            {selectedCount > 0 && (
                                <span className="absolute -right-1 -top-1 flex h-4.5 min-w-4.5 items-center justify-center rounded-full bg-primary px-1 text-[10px] font-bold text-primary-foreground shadow-xs">
                                    {selectedCount}
                                </span>
                            )}
                        </div>
                        <div>
                            <p className="text-sm font-semibold text-slate-900">
                                已选{" "}
                                <span className="text-primary font-bold">
                                    {selectedCount}
                                </span>{" "}
                                项
                            </p>
                            <p className="text-[11px] text-slate-400">
                                {dirty ? "尚有修改未保存" : "选择已同步"}
                            </p>
                        </div>
                    </div>

                    {/* 右侧：按钮组 */}
                    <div className="flex items-center gap-2">
                        <Button
                            id="sales-selection-public-save"
                            variant="outline"
                            size="sm"
                            className="rounded-xl border-slate-200 text-xs text-slate-700"
                            disabled={locked || conflict}
                            onClick={() => persist(false)}
                        >
                            保存选择
                        </Button>
                        {confirmed ? (
                            <Button
                                id="sales-selection-public-submit"
                                size="sm"
                                className="rounded-xl bg-emerald-600 text-xs font-semibold text-white shadow-xs hover:bg-emerald-700"
                                disabled={locked || conflict || dirty}
                                onClick={submit}
                            >
                                确认并提交选品
                            </Button>
                        ) : (
                            <Button
                                id="sales-selection-public-review"
                                size="sm"
                                className="rounded-xl bg-primary text-xs font-semibold text-primary-foreground shadow-xs"
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
