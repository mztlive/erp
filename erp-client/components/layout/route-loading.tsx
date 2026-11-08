/** 路由壳的 Suspense 占位。文案由各页传入，结构保持一致。 */
export function RouteLoading({ label }: { label: string }) {
    return <div className="p-5 text-sm text-muted-foreground">{label}</div>
}
