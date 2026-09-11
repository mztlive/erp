"use client"

import * as React from "react"
import { useStore } from "@tanstack/react-form"
import { useMutation, useQueryClient } from "@tanstack/react-query"
import { useAppForm } from "@/components/form"
import { Button } from "@/components/ui/button"
import { Input } from "@/components/ui/input"
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
            <main className="mx-auto max-w-md p-4">
                <h1 className="text-lg font-semibold">
                    {requestFailure(query.error) === "ended"
                        ? "选品链接已失效"
                        : "暂时无法打开选品页"}
                </h1>
                <p className="my-3 text-sm">
                    {requestFailure(query.error) === "ended"
                        ? "请联系销售核对当前链接。"
                        : "请检查网络连接后重试。"}
                </p>
                {requestFailure(query.error) !== "ended" && (
                    <Button
                        id="sales-selection-public-retry"
                        onClick={() => void query.refetch()}
                    >
                        重新打开
                    </Button>
                )}
            </main>
        )
    if (!page)
        return <main className="mx-auto max-w-md p-4">正在打开选品页…</main>
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
        <main className="mx-auto max-w-md p-4">
            <h1 className="text-lg font-semibold">选品已结束</h1>
            <p className="mt-2 text-sm">链接已失效，请联系销售核对。</p>
        </main>
    )
}

