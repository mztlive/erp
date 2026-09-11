"use client"

import * as React from "react"
import Link from "next/link"

import { PageScaffold } from "@/components/business"
import { Button } from "@/components/ui/button"
import {
    copyBookletLink,
    closeBooklet,
    deleteDisplayItem,
    prepareBooklet,
    publishBooklet,
    rotateBookletLink,
    voidBooklet,
} from "@/features/sales-selection/api"
import { useBookletQuery } from "@/features/sales-selection/queries"
import {
    BOOKLET_STATUS_LABEL,
    FORM_LABEL,
    SUBMIT_MODE_LABEL,
} from "@/features/sales-selection/types"

export function BookletDetailPage({ bookletId }: { bookletId: string }) {
    const query = useBookletQuery(bookletId)
    const booklet = query.data
    const [pending, setPending] = React.useState(false)
    if (!booklet) {
        return <PageScaffold density="compact">正在加载选品册…</PageScaffold>
    }
    const run = async (fn: () => Promise<unknown>) => {
        setPending(true)
        try {
            await fn()
            await query.refetch()
        } finally {
            setPending(false)
        }
    }
    return (
        <PageScaffold density="compact" className="gap-4">
            <div>
                <p className="text-sm text-muted-foreground">选品册</p>
                <h1 className="text-xl font-semibold">
                    {booklet.customer_name}
                </h1>
                <p className="text-sm text-muted-foreground">
                    {FORM_LABEL[booklet.form]} ·{" "}
                    {SUBMIT_MODE_LABEL[booklet.submit_mode]} ·{" "}
                    {BOOKLET_STATUS_LABEL[booklet.status]}
                </p>
            </div>
            <div className="flex flex-wrap gap-2">
                {booklet.status === "DRAFT" ||
                booklet.status === "PENDING_PUBLISH" ? (
                    <Button
                        id="sales-selection-prepare"
                        disabled={pending}
                        onClick={() =>
                            void run(() =>
                                prepareBooklet(booklet.id, {
                                    idempotencyKey: crypto.randomUUID(),
                                    expectedVersion: booklet.version,
                                    kind:
                                        booklet.status === "DRAFT"
                                            ? "FIRST_PREPARE"
                                            : "RE_PREPARE",
                                }),
                            )
                        }
                    >
                        {booklet.status === "DRAFT"
                            ? "开始准备"
                            : "整册重新准备"}
                    </Button>
                ) : null}
                {booklet.status === "PENDING_PUBLISH" && booklet.batch_id ? (
                    <Button
                        id="sales-selection-publish"
                        disabled={pending}
                        onClick={() =>
                            void run(() =>
                                publishBooklet(booklet.id, {
                                    idempotencyKey: crypto.randomUUID(),
                                    expectedVersion: booklet.version,
                                    batchId: booklet.batch_id ?? undefined,
                                }),
                            )
                        }
                    >
                        发布
                    </Button>
                ) : null}
                {booklet.status === "PUBLISHED" ? (
                    <>
                        <Button
                            id="sales-selection-copy-link"
                            disabled={pending}
                            onClick={() =>
                                void run(async () => {
                                    const view = await copyBookletLink(
                                        booklet.id,
                                    )
                                    if (view.public_path) {
                                        await navigator.clipboard.writeText(
                                            `${window.location.origin}${view.public_path}`,
                                        )
                                    }
                                })
                            }
                        >
                            复制链接
                        </Button>
                        <Button
                            id="sales-selection-rotate-link"
                            variant="outline"
                            disabled={pending}
                            onClick={() =>
                                void run(() =>
                                    rotateBookletLink(booklet.id, {
                                        idempotencyKey: crypto.randomUUID(),
                                        expectedVersion: booklet.version,
                                    }),
                                )
                            }
                        >
                            更换链接
                        </Button>
                        <Button
                            id="sales-selection-close"
                            variant="outline"
                            disabled={pending}
                            onClick={() =>
                                void run(() =>
                                    closeBooklet(booklet.id, {
                                        idempotencyKey: crypto.randomUUID(),
                                        expectedVersion: booklet.version,
                                    }),
                                )
                            }
                        >
                            关闭
                        </Button>
                    </>
                ) : null}
                {booklet.status === "DRAFT" ||
                booklet.status === "PENDING_PUBLISH" ? (
                    <Button
                        id="sales-selection-void"
                        variant="ghost"
                        disabled={pending}
                        onClick={() =>
                            void run(() =>
                                voidBooklet(booklet.id, {
                                    idempotencyKey: crypto.randomUUID(),
                                    expectedVersion: booklet.version,
                                }),
                            )
                        }
                    >
                        作废
                    </Button>
                ) : null}
                {booklet.proposal_id ? (
                    <Button
                        id="sales-selection-open-proposal"
                        nativeButton={false}
                        render={
                            <Link
                                href={`/sales/proposals/${booklet.proposal_id}`}
                            />
                        }
                        variant="outline"
                    >
                        查看方案
                    </Button>
                ) : null}
            </div>
            {booklet.last_prepare_failure ? (
                <p className="text-sm text-destructive">
                    {booklet.last_prepare_failure}
                </p>
            ) : null}
            <p className="text-sm text-muted-foreground">
                准备时间 {booklet.prepared_at ?? "—"} · 资格日期{" "}
                {booklet.eligibility_as_of ?? "—"} · 陈列{" "}
                {booklet.display_count} · 已删 {booklet.removed_count} · 缺图{" "}
                {booklet.missing_image_count}
            </p>
            {booklet.tier_reports.map((report) => (
                <p key={report.tier_id} className="text-sm">
                    档位 {report.tier_id}：期望 {report.expected_count}，实际{" "}
                    {report.actual_count}，{report.stop_label}
                    {report.image_failures > 0
                        ? `，图片失败 ${report.image_failures}`
                        : ""}
                </p>
            ))}
            <div className="grid gap-3 sm:grid-cols-2 lg:grid-cols-3">
                {booklet.items.map((item) => (
                    <article
                        key={item.id}
                        className="rounded-xl border p-3"
                        id={`sales-selection-item-${item.id}`}
                    >
                        <p className="font-medium">{item.name}</p>
                        <p className="text-sm tabular-nums">¥ {item.price}</p>
                        {item.missing_image ? (
                            <p className="text-xs text-muted-foreground">
                                暂无图片
                            </p>
                        ) : null}
                        {item.removed ? (
                            <p className="text-xs text-muted-foreground">
                                已删除
                            </p>
                        ) : null}
                        {booklet.status === "PENDING_PUBLISH" &&
                        !item.removed ? (
                            <Button
                                id={`sales-selection-delete-item-${item.id}`}
                                size="sm"
                                variant="ghost"
                                disabled={pending}
                                onClick={() =>
                                    void run(() =>
                                        deleteDisplayItem(
                                            booklet.id,
                                            item.id,
                                            booklet.version,
                                        ),
                                    )
                                }
                            >
                                删除
                            </Button>
                        ) : null}
                    </article>
                ))}
            </div>
        </PageScaffold>
    )
}
