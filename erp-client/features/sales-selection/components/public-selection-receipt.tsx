"use client"

import * as React from "react"
import { useMutation } from "@tanstack/react-query"
import { CheckCircle2, Download, ImageOff } from "lucide-react"
import { MoneyValue, QuantityValue } from "@/components/business/values"
import { Alert, AlertDescription } from "@/components/ui/alert"
import { Button } from "@/components/ui/button"
import { LoadingButton } from "@/components/ui/loading-button"
import { publicImageUrl } from "../api"
import { receiptContent, type ReceiptContent } from "../lib/receipt"
import type { PublicPageView, PublicReceiptView } from "../types"

/** 客户提交后的只读回执，金额始终采用服务端提交结果。 */
export function PublicSelectionReceipt({
    token,
    accessToken,
    onLock,
    page,
    receipt,
}: {
    token: string
    accessToken: string
    onLock: () => void
    page: PublicPageView
    receipt: PublicReceiptView
}) {
    const content = receiptContent(page, receipt)
    const recipient = page.recipient ?? receipt.recipient
    const download = useMutation({
        mutationFn: async () => {
            const { downloadReceipt } = await import("../lib/export-receipt")
            await downloadReceipt({ ...receipt, recipient }, content)
        },
    })

    return (
        <main className="min-h-svh bg-muted text-foreground">
            <div className="mx-auto max-w-lg px-4 pt-6 pb-10">
                <header className="mb-4 text-center">
                    <div className="mx-auto mb-2 flex size-12 items-center justify-center rounded-full bg-success-soft text-success-soft-foreground">
                        <CheckCircle2 className="size-8" aria-hidden="true" />
                    </div>
                    <h1 className="text-2xl font-semibold tracking-tight">
                        选品已提交
                    </h1>
                    <p className="mt-1.5 text-sm text-muted-foreground">
                        您的选品结果已保存
                    </p>
                </header>

                <article
                    aria-label="选品确认回执"
                    className="rounded-2xl bg-card px-4 shadow-xs sm:px-6"
                >
                    <header className="border-b border-grid py-4">
                        <p className="text-sm font-medium text-success">
                            选品确认回执
                        </p>
                        <h2 className="mt-2 break-words text-lg leading-snug font-semibold">
                            {receipt.customer_name}
                        </h2>
                        <dl className="mt-2 space-y-1 text-xs text-muted-foreground">
                            <div className="flex gap-2">
                                <dt className="shrink-0">方案编号</dt>
                                <dd className="num min-w-0 break-all">
                                    {receipt.proposal_no}
                                </dd>
                            </div>
                            <div className="flex gap-2">
                                <dt className="shrink-0">提交时间</dt>
                                <dd className="num">{content.submittedAt}</dd>
                            </div>
                        </dl>
                    </header>

                    <section aria-labelledby="sales-selection-receipt-items-title">
                        <div className="flex flex-wrap items-baseline justify-between gap-2 pt-4 pb-1">
                            <h3
                                id="sales-selection-receipt-items-title"
                                className="text-base font-semibold"
                            >
                                已选商品
                            </h3>
                            <p className="text-xs text-muted-foreground">
                                <span className="num">
                                    {content.items.length}
                                </span>{" "}
                                款
                                {content.quantity != null && (
                                    <>
                                        {" "}
                                        · 共{" "}
                                        <QuantityValue
                                            value={content.quantity}
                                            unit="份"
                                        />
                                    </>
                                )}
                            </p>
                        </div>
                        <ul className="divide-y divide-grid">
                            {content.items.map((item) => (
                                <ReceiptItem
                                    key={item.id}
                                    item={item}
                                    token={token}
                                    accessToken={accessToken}
                                    byQuantity={content.byQuantity}
                                />
                            ))}
                        </ul>
                        {content.items.length === 0 && (
                            <p className="py-6 text-sm text-muted-foreground">
                                暂无已提交商品，请联系销售核对。
                            </p>
                        )}
                        {content.byQuantity && (
                            <div className="flex flex-wrap items-baseline justify-between gap-x-4 gap-y-2 border-t border-grid py-4">
                                <span className="text-sm text-muted-foreground">
                                    合计（含税）
                                </span>
                                <MoneyValue
                                    value={content.total}
                                    size="summary"
                                    className="max-w-full break-all"
                                />
                            </div>
                        )}
                    </section>

                    {recipient && (
                        <section
                            className="space-y-2 border-t border-grid py-4"
                            aria-label="收件信息"
                        >
                            <h3 className="text-base font-semibold">
                                收件信息
                            </h3>
                            <p className="text-sm">
                                {recipient.name} · {recipient.phone}
                            </p>
                            <p className="break-words text-sm text-muted-foreground">
                                {[
                                    recipient.province,
                                    recipient.city,
                                    recipient.district,
                                    recipient.address,
                                ].join(" ")}
                            </p>
                        </section>
                    )}
                    <footer className="flex items-center gap-2 border-t border-grid py-3 text-xs text-muted-foreground">
                        <CheckCircle2
                            className="size-4 shrink-0 text-success"
                            aria-hidden="true"
                        />
                        选品结果已保存
                    </footer>
                </article>

                {page.notices.length > 0 && (
                    <div className="mt-4 space-y-2">
                        {page.notices.map((notice) => (
                            <p
                                key={notice}
                                className="text-xs leading-relaxed text-muted-foreground"
                            >
                                {notice}
                            </p>
                        ))}
                    </div>
                )}
                <div className="mt-6 space-y-3">
                    <Button
                        id="sales-selection-public-lock"
                        variant="outline"
                        className="w-full"
                        onClick={onLock}
                    >
                        {page.submit_mode === "PICKUP_VOUCHER"
                            ? "更换提货码"
                            : "退出选品"}
                    </Button>
                    <LoadingButton
                        id="sales-selection-receipt-download"
                        size="lg"
                        className="w-full"
                        loading={download.isPending}
                        onClick={() => download.mutate()}
                        title="下载 Excel 选品清单"
                    >
                        <Download data-icon="inline-start" aria-hidden="true" />
                        {download.isPending ? "正在生成清单…" : "保存选品清单"}
                    </LoadingButton>
                    {download.isError && (
                        <Alert variant="destructive">
                            <AlertDescription>
                                清单生成失败，请重试。
                            </AlertDescription>
                        </Alert>
                    )}
                    {download.isSuccess && (
                        <p
                            role="status"
                            className="text-center text-xs text-muted-foreground"
                        >
                            Excel 清单已生成，请在下载记录中查看。
                        </p>
                    )}
                    <p className="text-center text-xs text-muted-foreground">
                        如需调整，请联系销售
                    </p>
                </div>
            </div>
        </main>
    )
}

