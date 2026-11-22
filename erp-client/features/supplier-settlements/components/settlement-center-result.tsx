"use client"

import * as React from "react"
import Link from "next/link"
import { ExternalLinkIcon } from "lucide-react"

import { FormalActionResult } from "@/components/business"
import type { ResultState } from "@/components/business/feedback"
import { Button } from "@/components/ui/button"

function SettlementCenterResultPanel({
    result,
    resultRef,
    onClose,
}: {
    result: ResultState
    resultRef: React.RefObject<HTMLDivElement | null>
    onClose?: () => void
}) {
    return (
        <div ref={resultRef} tabIndex={-1} className="outline-none">
            {result ? (
                <FormalActionResult
                    status={
                        result.status === "failed" ? "blocked" : result.status
                    }
                    title={result.title}
                    description={result.description}
                    reference={result.reference}
                    facts={result.facts}
                    actions={
                        <div className="flex flex-wrap gap-2">
                            {onClose ? (
                                <Button
                                    id="supplier-settlements-center-result-close"
                                    type="button"
                                    size="sm"
                                    variant="outline"
                                    onClick={onClose}
                                >
                                    关闭结果
                                </Button>
                            ) : null}
                            {result.w12Href ? (
                                <Button
                                    id="supplier-settlements-center-result-w12"
                                    type="button"
                                    size="sm"
                                    render={<Link href={result.w12Href} />}
                                >
                                    去供应商往来处理应付
                                    <ExternalLinkIcon
                                        data-icon="inline-end"
                                        className="size-3.5"
                                        aria-hidden="true"
                                    />
                                </Button>
                            ) : null}
                        </div>
                    }
                />
            ) : null}
        </div>
    )
}

export { SettlementCenterResultPanel }
