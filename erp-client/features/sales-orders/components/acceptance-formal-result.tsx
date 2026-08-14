"use client"

import { FormalActionResult } from "@/components/business"
import { Button } from "@/components/ui/button"
import type { FormalResultState } from "@/features/sales-orders/lib/acceptance-model"

export function AcceptanceFormalResult({
    formalResult,
    resultRef,
    onDismiss,
    onRetry,
}: {
    formalResult: FormalResultState | null
    resultRef: React.Ref<HTMLDivElement>
    onDismiss: () => void
    /** 结果未知时按原提交编号重试（关闭结果、重新打开确认框）。 */
    onRetry: () => void
}) {
    if (!formalResult) return null

    return (
        <div ref={resultRef} tabIndex={-1} className="outline-none">
            <FormalActionResult
                status={
                    formalResult.status === "failed"
                        ? "rejected"
                        : formalResult.status === "unknown"
                          ? "unknown"
                          : "succeeded"
                }
                title={formalResult.title}
                description={formalResult.description}
                reference={formalResult.reference}
                facts={formalResult.facts}
                actions={
                    formalResult.status === "unknown" ? (
                        <Button
                            type="button"
                            size="sm"
                            variant="outline"
                            onClick={onRetry}
                        >
                            用原提交编号重试
                        </Button>
                    ) : (
                        <Button
                            type="button"
                            size="sm"
                            variant="outline"
                            onClick={onDismiss}
                        >
                            继续验收
                        </Button>
                    )
                }
            />
        </div>
    )
}
