"use client"

import Link from "next/link"

import { FormalActionResult } from "@/components/business"
import { Button } from "@/components/ui/button"
import { Spinner } from "@/components/ui/spinner"
import type { FormalSubmitResult } from "@/features/supplier-payables/types"

export type AllocationResultViewProps = {
    result: FormalSubmitResult
    returnTo?: string
    hasSubmitKey: boolean
    pending: boolean
    closeLabel?: string
    onGoToInvoiceView?: () => void
    onClose: () => void
    onResolveUnknown: () => Promise<boolean>
}

/** 提交后结果区：结果反馈、按操作号复查与返回入口。 */
export function AllocationResultView({
    result,
    returnTo,
    hasSubmitKey,
    pending,
    closeLabel = "回到列表",
    onGoToInvoiceView,
    onClose,
    onResolveUnknown,
}: AllocationResultViewProps) {
    const showReturnToSource =
        Boolean(result.returnTo || returnTo) && result.status === "succeeded"

    return (
        <FormalActionResult
            status={
                result.status === "succeeded"
                    ? "succeeded"
                    : result.status === "unknown"
                      ? "unknown"
                      : result.status === "blocked"
                        ? "blocked"
                        : "rejected"
            }
            title={result.title}
            description={
                result.status === "blocked" && result.existingDocumentId
                    ? [
                          result.description,
                          "已定位既有发票，不创建副本。可切换到进项发票视图继续核销。",
                      ]
                          .filter(Boolean)
                          .join(" ")
                    : result.description
            }
            reference={result.reference ?? result.operationId}
            facts={result.facts}
            actions={
                <>
                    {result.status === "unknown" && hasSubmitKey ? (
                        <Button
                            id="supplier-payables-allocation-result-resolve"
                            type="button"
                            variant="outline"
                            size="sm"
                            disabled={pending}
                            onClick={() => void onResolveUnknown()}
                        >
                            {pending ? (
                                <Spinner
                                    className="size-4 animate-spin"
                                    aria-hidden="true"
                                />
                            ) : null}
                            {pending ? "查询中…" : "按操作号查询最终结果"}
                        </Button>
                    ) : null}
                    {result.status === "blocked" &&
                    result.existingDocumentId &&
                    onGoToInvoiceView ? (
                        <Button
                            id="supplier-payables-allocation-result-go-invoice"
                            type="button"
                            variant="outline"
                            size="sm"
                            onClick={onGoToInvoiceView}
                        >
                            前往进项发票视图
                        </Button>
                    ) : null}
                    <Button
                        id="supplier-payables-allocation-result-close"
                        type="button"
                        variant="outline"
                        size="sm"
                        onClick={onClose}
                    >
                        {closeLabel}
                    </Button>
                    {showReturnToSource ? (
                        <Button
                            id="supplier-payables-allocation-result-return-source"
                            type="button"
                            size="sm"
                            render={
                                <Link
                                    href={result.returnTo || returnTo || "/"}
                                />
                            }
                        >
                            返回来源并重新校验先款条件
                        </Button>
                    ) : null}
                </>
            }
        />
    )
}
