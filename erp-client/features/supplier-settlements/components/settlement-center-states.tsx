"use client"

import {
    BusinessEmptyState,
    BusinessFailureState,
    DetailWorkspaceBody,
    DetailWorkspaceChrome,
    DetailWorkspacePage,
    DetailWorkspaceSection,
    DetailWorkspaceSkeleton,
    type DetailWorkspaceBackLink,
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

function SettlementCenterForbidden({
    onBack,
    backLabel = "返回对账单列表",
    backVisibility = "standalone-only",
    embedded = false,
}: {
    onBack: () => void
    backLabel?: string
    backVisibility?: DetailWorkspaceBackLink["visibility"]
    embedded?: boolean
}) {
    return (
        <DetailWorkspacePage>
            <DetailWorkspaceChrome
                title="供应商结算单"
                back={
                    embedded
                        ? undefined
                        : {
                              id: "supplier-settlements-center-forbidden-back",
                              label: backLabel,
                              visibility: backVisibility,
                              onClick: onBack,
                          }
                }
            />
            <DetailWorkspaceBody>
                <DetailWorkspaceSection width="form">
                    <BusinessFailureState
                        kind="permission"
                        title="权限不足"
                        description="当前账号没有查看供应商结算单详情的权限，请联系管理员开通。"
                    />
                </DetailWorkspaceSection>
            </DetailWorkspaceBody>
        </DetailWorkspacePage>
    )
}

function SettlementCenterError({
    error,
    onBack,
    backLabel = "返回对账单列表",
    backVisibility = "standalone-only",
    embedded = false,
    onRetry,
}: {
    error: unknown
    onBack: () => void
    backLabel?: string
    backVisibility?: DetailWorkspaceBackLink["visibility"]
    embedded?: boolean
    onRetry: () => void
}) {
    return (
        <DetailWorkspacePage>
            <DetailWorkspaceChrome
                title="供应商结算单"
                back={
                    embedded
                        ? undefined
                        : {
                              id: "supplier-settlements-center-error-back",
                              label: backLabel,
                              visibility: backVisibility,
                              onClick: onBack,
                          }
                }
            />
            <DetailWorkspaceBody>
                <DetailWorkspaceSection width="form">
                    <BusinessFailureState
                        idPrefix="supplier-settlements-center-error"
                        title="结算单加载失败"
                        error={error}
                        action={
                            <Button
                                id="supplier-settlements-center-error-retry"
                                type="button"
                                variant="outline"
                                size="sm"
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

function SettlementCenterEmpty({
    onBack,
    backLabel = "返回对账单列表",
    backVisibility = "standalone-only",
    embedded = false,
}: {
    onBack: () => void
    backLabel?: string
    backVisibility?: DetailWorkspaceBackLink["visibility"]
    embedded?: boolean
}) {
    return (
        <DetailWorkspacePage>
            <DetailWorkspaceChrome
                title="供应商结算单"
                back={
                    embedded
                        ? undefined
                        : {
                              id: "supplier-settlements-center-empty-back",
                              label: backLabel,
                              visibility: backVisibility,
                              onClick: onBack,
                          }
                }
            />
            <DetailWorkspaceBody>
                <DetailWorkspaceSection width="form">
                    <BusinessEmptyState
                        kind="no-data"
                        title="结算单不存在"
                        description="该结算单不存在或已被作废。可返回列表重新选择，或检查分享链接是否正确。"
                        action={
                            !embedded ? (
                                <Button
                                    id="supplier-settlements-center-empty-return"
                                    type="button"
                                    variant="outline"
                                    size="sm"
                                    onClick={onBack}
                                >
                                    {backLabel}
                                </Button>
                            ) : undefined
                        }
                    />
                </DetailWorkspaceSection>
            </DetailWorkspaceBody>
        </DetailWorkspacePage>
    )
}

export {
    SettlementCenterEmpty,
    SettlementCenterError,
    SettlementCenterForbidden,
    SettlementCenterLoading,
}