function ReceiptItem({
    item,
    token,
    accessToken,
    byQuantity,
}: {
    item: ReceiptContent["items"][number]
    token: string
    accessToken: string
    byQuantity: boolean
}) {
    const src = publicImageUrl(token, item.coverPath, accessToken)
    const [failedSrc, setFailedSrc] = React.useState<string>()
    return (
        <li className="flex items-start gap-3 py-3">
            <div className="flex size-16 shrink-0 items-center justify-center overflow-hidden rounded-lg bg-muted">
                {src && failedSrc !== src ? (
                    // eslint-disable-next-line @next/next/no-img-element
                    <img
                        src={src}
                        alt={item.name}
                        className="size-full object-cover"
                        loading="lazy"
                        referrerPolicy="no-referrer"
                        onError={() => setFailedSrc(src)}
                    />
                ) : (
                    <div className="flex flex-col items-center gap-1 text-muted-foreground">
                        <ImageOff className="size-5" aria-hidden="true" />
                        <span className="text-tiny">
                            {src ? "图片不可用" : "暂无图片"}
                        </span>
                    </div>
                )}
            </div>
            <div className="min-w-0 flex-1 self-center">
                <div className="flex flex-wrap items-baseline justify-between gap-x-3 gap-y-1">
                    <p className="min-w-0 break-words text-sm leading-snug font-medium">
                        {item.name}
                    </p>
                    {byQuantity && (
                        <MoneyValue
                            value={item.amount}
                            className="ml-auto max-w-full break-all text-sm font-semibold"
                        />
                    )}
                </div>
                {item.quantity != null && (
                    <p className="mt-1.5 text-xs text-muted-foreground">
                        数量：
                        <QuantityValue value={item.quantity} unit="份" />
                    </p>
                )}
                {item.specification.length > 0 && (
                    <p className="mt-1 break-words text-xs leading-relaxed text-muted-foreground">
                        {item.specification
                            .map((spec) => `${spec.name}：${spec.value}`)
                            .join(" / ")}
                    </p>
                )}
                {item.members.length > 0 && (
                    <ul className="mt-2 space-y-1 border-l border-border pl-2 text-xs leading-relaxed text-muted-foreground">
                        {item.members.map((member, index) => (
                            <li
                                key={`${member.name}-${index}`}
                                className="break-words"
                            >
                                {[
                                    member.name,
                                    member.specification
                                        .map(
                                            (spec) =>
                                                `${spec.name}：${spec.value}`,
                                        )
                                        .join(" / "),
                                    `1 ${member.unit}`,
                                ]
                                    .filter(Boolean)
                                    .join(" · ")}
                            </li>
                        ))}
                    </ul>
                )}
            </div>
        </li>
    )
}
