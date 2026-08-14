"use client"

import { FormalActionResult } from "@/components/business"
import type { ResultState } from "@/components/business/feedback"
import { Button } from "@/components/ui/button"

interface AdjustmentResultBannerProps {
    result: NonNullable<ResultState>
    isResolving: boolean
    onResolve: () => void
}

export function AdjustmentResultBanner({
    result,
    isResolving,
    onResolve,
}: AdjustmentResultBannerProps) {
    return (
        <FormalActionResult
            status={
                result.status === "succeeded"
                    ? "succeeded"
                    : result.status === "unknown"
                      ? "unknown"
                      : "blocked"
            }
            title={result.title}
            description={result.description}
            reference={result.reference}
            referenceLabel={
                result.status === "unknown" ? "原任务号" : undefined
            }
            actions={
                result.pendingIdempotencyKey ? (
                    <div className="flex flex-wrap gap-2">
                        <Button
                            type="button"
                            size="sm"
                            variant="outline"
                            disabled={isResolving}
                            onClick={() => {
                                void onResolve()
                            }}
                        >
                            查询最终结果
                        </Button>
                    </div>
                ) : undefined
            }
        />
    )
}
