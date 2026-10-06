import { Suspense } from "react"
export default function SupplierPortalLayout({
    children,
}: {
    children: React.ReactNode
}) {
    return (
        <Suspense fallback={<div className="p-8">加载中…</div>}>
            {children}
        </Suspense>
    )
}
