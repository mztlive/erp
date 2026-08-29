"use client"

import { FormalActionResult } from "@/components/business"
import { Button } from "@/components/ui/button"
import type { FormalResultState } from "@/features/sales-orders/lib/acceptance-model"
import { resultText } from "@/lib/ui-text"

export function AcceptanceFormalResult({
    formalResult,
    resultRef,
    onDismiss,
    onRetry,
}: {
    formalResult: FormalResultState | null
    resultRef: React.Ref<HTMLDivElement>
    onDismiss: () => void
    onRetry: () => void
}) {
    if (!formalResult) return null

    const succeeded =
        formalResult.status === "succeeded" && formalResult.kind === "post"
    const exceptionCta = succeeded && formalResult.hasException

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
                            {resultText.useOriginalTaskNoRetry}
                        </Button>
                    ) : (
                        <Button
                            type="button"
                            size="sm"
                            variant={exceptionCta ? "outline" : "secondary"}
                            onClick={onDismiss}
                        >
                            {exceptionCta
                                ? "先看本单进度"
                                : succeeded &&
                                    formalResult.remainingEligibleCount === 0
                                  ? "查看本单进度"
                                  : "继续看本单进度"}
                        </Button>
                    )
                }
            />
        </div>
    )
}
