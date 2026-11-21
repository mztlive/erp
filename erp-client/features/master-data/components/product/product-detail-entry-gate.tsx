"use client"

import * as React from "react"
import Link from "next/link"

import {
    BusinessFailureState,
    DetailWorkspaceBody,
    DetailWorkspaceChrome,
    DetailWorkspacePage,
    DetailWorkspaceSkeleton,
} from "@/components/business"
import { Button } from "@/components/ui/button"
import { masterDataCopy } from "@/features/master-data/lib/copy"

type QueryLike = {
    isPending: boolean
    isError: boolean
    error: unknown
    refetch: () => void | Promise<unknown>
}

type ProductDetailEntryGateProps = {
    isCreate: boolean
    hasDetailData: boolean
    detailQuery: QueryLike & {
        data?: { loadWarnings?: readonly string[] } | null
    }
    accountQuery: QueryLike
    canCreate: boolean
    listHref: string
    children: React.ReactNode
}

/**
 * 商品详情页的加载 / 失败 / 无权限中间态；
 * 统一使用纯白通栏详情页壳，避免加载期间闪现浅灰背景。
 */
function ProductDetailEntryGate({
    isCreate,
    hasDetailData,
    detailQuery,
    accountQuery,
    canCreate,
    listHref,
    children,
}: ProductDetailEntryGateProps) {
    if (!isCreate && detailQuery.isPending) {
        return (
            <DetailWorkspaceSkeleton
                title="商品详情"
                description={masterDataCopy.centerLoading}
            />
        )
    }

    if (!isCreate && (detailQuery.isError || !hasDetailData)) {
        return (
            <DetailWorkspacePage>
                <DetailWorkspaceChrome
                    title="商品详情"
                    back={{
                        id: "master-data-product-detail-gate-back",
                        label: "商品列表",
                        href: listHref,
                    }}
                />
                <DetailWorkspaceBody>
                    <div className="py-6">
                        <BusinessFailureState
                            error={
                                detailQuery.isError
                                    ? detailQuery.error
                                    : undefined
                            }
                            description={
                                detailQuery.isError
                                    ? masterDataCopy.centerLoadFail
                                    : masterDataCopy.centerMissingDesc
                            }
                            action={
                                detailQuery.isError ? (
                                    <Button
                                        id="master-data-product-product-detail-entry-gate-button-1"
                                        type="button"
                                        onClick={() =>
                                            void detailQuery.refetch()
                                        }
                                    >
                                        重试
                                    </Button>
                                ) : (
                                    <Button
                                        id="master-data-product-product-detail-entry-gate-button-2"
                                        render={<Link href={listHref} />}
                                    >
                                        {masterDataCopy.actionBackList}
                                    </Button>
                                )
                            }
                        />
                    </div>
                </DetailWorkspaceBody>
            </DetailWorkspacePage>
        )
    }

    if (isCreate && accountQuery.isPending) {
        return (
            <DetailWorkspaceSkeleton
                title="新建商品"
                description="正在核对创建权限"
            />
        )
    }

    if (isCreate && accountQuery.isError) {
        return (
            <DetailWorkspacePage>
                <DetailWorkspaceChrome
                    title="新建商品"
                    back={{
                        id: "master-data-product-detail-gate-back",
                        label: "商品列表",
                        href: listHref,
                    }}
                />
                <DetailWorkspaceBody>
                    <div className="py-6">
                        <BusinessFailureState
                            error={accountQuery.error}
                            onRetry={() => void accountQuery.refetch()}
                        />
                    </div>
                </DetailWorkspaceBody>
            </DetailWorkspacePage>
        )
    }

    if (isCreate && !canCreate) {
        return (
            <DetailWorkspacePage>
                <DetailWorkspaceChrome
                    title="新建商品"
                    back={{
                        id: "master-data-product-detail-gate-back",
                        label: "商品列表",
                        href: listHref,
                    }}
                />
                <DetailWorkspaceBody>
                    <div className="py-6">
                        <BusinessFailureState
                            kind="permission"
                            description="当前账号没有创建商品的权限，请联系管理员或有权限的同事。"
                            action={
                                <Button
                                    id="master-data-product-product-detail-entry-gate-button-3"
                                    render={<Link href={listHref} />}
                                >
                                    返回列表
                                </Button>
                            }
                        />
                    </div>
                </DetailWorkspaceBody>
            </DetailWorkspacePage>
        )
    }

    return (
        <>
            {detailQuery.data?.loadWarnings?.map((message) => (
                <BusinessFailureState
                    key={message}
                    kind="business"
                    title="部分资料读取失败"
                    description={message}
                />
            ))}
            {children}
        </>
    )
}

export { ProductDetailEntryGate }
