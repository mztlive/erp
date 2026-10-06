"use client"
import Image from "next/image"
import { useEffect, useState } from "react"
import { type PortalFileSource } from "../api"
import { usePortalFile } from "../hooks/queries"

/** 图片字节按当前来源授权读取；对象地址仅在浏览器内临时持有。 */
export function PortalImage({
    assetId,
    source,
    alt,
    className = "h-14 w-14 rounded-md object-contain",
}: {
    assetId?: string | null
    source: PortalFileSource
    alt: string
    className?: string
}) {
    const query = usePortalFile(assetId, source)
    const [url, setUrl] = useState<string | null>(null)
    useEffect(() => {
        if (!query.data) return
        const next = URL.createObjectURL(query.data)
        setUrl(next)
        return () => {
            URL.revokeObjectURL(next)
            setUrl(null)
        }
    }, [query.data])
    if (!assetId)
        return <span className="text-xs text-muted-foreground">暂无图片</span>
    if (!url)
        return (
            <span className="text-xs text-muted-foreground">
                {query.isError ? "图片暂不可用" : "图片读取中"}
            </span>
        )
    return (
        <Image
            src={url}
            alt={alt}
            width={144}
            height={144}
            unoptimized
            className={className}
        />
    )
}
