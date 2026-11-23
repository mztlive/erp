"use client"

import * as React from "react"
import Link from "next/link"
import { DownloadIcon, RefreshCwIcon } from "lucide-react"

import {
    DetailWorkspaceSection,
    MoneyValue,
    QuantityValue,
    detailWorkspaceStyles,
} from "@/components/business"
import { Button } from "@/components/ui/button"
import { LoadingButton } from "@/components/ui/loading-button"
import { StatusBadge } from "@/components/ui/status-badge"
import { toast } from "@/components/ui/toast"
import { useAccountProfileQuery } from "@/features/auth/queries"
import {
    useBookSelectionDetails,
    useBookVouchers,
} from "@/features/sales-selection/hooks/queries"
import {
    downloadBookSelectionDetails,
    downloadBookVouchers,
} from "@/features/sales-selection/lib/export-voucher-details"
import {
    bookIdentity,
    formatBookInstant,
} from "@/features/sales-selection/lib/presentation"
import type { SelectionBookDetail } from "@/features/sales-selection/types"
import { toAutomationIdSegment } from "@/lib/automation-id"
import { hasPermission } from "@/lib/permissions"

export function BookDetailVoucherPanel({
    detail,
}: {
    detail: SelectionBookDetail
}) {
    const bookId = bookIdentity(detail)
    const isVoucher = detail.submit_mode === "PICKUP_VOUCHER"
    const accountProfile = useAccountProfileQuery()
    const canReadVouchers = hasPermission(
        accountProfile.data?.permissions,
        "sales_selection_booklet:copy_link",
    )
    const canReadSelections = hasPermission(
        accountProfile.data?.permissions,
        "sales_selection_booklet:get",
    )
    const canOpenProposal = hasPermission(
        accountProfile.data?.permissions,
        "sales_selection_proposal:get",
    )
    const beforePublication =
        detail.status === "DRAFT" ||
        detail.status === "PREPARING" ||
        detail.status === "PENDING_PUBLISH"
    const vouchersEnabled = isVoucher && canReadVouchers && !beforePublication
    const selectionsEnabled = isVoucher && canReadSelections
    const vouchersQuery = useBookVouchers(bookId, vouchersEnabled)
    const selectionsQuery = useBookSelectionDetails(bookId, selectionsEnabled)
    const [exporting, setExporting] = React.useState<
        "vouchers" | "details" | null
    >(null)
    const vouchers = vouchersQuery.data ?? []
    const selections = selectionsQuery.data ?? []
    const refreshing = vouchersQuery.isFetching || selectionsQuery.isFetching

    if (!isVoucher) return null

    const exportVouchers = async () => {
        if (!vouchersEnabled || vouchers.length === 0) return
        setExporting("vouchers")
        try {
            const result = await vouchersQuery.refetch()
            if (result.isError || !result.data) {
                throw new Error("请刷新提货码后重试。")
            }
            if (result.data.length === 0) return
            await downloadBookVouchers(detail.customer_name, result.data)
        } catch {
            toast.add({
                title: "提货码导出失败",
                description: "请刷新提货码后重试。",
                type: "error",
            })
        } finally {
            setExporting(null)
        }
    }

    const exportDetails = async () => {
        if (!selectionsEnabled || selections.length === 0) return
        setExporting("details")
        try {
            const result = await selectionsQuery.refetch()
            if (result.isError || !result.data) {
                throw new Error("请刷新选品明细后重试。")
            }
            if (result.data.length === 0) return
            await downloadBookSelectionDetails(
                detail.customer_name,
                result.data,
            )
        } catch {
            toast.add({
                title: "选品明细导出失败",
                description: "请刷新选品明细后重试。",
                type: "error",
            })
        } finally {
            setExporting(null)
        }
    }

    return (
        <>
            <DetailWorkspaceSection
                title="提货券选品"
                ariaLabel="提货券选品"
                actions={
                    <>
                        <LoadingButton
                            id="sales-selection-detail-refresh-vouchers"
                            type="button"
                            variant="ghost"
                            className={detailWorkspaceStyles.subtleButton}
                            size="sm"
                            loading={refreshing && exporting === null}
                            disabled={
                                exporting !== null ||
                                (!vouchersEnabled && !selectionsEnabled)
                            }
                            onClick={() => {
                                if (vouchersEnabled) {
                                    void vouchersQuery.refetch()
                                }
                                if (selectionsEnabled) {
                                    void selectionsQuery.refetch()
                                }
                            }}
                        >
                            <RefreshCwIcon aria-hidden="true" />
                            刷新
                        </LoadingButton>
                        {canReadVouchers ? (
                            <LoadingButton
                                id="sales-selection-detail-export-vouchers"
                                type="button"
                                variant="ghost"
                                className={detailWorkspaceStyles.subtleButton}
                                size="sm"
                                loading={exporting === "vouchers"}
                                disabled={
                                    exporting !== null ||
                                    !vouchersEnabled ||
                                    vouchers.length === 0 ||
                                    vouchersQuery.isPending ||
                                    vouchersQuery.isError
                                }
                                onClick={() => void exportVouchers()}
                            >
                                <DownloadIcon aria-hidden="true" />
                                导出提货码
                            </LoadingButton>
                        ) : null}
                        {canReadSelections ? (
                            <LoadingButton
                                id="sales-selection-detail-export-selection-details"
                                type="button"
                                variant="ghost"
                                className={detailWorkspaceStyles.subtleButton}
                                size="sm"
                                loading={exporting === "details"}
                                disabled={
                                    exporting !== null ||
                                    selections.length === 0 ||
                                    selectionsQuery.isPending ||
                                    selectionsQuery.isError
                                }
                                onClick={() => void exportDetails()}
                            >
                                <DownloadIcon aria-hidden="true" />
                                导出全部选品明细
                            </LoadingButton>
                        ) : null}
                    </>
                }
            >
                <dl className="grid grid-cols-2 gap-4 sm:grid-cols-3">
                    <div className="grid gap-1">
                        <dt className="text-xs text-muted-foreground">
                            每人额度
                        </dt>
                        <dd className="font-semibold">
                            <MoneyValue value={detail.per_person_budget} />
                        </dd>
                    </div>
                    <div className="grid gap-1">
                        <dt className="text-xs text-muted-foreground">
                            提货码数量
                        </dt>
                        <dd className="num font-semibold">
                            {detail.voucher_count ?? vouchers.length} 个
                        </dd>
                    </div>
                    <div className="grid gap-1">
                        <dt className="text-xs text-muted-foreground">
                            已提交人数
                        </dt>
                        <dd className="num font-semibold">
                            {!canReadSelections || selectionsQuery.isPending
                                ? "—"
                                : selections.length}{" "}
                            人
                        </dd>
                    </div>
                </dl>
                {!canReadVouchers ? (
                    <p className="text-xs text-muted-foreground">
                        当前账号无权查看提货码。
                    </p>
                ) : beforePublication ? (
                    <p className="text-xs text-muted-foreground">
                        发布后生成提货码
                    </p>
                ) : vouchersQuery.isPending ? (
                    <p className="text-xs text-muted-foreground">
                        正在加载提货码…
                    </p>
                ) : vouchersQuery.isError ? (
                    <p
                        role="alert"
                        className="text-body-compact text-destructive"
                    >
                        提货码未加载成功，请点击刷新重试。
                    </p>
                ) : vouchers.length === 0 ? (
                    <p className="text-xs text-muted-foreground">
                        暂无可用提货码。
                    </p>
                ) : (
                    <div className="grid max-h-60 grid-cols-1 gap-2 overflow-y-auto sm:grid-cols-2 xl:grid-cols-3">
                        {vouchers.map((voucher) => (
                            <div
                                key={voucher.voucher_code}
                                className="flex items-center justify-between gap-3 rounded-md border px-3 py-2"
                            >
                                <span className="num break-all text-body-compact">
                                    {voucher.voucher_code}
                                </span>
                                <StatusBadge
                                    tone={
                                        voucher.submitted
                                            ? "success"
                                            : "neutral"
                                    }
                                    label={
                                        voucher.submitted ? "已提交" : "待选品"
                                    }
                                />
                            </div>
                        ))}
                    </div>
                )}
            </DetailWorkspaceSection>
            <DetailWorkspaceSection title="个人选品结果">
                {!canReadSelections ? (
                    <p className="text-xs text-muted-foreground">
                        当前账号无权查看个人选品结果。
                    </p>
                ) : selectionsQuery.isPending ? (
                    <p className="text-xs text-muted-foreground">
                        正在加载选品结果…
                    </p>
                ) : selectionsQuery.isError ? (
                    <p
                        role="alert"
                        className="text-body-compact text-destructive"
                    >
                        选品结果未加载成功，请点击刷新重试。
                    </p>
                ) : selections.length === 0 ? (
                    <p className="text-xs text-muted-foreground">
                        暂无已提交的选品结果。
                    </p>
                ) : (
                    selections.map((selection) => (
                        <article
                            key={selection.proposal_id}
                            className="grid min-w-0 gap-3 border-t border-border/60 pt-4"
                        >
                            <div className="flex flex-wrap items-center justify-between gap-2">
                                <div className="grid gap-1">
                                    <p className="text-body-compact font-medium">
                                        {selection.recipient?.name ??
                                            "未填写收件人"}
                                        <span className="num ml-3 font-normal text-muted-foreground">
                                            {selection.recipient?.phone ?? "—"}
                                        </span>
                                    </p>
                                    <p className="text-xs text-muted-foreground">
                                        提货码：
                                        <span className="num">
                                            {selection.voucher_code ?? "—"}
                                        </span>{" "}
                                        ·{" "}
                                        <span className="num">
                                            {formatBookInstant(
                                                selection.submitted_at,
                                            )}
                                        </span>
                                    </p>
                                </div>
                                {canOpenProposal ? (
                                    <Button
                                        id={`sales-selection-voucher-open-proposal-${toAutomationIdSegment(selection.voucher_code ?? selection.proposal_no)}`}
                                        type="button"
                                        size="sm"
                                        variant="ghost"
                                        className={
                                            detailWorkspaceStyles.inlineLinkButton
                                        }
                                        render={
                                            <Link
                                                href={`/sales/selection/proposals/${encodeURIComponent(selection.proposal_id)}`}
                                            />
                                        }
                                    >
                                        查看方案{" "}
                                        <span className="num">
                                            {selection.proposal_no}
                                        </span>
                                    </Button>
                                ) : null}
                            </div>
                            <p className="break-words text-body-compact">
                                <span className="text-muted-foreground">
                                    收件地址：
                                </span>
                                {[
                                    selection.recipient?.province,
                                    selection.recipient?.city,
                                    selection.recipient?.district,
                                    selection.recipient?.address,
                                ]
                                    .filter(Boolean)
                                    .join(" ") || "—"}
                            </p>
                            <ul className="grid gap-2 border-t pt-3">
                                {selection.items.map((item) => (
                                    <li
                                        key={JSON.stringify([
                                            item.display_item_id,
                                            item.name,
                                            item.specification ?? [],
                                            item.unit,
                                        ])}
                                        className="flex flex-wrap items-baseline justify-between gap-2 text-body-compact"
                                    >
                                        <span>
                                            {item.name}
                                            <span className="ml-2 text-xs text-muted-foreground">
                                                {(item.specification ?? [])
                                                    .map(
                                                        (spec) =>
                                                            `${spec.name}：${spec.value}`,
                                                    )
                                                    .join(" / ")}
                                            </span>
                                        </span>
                                        <span className="flex items-baseline gap-3">
                                            <QuantityValue
                                                value={
                                                    item.quantity?.toString() ??
                                                    "—"
                                                }
                                                unit={item.unit ?? "件"}
                                            />
                                            <MoneyValue
                                                value={item.line_amount}
                                            />
                                        </span>
                                    </li>
                                ))}
                            </ul>
                            <p className="flex items-baseline justify-end gap-2 text-body-compact font-medium">
                                选品合计
                                <MoneyValue value={selection.total_amount} />
                            </p>
                        </article>
                    ))
                )}
            </DetailWorkspaceSection>
        </>
    )
}
