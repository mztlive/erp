import * as React from "react"
import { FormalActionResult } from "@/components/business"
import { Alert, AlertDescription, AlertTitle } from "@/components/ui/alert"
import { Button } from "@/components/ui/button"
import type { IntegrationFormalResult } from "../../types"
import { formalStatus } from "../lib/helpers"

export function IntegrationActionResult({
    lastResult,
    actionError,
    autoNext,
    hasNext,
    resultRef,
    onNext,
    onBackToQueue,
    detailWorkspace,
}: {
    lastResult: IntegrationFormalResult | null
    actionError: string | null
    autoNext: boolean
    hasNext?: boolean
    resultRef: React.Ref<HTMLDivElement>
    onNext: () => void
    onBackToQueue?: () => void
    detailWorkspace?: boolean
}) {
    const terminalAction =
        lastResult?.terminal && !autoNext ? (
            hasNext ? (
                <Button
                    id="integration-action-result-next"
                    type="button"
                    size="sm"
                    onClick={onNext}
                >
                    下一项
                </Button>
            ) : onBackToQueue ? (
                <Button
                    id="integration-action-result-back-to-queue"
                    type="button"
                    size="sm"
                    variant="outline"
                    onClick={onBackToQueue}
                >
                    返回队列
                </Button>
            ) : null
        ) : null

    return (
        <>
            {lastResult ? (
                <div
                    ref={resultRef}
                    tabIndex={-1}
                    className={
                        detailWorkspace
                            ? "scroll-mt-32 outline-none"
                            : "outline-none"
                    }
                >
                    <FormalActionResult
                        status={formalStatus(lastResult.status)}
                        title={lastResult.title}
                        description={lastResult.description}
                        facts={lastResult.facts}
                        actions={terminalAction}
                    />
                </div>
            ) : null}

            {actionError ? (
                <Alert variant="destructive">
                    <AlertTitle>操作失败</AlertTitle>
                    <AlertDescription>{actionError}</AlertDescription>
                </Alert>
            ) : null}
        </>
    )
}
