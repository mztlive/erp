"use client"

import Link from "next/link"

import { FormalActionResult } from "@/components/business"
import { Button } from "@/components/ui/button"
import type { FormalSubmitResult } from "@/features/supplier-payables/types"

export type AllocationResultViewProps = {
    result: FormalSubmitResult
    returnTo?: string
    hasSubmitKey: boolean
    onGoToInvoiceView?: () => void
    onClose: () => void
    onResolveUnknown: () => Promise<boolean>
}

/** 提交后结果区：结果反馈、按操作号复查与返回入口。 */
export function AllocationResultView({
    result,
    returnTo,
    hasSubmitKey,
    onGoToInvoiceView,
    onClose,
    onResolveUnknown,
}: AllocationResultViewProps) {
    return (
        <div className="space-y-3">
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
                description={result.description}
                reference={result.reference ?? result.operationId}
                facts={result.facts}
            />
            {result.status === "unknown" && hasSubmitKey ? (
                <Button
                    type="button"
                    variant="outline"
                    onClick={() => void onResolveUnknown()}
                >
                    按操作号查询最终结果
                </Button>
            ) : null}
            {result.status === "blocked" && result.existingDocumentId ? (
                <div className="space-y-2">
                    <p className="text-sm text-muted-foreground">
                        已定位既有发票，不创建副本。可切换到进项发票视图继续核销。
                    </p>
                    {onGoToInvoiceView ? (
                        <Button
                            type="button"
                            variant="outline"
                            onClick={onGoToInvoiceView}
                        >
                            前往进项发票视图
                        </Button>
                    ) : null}
                </div>
            ) : null}
            <div className="flex flex-wrap gap-2">
                <Button type="button" variant="outline" onClick={onClose}>
                    回到列表
                </Button>
                {(result.returnTo || returnTo) &&
                result.status === "succeeded" ? (
                    <Button
                        type="button"
                        render={
                            <Link href={result.returnTo || returnTo || "/"} />
                        }
                    >
                        返回来源并重新校验先款条件
                    </Button>
                ) : null}
            </div>
        </div>
    )
}
