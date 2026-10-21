"use client"

import { useState } from "react"
import { useSelector } from "@tanstack/react-form"
import { BusinessFailureState } from "@/components/business"
import { useAppForm } from "@/components/form"
import { Button } from "@/components/ui/button"
import {
    Dialog,
    DialogContent,
    DialogDescription,
    DialogFooter,
    DialogHeader,
    DialogTitle,
} from "@/components/ui/dialog"
import { Input } from "@/components/ui/input"
import { getErrorCode } from "@/lib/api/errors"
import { toAutomationIdSegment } from "@/lib/automation-id"
import {
    inquiryCandidateBlocker,
    type InquiryCandidateQuery,
} from "../api/inquiry-candidates"
import { useInquiryCandidates } from "../hooks/use-inquiry-candidates"
import { useInquirySalesBasis } from "../hooks/use-inquiry-sales-basis"

/** 选择和校验只读；确认之前不改写原销售表单。 */
export function SalesOrderInquiryPickerDialog({
    customerId,
    discardsChanges,
    onClose,
    onConfirm,
}: {
    customerId: string
    discardsChanges: boolean
    onClose: () => void
    onConfirm: (inquiryId: string, round: number) => void
}) {
    const [query, setQuery] = useState<InquiryCandidateQuery>({
        customer_id: customerId || undefined,
        page: 1,
    })
    const form = useAppForm({
        defaultValues: { search: "", inquiryId: "" },
        onSubmit: ({ value }) => {
            form.setFieldValue("inquiryId", "")
            setQuery({
                customer_id: customerId || undefined,
                q: value.search.trim() || undefined,
                page: 1,
            })
        },
    })
    const selectedId = useSelector(
        form.store,
        (state) => state.values.inquiryId,
    )
    const list = useInquiryCandidates(query)
    const selected = list.data?.items.find((row) => row.id === selectedId)
    const round = selected?.acceptances.at(-1)?.feedback_round ?? 0
    const basis = useInquirySalesBasis(selectedId, round)
    const canConfirm = Boolean(
        selected &&
        !inquiryCandidateBlocker(selected) &&
        !list.isFetching &&
        !list.isError &&
        basis.data &&
        !basis.isFetching &&
        !basis.isError &&
        basis.data.inquiry_id === selectedId &&
        basis.data.feedback_round === round &&
        basis.data.lineItems.length > 0 &&
        (!customerId || basis.data.customer_id === customerId),
    )
    const changePage = (page: number) => {
        form.setFieldValue("inquiryId", "")
        setQuery({ ...query, page, scope_version: list.data?.scope_version })
    }
    const retryList = () => {
        form.setFieldValue("inquiryId", "")
        if (
            getErrorCode(list.error) === "DATA_SCOPE_CHANGED" &&
            query.scope_version
        ) {
            setQuery({ ...query, page: 1, scope_version: undefined })
        } else {
            void list.refetch()
        }
    }

    return (
        <Dialog open onOpenChange={(open) => !open && onClose()}>
            <DialogContent
                className="max-h-[calc(100dvh-2rem)] overflow-y-auto sm:max-w-2xl"
                closeButtonId="sales-inquiry-picker-close"
            >
                <DialogHeader>
                    <DialogTitle>从询价单带入</DialogTitle>
                    <DialogDescription>
                        {customerId
                            ? "仅展示当前客户的询价。"
                            : "可搜索询价单号或客户名称。"}
                        选择已接受且仍可开单的结果，带入客户、商品、交期和配送信息；成交价须逐项确认。
                    </DialogDescription>
                </DialogHeader>
                <form
                    className="flex gap-2"
                    onSubmit={(event) => {
                        event.preventDefault()
                        void form.handleSubmit()
                    }}
                >
                    <form.AppField name="search">
                        {(field) => (
                            <Input
                                id="sales-inquiry-picker-search"
                                aria-label="搜索询价单号或客户名称"
                                placeholder="搜索询价单号或客户名称"
                                value={field.state.value}
                                onChange={(event) =>
                                    field.handleChange(event.target.value)
                                }
                            />
                        )}
                    </form.AppField>
                    <Button
                        id="sales-inquiry-picker-search-submit"
                        type="submit"
                        variant="outline"
                    >
                        搜索
                    </Button>
                </form>
                <div
                    className="space-y-2"
                    aria-live="polite"
                    aria-busy={list.isFetching}
                >
                    {list.isError ? (
                        <BusinessFailureState
                            idPrefix="sales-inquiry-picker-list"
                            error={list.error}
                            onRetry={retryList}
                            retryLabel="重新加载询价"
                            action={
                                getErrorCode(list.error) ===
                                "DATA_SCOPE_CHANGED" ? (
                                    <Button
                                        id="sales-inquiry-picker-reset-scope"
                                        type="button"
                                        variant="outline"
                                        onClick={retryList}
                                    >
                                        从第一页重新加载
                                    </Button>
                                ) : undefined
                            }
                        />
                    ) : list.isPending ? (
                        <p className="text-sm text-muted-foreground">
                            正在加载询价…
                        </p>
                    ) : list.data.items.length === 0 ? (
                        <p className="text-sm text-muted-foreground">
                            没有符合条件的询价，请调整搜索条件或取消后核对客户。
                        </p>
                    ) : (
                        list.data.items.map((row) => {
                            const blocker = inquiryCandidateBlocker(row)
                            return (
                                <Button
                                    key={row.id}
                                    id={`sales-inquiry-picker-option-${toAutomationIdSegment(row.id)}`}
                                    type="button"
                                    variant={
                                        selectedId === row.id
                                            ? "secondary"
                                            : "outline"
                                    }
                                    className="h-auto w-full flex-col items-start whitespace-normal px-4 py-3 text-left"
                                    aria-pressed={selectedId === row.id}
                                    disabled={
                                        Boolean(blocker) || list.isFetching
                                    }
                                    onClick={() =>
                                        form.setFieldValue("inquiryId", row.id)
                                    }
                                >
                                    <span>
                                        {row.inquiry_no} · {row.customer_name}
                                    </span>
                                    <span className="text-xs font-normal text-muted-foreground">
                                        {blocker ??
                                            `可开单 · 第 ${row.acceptances.at(-1)?.feedback_round} 轮已接受结果`}
                                    </span>
                                </Button>
                            )
                        })
                    )}
                </div>
                {list.data && !list.isError && (
                    <div className="flex items-center justify-between gap-2 text-sm">
                        <span className="text-muted-foreground">
                            共 {list.data.total} 张询价 · 第 {query.page} 页
                        </span>
                        <div className="flex gap-2">
                            <Button
                                id="sales-inquiry-picker-previous"
                                type="button"
                                variant="outline"
                                disabled={query.page <= 1 || list.isFetching}
                                onClick={() => changePage(query.page - 1)}
                            >
                                上一页
                            </Button>
                            <Button
                                id="sales-inquiry-picker-next"
                                type="button"
                                variant="outline"
                                disabled={
                                    query.page * list.data.page_size >=
                                        list.data.total || list.isFetching
                                }
                                onClick={() => changePage(query.page + 1)}
                            >
                                下一页
                            </Button>
                        </div>
                    </div>
                )}
                {selected && (
                    <div
                        aria-live="polite"
                        className="rounded-lg border p-3 text-sm"
                    >
                        {basis.isFetching ? (
                            "正在核对已接受结果和剩余数量…"
                        ) : basis.isError ? (
                            <BusinessFailureState
                                idPrefix="sales-inquiry-picker-basis"
                                error={basis.error}
                                onRetry={() => void basis.refetch()}
                                retryLabel="重新核对"
                            />
                        ) : basis.data &&
                          customerId &&
                          basis.data.customer_id !== customerId ? (
                            <p role="alert" className="text-destructive">
                                询价客户与当前客户不一致，请重新选择。
                            </p>
                        ) : basis.data ? (
                            <p>
                                已选 {selected.inquiry_no}，将带入{" "}
                                {basis.data.lineItems.length}{" "}
                                条商品明细及对应配送信息。
                            </p>
                        ) : null}
                    </div>
                )}
                {discardsChanges && (
                    <p role="alert" className="text-sm text-destructive">
                        确认后将放弃当前页面已填内容和已选文件，按询价结果重新开单。取消选择可继续编辑当前内容。
                    </p>
                )}
                <DialogFooter>
                    <Button
                        id="sales-inquiry-picker-cancel"
                        type="button"
                        variant="outline"
                        onClick={onClose}
                    >
                        取消
                    </Button>
                    <Button
                        id="sales-inquiry-picker-confirm"
                        type="button"
                        disabled={!canConfirm}
                        onClick={() =>
                            canConfirm && onConfirm(selectedId, round)
                        }
                    >
                        {discardsChanges ? "放弃当前内容并带入" : "确认并带入"}
                    </Button>
                </DialogFooter>
            </DialogContent>
        </Dialog>
    )
}
