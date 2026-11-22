"use client"

import { ArrowLeftIcon } from "lucide-react"

import {
    BusinessEmptyState,
    BusinessFailureState,
    DetailWorkspaceBody,
    DetailWorkspaceChrome,
    DetailWorkspacePage,
    DetailWorkspaceSection,
    DetailWorkspaceSkeleton,
} from "@/components/business"
import { Button } from "@/components/ui/button"

function SettlementCenterLoading() {
    return (
        <DetailWorkspaceSkeleton
            title="供应商结算单"
            description="正在加载结算单，请稍候…"
            summary
        />
    )
}

function SettlementCenterError({
    error,
    onBack,
    onRetry,
}: {
    error: unknown
    onBack: () => void
    onRetry: () => void
}) {
    return (
        <DetailWorkspacePage>
            <DetailWorkspaceChrome title="供应商结算单" />
            <DetailWorkspaceBody>
                <DetailWorkspaceSection width="form">
                    <Button
                        id="supplier-settlements-center-error-back"
                        type="button"
                        variant="ghost"
                        size="sm"
                        onClick={onBack}
                    >
                        <ArrowLeftIcon className="size-4" />
                        返回列表
                    </Button>
                    <BusinessFailureState
                        title="结算单加载失败"
                        error={error}
                        action={
                            <Button
                                id="supplier-settlements-center-error-retry"
                                type="button"
                                onClick={onRetry}
                            >
                                重试
                            </Button>
                        }
                    />
                </DetailWorkspaceSection>
            </DetailWorkspaceBody>
        </DetailWorkspacePage>
    )
}

function SettlementCenterEmpty({ onBack }: { onBack: () => void }) {
    return (
        <DetailWorkspacePage>
            <DetailWorkspaceChrome title="供应商结算单" />
            <DetailWorkspaceBody>
                <DetailWorkspaceSection width="form">
                    <Button
                        id="supplier-settlements-center-empty-back"
                        type="button"
                        variant="ghost"
                        size="sm"
                        onClick={onBack}
                    >
                        <ArrowLeftIcon className="size-4" />
                        返回列表
                    </Button>
                    <BusinessEmptyState
                        kind="no-data"
                        className="rounded-lg border-0 bg-transparent p-6 shadow-none ring-0"
                        title="结算单不存在"
                        description="该结算单不存在或已被作废。可返回列表重新选择，或检查分享链接是否正确。"
                    />
                </DetailWorkspaceSection>
            </DetailWorkspaceBody>
        </DetailWorkspacePage>
    )
}

export { SettlementCenterEmpty, SettlementCenterError, SettlementCenterLoading }
