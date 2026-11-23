"use client"

import {
    DetailWorkspaceSection,
    detailWorkspaceStyles,
} from "@/components/business"
import { Button } from "@/components/ui/button"
import { Spinner } from "@/components/ui/spinner"

export function SalesOrderCreateContractLoading({
    customerMismatch = false,
    failed,
    imported,
    revisionUnavailable,
    fetching,
    canChange,
    onRetry,
    onChange,
}: {
    customerMismatch?: boolean
    failed: boolean
    imported: boolean
    revisionUnavailable: boolean
    fetching: boolean
    canChange: boolean
    onRetry: () => void
    onChange: () => void
}) {
    return (
        <DetailWorkspaceSection
            width="form"
            headingId="sales-contract-handoff-title"
            title={
                <span className="inline-flex items-center gap-2">
                    {!failed ? <Spinner /> : null}
                    {failed
                        ? "开单信息加载失败"
                        : imported
                          ? "合同已归档，正在带入开单信息"
                          : "正在加载合同信息"}
                </span>
            }
        >
            <div className="space-y-3" aria-live="polite">
                <p className="text-xs text-muted-foreground">
                    {failed
                        ? imported
                            ? "合同已经归档，无需重新上传。请重试加载开单信息。"
                            : "暂时无法读取所选合同，请重试或更换合同。"
                        : "正在读取客户、结算主体与合同条款，请稍候。"}
                </p>
                {revisionUnavailable ? (
                    <p
                        role="alert"
                        className="text-body-compact text-destructive"
                    >
                        {customerMismatch
                            ? "所选合同的客户与销售方案不一致，请更换同客户的有效合同。"
                            : "所选合同版本不存在或已更新，请重新加载或更换有效合同。"}
                    </p>
                ) : null}
                {failed ? (
                    <div className="flex flex-wrap items-center gap-2 pt-1">
                        <Button
                            id="sales-contract-handoff-retry"
                            type="button"
                            variant="outline"
                            size="sm"
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
                                size="sm"
                                className={detailWorkspaceStyles.quietButton}
                                onClick={onChange}
                            >
                                更换合同
                            </Button>
                        ) : null}
                    </div>
                ) : null}
            </div>
        </DetailWorkspaceSection>
    )
}
