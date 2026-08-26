"use client"

import * as React from "react"
import Link from "next/link"

import {
    BusinessFailureState,
    PageHeader,
    PageScaffold,
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
    detailQuery: QueryLike
    accountQuery: QueryLike
    canCreate: boolean
    listHref: string
    children: React.ReactNode
}

/**
 * 商品详情页的加载 / 失败 / 无权限中间态；
 * 就绪时渲染 children（编辑表单）。
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
            <PageScaffold density="compact">
                <PageHeader
                    title="商品详情"
                    description={masterDataCopy.centerLoading}
                />
                <div
                    className="h-40 animate-pulse rounded-lg bg-muted"
                    aria-busy
                />
            </PageScaffold>
        )
    }

    if (!isCreate && (detailQuery.isError || !hasDetailData)) {
        return (
            <PageScaffold density="compact">
                <PageHeader title="商品详情" />
                <BusinessFailureState
                    error={detailQuery.isError ? detailQuery.error : undefined}
                    description={
                        detailQuery.isError
                            ? masterDataCopy.centerLoadFail
                            : masterDataCopy.centerMissingDesc
                    }
                    action={
                        detailQuery.isError ? (
                            <Button
                                type="button"
                                onClick={() => void detailQuery.refetch()}
                            >
                                重试
                            </Button>
                        ) : (
                            <Button render={<Link href={listHref} />}>
                                {masterDataCopy.actionBackList}
                            </Button>
                        )
                    }
                />
            </PageScaffold>
        )
    }

    if (isCreate && accountQuery.isPending) {
        return (
            <PageScaffold density="compact">
                <PageHeader title="新建商品" description="正在核对创建权限" />
                <div
                    className="h-40 animate-pulse rounded-lg bg-muted"
                    aria-busy
                />
            </PageScaffold>
        )
    }

    if (isCreate && accountQuery.isError) {
        return (
            <PageScaffold density="compact">
                <PageHeader title="新建商品" />
                <BusinessFailureState
                    error={accountQuery.error}
                    onRetry={() => void accountQuery.refetch()}
                />
            </PageScaffold>
        )
    }

    if (isCreate && !canCreate) {
        return (
            <PageScaffold density="compact">
                <PageHeader title="新建商品" />
                <BusinessFailureState
                    kind="permission"
                    description="当前账号没有创建商品的权限，请联系管理员或有权限的同事。"
                    action={
                        <Button render={<Link href={listHref} />}>
                            返回列表
                        </Button>
                    }
                />
            </PageScaffold>
        )
    }

    return <>{children}</>
}

export { ProductDetailEntryGate }
