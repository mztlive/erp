import { PortalSession } from "@/features/supplier-portal/components/portal-session"
export default function PortalLayout({
    children,
}: {
    children: React.ReactNode
}) {
    return <PortalSession>{children}</PortalSession>
}
