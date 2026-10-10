"use client"

import { Button } from "@/components/ui/button"
import { Spinner } from "@/components/ui/spinner"

export function SalesOrderCreateContractLoading({
    failed,
    imported,
    revisionUnavailable,
    fetching,
    canChange,
    onRetry,
    onChange,
}: {
    failed: boolean
    imported: boolean
    revisionUnavailable: boolean
    fetching: boolean
    canChange: boolean
    onRetry: () => void
    onChange: () => void
}) {
    return (
        <section
            className="space-y-4 rounded-xl border border-grid bg-card p-6"
            aria-labelledby="sales-contract-handoff-title"
            aria-live="polite"
        >
            <h2
                id="sales-contract-handoff-title"
                className="flex items-center gap-2 font-semibold"
            >
                {!failed ? <Spinner /> : null}
                {failed
                    ? "开单信息加载失败"
                    : imported
                      ? "合同已归档，正在带入开单信息"
                      : "正在加载合同信息"}
            </h2>
            <p className="text-sm text-muted-foreground">
                {failed
                    ? imported
                        ? "合同已经归档，无需重新上传。请重试加载开单信息。"
                        : "暂时无法读取所选合同，请重试或更换合同。"
                    : "正在读取客户、结算主体与合同条款，请稍候。"}
            </p>
            {revisionUnavailable ? (
                <p role="alert" className="text-sm text-destructive">
                    所选合同版本不存在或已更新，请重新加载或更换有效合同。
                </p>
            ) : null}
            {failed ? (
                <div className="flex flex-wrap gap-2">
                    <Button
                        id="sales-contract-handoff-retry"
                        type="button"
                        variant="outline"
                        disabled={fetching}
                        onClick={onRetry}
                    >
                        {fetching ? <Spinner /> : null}
                        重新加载
                    </Button>
                    {canChange ? (
                        <Button
                            id="sales-contract-handoff-change"
                            type="button"
                            variant="ghost"
                            onClick={onChange}
                        >
                            更换合同
                        </Button>
                    ) : null}
                </div>
            ) : null}
        </section>
    )
}
