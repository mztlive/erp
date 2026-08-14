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
    resultRef,
    onNext,
}: {
    lastResult: IntegrationFormalResult | null
    actionError: string | null
    autoNext: boolean
    resultRef: React.Ref<HTMLDivElement>
    onNext: () => void
}) {
    return (
        <>
            {lastResult ? (
                <div ref={resultRef} tabIndex={-1} className="outline-none">
                    <FormalActionResult
                        status={formalStatus(lastResult.status)}
                        title={lastResult.title}
                        description={lastResult.description}
                        reference={lastResult.reference}
                        referenceLabel="本次处理编号"
                        facts={lastResult.facts}
                        actions={
                            lastResult.terminal && !autoNext ? (
                                <Button
                                    type="button"
                                    size="sm"
                                    onClick={onNext}
                                >
                                    下一项
                                </Button>
                            ) : null
                        }
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
