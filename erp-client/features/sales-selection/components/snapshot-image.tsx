"use client"

import * as React from "react"
import { useQuery } from "@tanstack/react-query"
import { apiGetBlob } from "@/lib/api"

/** 管理端快照只经当前用户的 Authorization 读取，不把文件身份当地址。 */
export function SnapshotImage({
    path,
    alt,
}: {
    path?: string | null
    alt: string
}) {
    const query = useQuery({
        queryKey: ["sales-selection-image", path],
        queryFn: () => apiGetBlob(path!, { cache: "no-store" }),
        enabled: Boolean(path?.startsWith("/admin/sales-selection-books/")),
        staleTime: 0,
        gcTime: 0,
        retry: false,
    })
    const [src, setSrc] = React.useState<string>()
    React.useEffect(() => {
        if (!query.data) {
            setSrc(undefined)
            return
        }
        const url = URL.createObjectURL(query.data)
        setSrc(url)
        return () => URL.revokeObjectURL(url)
    }, [query.data])
    if (!src)
        return (
            <div className="flex h-full items-center justify-center text-xs text-muted-foreground">
                {query.isError
                    ? "图片暂时无法读取"
                    : path
                      ? "正在读取图片…"
                      : "暂无图片"}
            </div>
        )
    // eslint-disable-next-line @next/next/no-img-element
    return <img src={src} alt={alt} className="h-full w-full object-cover" />
}
