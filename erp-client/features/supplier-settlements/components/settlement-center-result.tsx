"use client"

import * as React from "react"
import Link from "next/link"
import { ExternalLinkIcon } from "lucide-react"

import { FormalActionResult } from "@/components/business"
import type { ResultState } from "@/components/business/feedback"
import { Button } from "@/components/ui/button"
import { LoadingButton } from "@/components/ui/loading-button"

function SettlementCenterResultPanel({
    result,
    resultRef,
    onClose,
    idPrefix = "supplier-settlements-center-result",
    onRefresh,
    refreshing,
    onRetry,
    retrying,
}: {
    result: ResultState
    resultRef: React.RefObject<HTMLDivElement | null>
    onClose?: () => void
    idPrefix?: string
    onRefresh?: () => void
    refreshing?: boolean
    onRetry?: () => void
    retrying?: boolean
}) {
    return (
        <div
            id={idPrefix}
            ref={resultRef}
            tabIndex={-1}
            className="outline-none empty:hidden"
        >
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
                            {onRefresh && (
                                <LoadingButton
                                    id={`${idPrefix}-refresh`}
                                    size="sm"
                                    variant="outline"
                                    loading={refreshing}
                                    disabled={retrying}
                                    onClick={onRefresh}
                                >
                                    刷新核对
                                </LoadingButton>
                            )}
                            {onRetry && (
                                <LoadingButton
                                    id={`${idPrefix}-retry`}
                                    size="sm"
                                    variant="outline"
                                    loading={retrying}
                                    disabled={refreshing}
                                    onClick={onRetry}
                                >
                                    按原请求重试
                                </LoadingButton>
                            )}
                            {onClose ? (
                                <Button
                                    id={`${idPrefix}-close`}
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
                                    id={`${idPrefix}-w12`}
                                    nativeButton={false}
                                    size="sm"
                                    variant="outline"
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