/** 显示服务端已提交的客户明细和金额。 */
const Receipt = ({ page }: { page: PublicPageView }) => {
    const receipt = page.receipt!
    return (
        <main className="mx-auto max-w-md space-y-3 p-4">
            <h1 className="text-lg font-semibold">已提交选品</h1>
            <p>{receipt.customer_name}</p>
            <p className="text-sm">方案编号 {receipt.proposal_no}</p>
            <p className="text-sm">
                提交时间：{formatTime(receipt.submitted_at)}
            </p>
            <ChoiceSummary page={page} receipt />
            {page.notices.map((notice) => (
                <p key={notice} className="text-xs text-muted-foreground">
                    {notice}
                </p>
            ))}
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
        <div className="space-y-2 rounded-lg border p-3 text-sm">
            {choices.map((choice) => {
                const item = page.items.find(
                    (item) => item.item_id === choice.item_id,
                )
                return (
                    <div key={choice.item_id}>
                        <p className="font-medium">
                            {item?.name ?? "商品资料暂不可用，请联系销售核对"}
                            {choice.quantity != null
                                ? ` × ${choice.quantity} 份`
                                : ""}
                        </p>
                        {item?.specification.map((spec) => (
                            <span key={spec.name} className="mr-2 text-xs">
                                {spec.name}：{spec.value}
                            </span>
                        ))}
                        {item?.members.map((member, index) => (
                            <p
                                key={`${member.name}-${index}`}
                                className="text-xs text-muted-foreground"
                            >
                                {member.name} ·{" "}
                                {member.specification
                                    .map((s) => `${s.name}：${s.value}`)
                                    .join(" / ")}{" "}
                                · {member.unit}
                            </p>
                        ))}
                        {choice.line_amount != null && (
                            <p>行金额 ¥ {choice.line_amount}</p>
                        )}
                    </div>
                )
            })}
            {!choices.length && <p>尚未选择商品</p>}
            {page.submit_mode === "BY_QUANTITY" && total != null && (
                <p className="font-semibold">合计 ¥ {total}</p>
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
    return (
        <main className="mx-auto max-w-md p-4 pb-8">
            <header className="mb-4 space-y-2">
                <h1 className="text-lg font-semibold">
                    {page.customer_name} 选品
                </h1>
                {page.notices.map((notice) => (
                    <p key={notice} className="text-xs text-muted-foreground">
                        {notice}
                    </p>
                ))}
            </header>
            {(message || request) && (
                <div
                    role="status"
                    className="mb-4 rounded-lg border p-3 text-sm"
                >
                    <p>{message || "上次操作结果待核对，请先恢复本次请求。"}</p>
                    {request && !mutation.isPending && (
                        <Button
                            id="sales-selection-public-reconcile"
                            className="mt-2"
                            onClick={() => mutation.mutate(request)}
                        >
                            核对并恢复本次操作
                        </Button>
                    )}
                </div>
            )}
            {conflict && (
                <section className="mb-4 space-y-2" aria-label="最新保存的选择">
                    <h2 className="font-medium">其他页面最新保存的选择</h2>
                    {latest ? (
                        <ChoiceSummary page={latest} />
                    ) : (
                        <p>最新清单暂时无法读取，请重试。</p>
                    )}
                    <Button
                        id="sales-selection-public-reload-conflict"
                        variant="outline"
                        onClick={async () =>
                            setLatest((await refresh()) ?? null)
                        }
                    >
                        重新读取最新清单
                    </Button>
                    <Button
                        id="sales-selection-public-acknowledge"
                        disabled={!latest || latest.kind !== "SELECTING"}
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
                </section>
            )}
            <fieldset disabled={locked || conflict} className="space-y-3">
                {page.items.map((item) => {
                    const pick = picks[item.item_id]
                    const image = publicImageUrl(token, item.cover_path)
                    return (
                        <article
                            key={item.item_id}
                            className="rounded-xl border p-3"
                        >
                            {item.tier_name && (
                                <p className="mb-2 text-sm font-semibold">
                                    {item.tier_name}
                                </p>
                            )}
                            {image ? (
                                // eslint-disable-next-line @next/next/no-img-element
                                <img
                                    src={image}
                                    alt=""
                                    className="mb-2 h-40 w-full rounded-lg object-cover"
                                    referrerPolicy="no-referrer"
                                />
                            ) : (
                                <div className="mb-2 flex h-32 items-center justify-center rounded-lg bg-muted text-sm">
                                    暂无图片
                                </div>
                            )}
                            <label
                                aria-label={item.name}
                                className="flex items-start gap-2"
                                htmlFor={`sales-selection-public-select-${item.item_id}`}
                            >
                                <input
                                    id={`sales-selection-public-select-${item.item_id}`}
                                    type="checkbox"
                                    checked={pick?.selected ?? false}
                                    onChange={(e) =>
                                        change(item.item_id, {
                                            selected: e.target.checked,
                                        })
                                    }
                                />
                                <span className="min-w-0 break-words">
                                    <span className="block font-medium">
                                        {item.name}
                                    </span>
                                    <span>¥ {item.price}</span>
                                </span>
                            </label>
                            <p className="mt-1 text-xs">
                                {item.specification
                                    .map((s) => `${s.name}：${s.value}`)
                                    .join(" / ")}
                            </p>
                            {item.members.length > 0 && (
                                <ul className="mt-2 space-y-1 text-xs text-muted-foreground">
                                    {item.members.map((member, index) => (
                                        <li key={`${member.name}-${index}`}>
                                            {member.name} ·{" "}
                                            {member.specification
                                                .map(
                                                    (s) =>
                                                        `${s.name}：${s.value}`,
                                                )
                                                .join(" / ")}{" "}
                                            · {member.unit}
                                        </li>
                                    ))}
                                </ul>
                            )}
                            {pick?.selected && !mall && (
                                <div className="mt-2">
                                    <label
                                        className="text-xs"
                                        htmlFor={`sales-selection-public-qty-${item.item_id}`}
                                    >
                                        采购份数
                                    </label>
                                    <Input
                                        id={`sales-selection-public-qty-${item.item_id}`}
                                        inputMode="numeric"
                                        value={pick.quantity}
                                        onChange={(e) =>
                                            change(item.item_id, {
                                                quantity: e.target.value,
                                            })
                                        }
                                    />
                                </div>
                            )}
                        </article>
                    )
                })}
            </fieldset>
            <section
                className="mt-5 space-y-3 border-t pt-4"
                aria-label="核对并提交"
            >
                <p className="text-sm">
                    已选{" "}
                    {
                        Object.values(picks).filter((pick) => pick.selected)
                            .length
                    }{" "}
                    项{dirty ? " · 尚有修改未保存" : ""}
                </p>
                {confirmed && (
                    <>
                        <h2 className="font-medium">请核对本次提交清单</h2>
                        <ChoiceSummary page={confirmed} />
                    </>
                )}
                <div className="flex flex-wrap gap-2">
                    <Button
                        id="sales-selection-public-save"
                        variant="outline"
                        disabled={locked || conflict}
                        onClick={() => persist(false)}
                    >
                        保存选择
                    </Button>
                    {confirmed ? (
                        <Button
                            id="sales-selection-public-submit"
                            disabled={locked || conflict || dirty}
                            onClick={submit}
                        >
                            确认并提交选品
                        </Button>
                    ) : (
                        <Button
                            id="sales-selection-public-review"
                            disabled={locked || conflict}
                            onClick={() => persist(true)}
                        >
                            核对并提交
                        </Button>
                    )}
                </div>
            </section>
        </main>
    )
}

/** 将服务端秒时间戳格式化为客户本地时间。 */
const formatTime = (value: number) => {
    return new Date(value * 1000).toLocaleString("zh-CN")
}
