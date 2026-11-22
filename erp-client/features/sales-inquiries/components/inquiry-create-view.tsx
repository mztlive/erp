"use client"

import * as React from "react"
import {
    BusinessFailureState,
    DetailWorkspaceBody,
    DetailWorkspaceChrome,
    DetailWorkspacePage,
    DetailWorkspaceSection,
    DetailWorkspaceSkeleton,
} from "@/components/business"
import { Alert, AlertDescription } from "@/components/ui/alert"
import { hasPermission } from "@/lib/permissions"
import { useInquiry } from "../hooks/queries"
import { resolveInquiryBackLink } from "../lib/navigation"
import { InquiryDemandEditor } from "./inquiry-demand-editor"

export function InquiryCreateView({
    copyFromId,
    returnTo,
    userId,
    permissions,
}: {
    copyFromId: string
    returnTo?: string | null
    userId: string
    permissions: readonly string[]
}) {
    const title = copyFromId ? "复制为新询价" : "新建销售询价"
    const back = resolveInquiryBackLink({
        id: "sales-inquiry-new-back",
        procurement: false,
        returnTo,
    })
    const copyQuery = useInquiry(
        copyFromId,
        false,
        undefined,
        Boolean(copyFromId),
    )
    const canCreate =
        hasPermission(permissions, "sales_inquiry:create") &&
        hasPermission(permissions, "sellable_sku:list")

    if (!canCreate) {
        return (
            <DetailWorkspacePage>
                <DetailWorkspaceChrome title={title} back={back} />
                <DetailWorkspaceBody>
                    <DetailWorkspaceSection width="form">
                        <Alert variant="destructive">
                            <AlertDescription>
                                当前账号没有新建询价或选择商品权限。
                            </AlertDescription>
                        </Alert>
                    </DetailWorkspaceSection>
                </DetailWorkspaceBody>
            </DetailWorkspacePage>
        )
    }

    if (copyFromId && copyQuery.isPending) {
        return (
            <DetailWorkspaceSkeleton
                title={title}
                description="正在读取原询价…"
            />
        )
    }

    if (copyFromId && copyQuery.isError) {
        return (
            <DetailWorkspacePage>
                <DetailWorkspaceChrome title={title} back={back} />
                <DetailWorkspaceBody>
                    <DetailWorkspaceSection width="form">
                        <BusinessFailureState
                            id="sales-inquiry-copy-retry"
                            title="原询价读取失败"
                            error={copyQuery.error}
                            onRetry={() => void copyQuery.refetch()}
                        />
                    </DetailWorkspaceSection>
                </DetailWorkspaceBody>
            </DetailWorkspacePage>
        )
    }

    return (
        <DetailWorkspacePage>
            <DetailWorkspaceChrome title={title} back={back} />
            <DetailWorkspaceBody
                alerts={
                    copyFromId ? (
                        <Alert variant="info">
                            <AlertDescription>
                                从原询价复制需求；请核对交付日期并填写新的询价截止日期。原报价、接受结果和转单占用不会带入。
                            </AlertDescription>
                        </Alert>
                    ) : undefined
                }
            >
                <DetailWorkspaceSection divider={false}>
                    <InquiryDemandEditor
                        copyFrom={
                            copyFromId &&
                            copyQuery.data &&
                            "sales_owner_user_id" in copyQuery.data
                                ? copyQuery.data
                                : undefined
                        }
                        userId={userId}
                        permissions={permissions}
                        onSaved={() => undefined}
                    />
                </DetailWorkspaceSection>
            </DetailWorkspaceBody>
        </DetailWorkspacePage>
    )
}
